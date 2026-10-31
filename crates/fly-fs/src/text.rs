//! 文本文件读写：BOM/编码检测、行尾检测与还原、二进制识别、原子保存。
//!
//! 编辑器内部的文本统一规范为 `\n`（CodeMirror 语义），文件原始 EOL 在
//! 读取时检测、保存时还原——与 VS Code 的处理方式一致，避免 CRLF 文件
//! 被「顺带」改写行尾。
//!
//! 大文件分级复用 [`crate::size_tier`]：调用方传入上限字节，超出返回
//! [`TextError::TooLarge`]，10–50MB 的只读降级由前端根据 tier 处理。

use std::fs;
use std::io::Write;
use std::path::Path;

use crate::SizeTier;

/// 行尾风格。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Eol {
    Lf,
    Crlf,
    Cr,
}

impl Eol {
    /// 状态栏/DTO 显示用。
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Lf => "LF",
            Self::Crlf => "CRLF",
            Self::Cr => "CR",
        }
    }

    const fn separator(self) -> &'static str {
        match self {
            Self::Lf => "\n",
            Self::Crlf => "\r\n",
            Self::Cr => "\r",
        }
    }

    fn from_str(value: &str) -> Option<Self> {
        match value {
            "LF" => Some(Self::Lf),
            "CRLF" => Some(Self::Crlf),
            "CR" => Some(Self::Cr),
            _ => None,
        }
    }
}

/// 读取/保存支持的文本编码（其余编码 Phase 1 后续接入 chardetng）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextEncoding {
    Utf8,
    Utf16Le,
    Utf16Be,
}

impl TextEncoding {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Utf8 => "UTF-8",
            Self::Utf16Le => "UTF-16LE",
            Self::Utf16Be => "UTF-16BE",
        }
    }

    const fn bom(self) -> &'static [u8] {
        match self {
            Self::Utf8 => &[0xEF, 0xBB, 0xBF],
            Self::Utf16Le => &[0xFF, 0xFE],
            Self::Utf16Be => &[0xFE, 0xFF],
        }
    }

    /// 解码（不含 BOM 的字节序列，UTF-8 非法序列按 lossy 替换）。
    fn decode(self, bytes: &[u8]) -> String {
        match self {
            Self::Utf8 => String::from_utf8_lossy(bytes).into_owned(),
            Self::Utf16Le => String::from_utf16_lossy(
                &bytes
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|pair| u16::from_le_bytes(*pair))
                    .collect::<Vec<u16>>(),
            ),
            Self::Utf16Be => String::from_utf16_lossy(
                &bytes
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|pair| u16::from_be_bytes(*pair))
                    .collect::<Vec<u16>>(),
            ),
        }
    }

    /// 编码。注意不能用 encoding_rs 的 `encode()`：WHATWG 规范将
    /// UTF-16LE/BE 定义为仅解码编码，其 encoder 会退化为 UTF-8。
    fn encode(self, text: &str) -> Vec<u8> {
        match self {
            Self::Utf8 => text.as_bytes().to_vec(),
            Self::Utf16Le => text
                .encode_utf16()
                .flat_map(|unit| unit.to_le_bytes())
                .collect(),
            Self::Utf16Be => text
                .encode_utf16()
                .flat_map(|unit| unit.to_be_bytes())
                .collect(),
        }
    }

    fn from_str(value: &str) -> Option<Self> {
        match value {
            "UTF-8" => Some(Self::Utf8),
            "UTF-16LE" => Some(Self::Utf16Le),
            "UTF-16BE" => Some(Self::Utf16Be),
            _ => None,
        }
    }
}

/// 一个已解码的文本文件。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextFile {
    /// 已规范为 `\n` 行尾的文本；二进制文件为空。
    pub text: String,
    pub encoding: TextEncoding,
    pub bom: bool,
    pub eol: Eol,
    pub size: u64,
    pub tier: SizeTier,
    /// 含 NUL 字节判定为二进制，前端拒绝按文本打开。
    pub binary: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum TextError {
    #[error("文件过大：{size} 字节，超过 {limit} 字节上限，不加载全文")]
    TooLarge { size: u64, limit: u64 },
    #[error("IO 错误：{0}")]
    Io(#[from] std::io::Error),
    #[error("非法参数：{0}")]
    InvalidArgument(String),
}

const BOM_UTF8: &[u8] = &[0xEF, 0xBB, 0xBF];
const BOM_UTF16LE: &[u8] = &[0xFF, 0xFE];
const BOM_UTF16BE: &[u8] = &[0xFE, 0xFF];

/// 检测 BOM，返回（编码，BOM 字节数）。无 BOM 返回 UTF-8 + 0。
fn detect_bom(bytes: &[u8]) -> (TextEncoding, usize) {
    if bytes.starts_with(BOM_UTF8) {
        (TextEncoding::Utf8, 3)
    } else if bytes.starts_with(BOM_UTF16LE) {
        (TextEncoding::Utf16Le, 2)
    } else if bytes.starts_with(BOM_UTF16BE) {
        (TextEncoding::Utf16Be, 2)
    } else {
        (TextEncoding::Utf8, 0)
    }
}

/// 前 8KB 含 NUL 即判定二进制。
pub fn is_binary(bytes: &[u8]) -> bool {
    bytes.iter().take(8192).any(|&b| b == 0)
}

/// 在原始（未规范）文本上检测行尾：取第一个行终止符。
pub fn detect_eol(text: &str) -> Eol {
    let bytes = text.as_bytes();
    for (i, &b) in bytes.iter().enumerate() {
        match b {
            b'\n' => {
                return if i > 0 && bytes[i - 1] == b'\r' {
                    Eol::Crlf
                } else {
                    Eol::Lf
                };
            }
            b'\r' => {
                return if bytes.get(i + 1) == Some(&b'\n') {
                    Eol::Crlf
                } else {
                    Eol::Cr
                };
            }
            _ => {}
        }
    }
    Eol::Lf
}

/// 规范为 `\n` 行尾（编辑器内部表示）。
pub fn normalize_eol(text: &str) -> String {
    if !text.contains('\r') {
        return text.to_string();
    }
    text.replace("\r\n", "\n").replace('\r', "\n")
}

fn apply_eol(text: &str, eol: Eol) -> String {
    match eol {
        Eol::Lf => text.to_string(),
        Eol::Crlf | Eol::Cr => text.replace('\n', eol.separator()),
    }
}

/// 读取并解码文本文件。`max_bytes` 防御超大文件，超出返回
/// [`TextError::TooLarge`] 而不读入内存。
pub fn read_text_file(path: &Path, max_bytes: u64) -> Result<TextFile, TextError> {
    let metadata = fs::metadata(path).map_err(TextError::Io)?;
    let size = metadata.len();
    let tier = SizeTier::from_size(size);
    if size > max_bytes {
        return Err(TextError::TooLarge {
            size,
            limit: max_bytes,
        });
    }

    let bytes = fs::read(path).map_err(TextError::Io)?;
    let (encoding, bom_len) = detect_bom(&bytes);
    // BOM 明确声明了编码，视为文本——UTF-16 文件天然含 NUL 字节，
    // 不能用 NUL 探测判定二进制；无 BOM 才用 NUL 探测。
    if bom_len == 0 && is_binary(&bytes) {
        return Ok(TextFile {
            text: String::new(),
            encoding: TextEncoding::Utf8,
            bom: false,
            eol: Eol::Lf,
            size,
            tier,
            binary: true,
        });
    }

    let raw = encoding.decode(&bytes[bom_len..]);
    let eol = detect_eol(&raw);
    let text = normalize_eol(&raw);

    Ok(TextFile {
        text,
        encoding,
        bom: bom_len > 0,
        eol,
        size,
        tier,
        binary: false,
    })
}

/// 原子保存：同目录写临时文件 + rename 替换。
///
/// `text` 必须是已规范为 `\n` 的文本；本函数负责按 `eol` 还原、
/// 按 `encoding`/`bom` 编码。返回写入字节数。
pub fn write_text_file(
    path: &Path,
    text: &str,
    eol: Eol,
    encoding: TextEncoding,
    bom: bool,
) -> Result<u64, TextError> {
    let content = apply_eol(text, eol);
    let mut bytes = Vec::with_capacity(content.len() + 4);
    if bom {
        bytes.extend_from_slice(encoding.bom());
    }
    bytes.extend_from_slice(&encoding.encode(&content));

    let dir = path
        .parent()
        .ok_or_else(|| TextError::InvalidArgument(format!("路径缺少父目录：{}", path.display())))?;
    let file_name = path
        .file_name()
        .ok_or_else(|| TextError::InvalidArgument(format!("路径缺少文件名：{}", path.display())))?;
    let mut temp = dir.to_path_buf();
    temp.push(format!(
        ".flyide-{}-{}-{}.tmp",
        file_name.to_string_lossy(),
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.subsec_nanos())
            .unwrap_or(0)
    ));

    let write_result = (|| -> std::io::Result<()> {
        let mut file = fs::File::create(&temp)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        drop(file);
        // Windows 上 rename 会替换已存在目标（MoveFileEx REPLACE_EXISTING）
        fs::rename(&temp, path)?;
        Ok(())
    })();
    if let Err(err) = write_result {
        // 清理残留临时文件，保留原始错误
        let _ = fs::remove_file(&temp);
        return Err(TextError::Io(err));
    }
    Ok(bytes.len() as u64)
}

/// 解析 DTO 字符串为枚举（供命令层使用）。
pub fn parse_eol(value: &str) -> Result<Eol, TextError> {
    Eol::from_str(value).ok_or_else(|| TextError::InvalidArgument(format!("未知行尾：{value}")))
}

/// 解析 DTO 字符串为编码枚举。
pub fn parse_encoding(value: &str) -> Result<TextEncoding, TextError> {
    TextEncoding::from_str(value)
        .ok_or_else(|| TextError::InvalidArgument(format!("未知编码：{value}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    static COUNTER: AtomicU32 = AtomicU32::new(0);

    struct TempDir(PathBuf);
    impl TempDir {
        fn new() -> Self {
            let dir = std::env::temp_dir().join(format!(
                "fly-fs-text-{}-{}",
                std::process::id(),
                COUNTER.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(&dir).expect("创建临时目录");
            Self(dir)
        }
        fn path(&self, name: &str) -> PathBuf {
            self.0.join(name)
        }
    }
    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    use std::path::PathBuf;

    #[test]
    fn eol_detection_covers_all_styles() {
        assert_eq!(detect_eol(""), Eol::Lf);
        assert_eq!(detect_eol("no newline"), Eol::Lf);
        assert_eq!(detect_eol("a\nb"), Eol::Lf);
        assert_eq!(detect_eol("a\r\nb"), Eol::Crlf);
        assert_eq!(detect_eol("a\rb"), Eol::Cr);
        assert_eq!(detect_eol("中文\r\nx"), Eol::Crlf);
    }

    #[test]
    fn normalize_and_apply_are_inverse() {
        let raw = "a\r\nb\rc\nd";
        let normalized = normalize_eol(raw);
        assert_eq!(normalized, "a\nb\nc\nd");
        assert_eq!(apply_eol(&normalized, Eol::Crlf), "a\r\nb\r\nc\r\nd");
        assert_eq!(apply_eol(&normalized, Eol::Lf), normalized);
        assert_eq!(apply_eol(&normalized, Eol::Cr), "a\rb\rc\rd");
    }

    #[test]
    fn crlf_round_trip_preserves_bytes() {
        let dir = TempDir::new();
        let path = dir.path("crlf.txt");
        let written = write_text_file(
            &path,
            "alpha\nbeta\ngamma",
            Eol::Crlf,
            TextEncoding::Utf8,
            false,
        )
        .expect("写入");
        let on_disk = fs::read(&path).expect("读取");
        assert_eq!(on_disk, b"alpha\r\nbeta\r\ngamma");
        assert_eq!(written as usize, on_disk.len());

        let file = read_text_file(&path, u64::MAX).expect("读取");
        assert_eq!(file.text, "alpha\nbeta\ngamma");
        assert_eq!(file.eol, Eol::Crlf);
        assert_eq!(file.encoding, TextEncoding::Utf8);
        assert!(!file.bom);
    }

    #[test]
    fn utf16_le_with_bom_round_trip() {
        let dir = TempDir::new();
        let path = dir.path("utf16.txt");
        write_text_file(
            &path,
            "中文内容\nsecond",
            Eol::Crlf,
            TextEncoding::Utf16Le,
            true,
        )
        .expect("写入");

        let bytes = fs::read(&path).expect("读取");
        assert!(bytes.starts_with(BOM_UTF16LE), "应含 UTF-16LE BOM");

        let file = read_text_file(&path, u64::MAX).expect("读取");
        assert_eq!(file.encoding, TextEncoding::Utf16Le);
        assert!(file.bom);
        assert_eq!(file.eol, Eol::Crlf);
        assert_eq!(file.text, "中文内容\nsecond");
    }

    #[test]
    fn utf8_bom_is_preserved() {
        let dir = TempDir::new();
        let path = dir.path("bom.txt");
        write_text_file(&path, "x\ny", Eol::Lf, TextEncoding::Utf8, true).expect("写入");
        assert!(fs::read(&path).expect("读取").starts_with(BOM_UTF8));
        let file = read_text_file(&path, u64::MAX).expect("读取");
        assert!(file.bom);
        assert_eq!(file.text, "x\ny");
    }

    #[test]
    fn nul_byte_marks_binary() {
        let dir = TempDir::new();
        let path = dir.path("bin.dat");
        fs::write(&path, [0x68, 0x69, 0x00, 0x61]).expect("写入");
        let file = read_text_file(&path, u64::MAX).expect("读取");
        assert!(file.binary);
        assert_eq!(file.text, "");
    }

    #[test]
    fn too_large_is_rejected_before_reading() {
        let dir = TempDir::new();
        let path = dir.path("big.txt");
        fs::write(&path, "x".repeat(64)).expect("写入");
        let err = read_text_file(&path, 8).expect_err("应超限");
        assert!(matches!(err, TextError::TooLarge { size: 64, limit: 8 }));
    }

    #[test]
    fn parse_helpers_reject_unknown_values() {
        assert!(parse_eol("CRLF").is_ok());
        assert!(parse_eol("crlf").is_err());
        assert!(parse_encoding("UTF-16LE").is_ok());
        assert!(parse_encoding("GBK").is_err());
    }
}
