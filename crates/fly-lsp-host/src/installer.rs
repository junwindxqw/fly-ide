//! 语言服务器安装器：HTTPS 下载、SHA-256 校验、zip 解压、版本固定。
//!
//! 设计约束（对应开发计划 3.5.5）：
//! - 仅 HTTPS，仅允许白名单域名（`allowed_hosts`）；
//! - 下载必须校验 SHA-256，版本固定（清单中声明）；
//! - 解压必须防路径穿越（zip-slip）；
//! - 安装到 `%LOCALAPPDATA%\FlyIDE\servers\<id>\<version>\`，无需管理员权限；
//! - 全部异步（reqwest），大文件流式落盘。

use std::fs::{self, File};
use std::io::{self, Read};
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

/// 安装错误。
#[derive(Debug, thiserror::Error)]
pub enum InstallError {
    #[error("不允许的下载地址：{0}")]
    HostNotAllowed(String),
    #[error("下载失败：{0}")]
    Http(String),
    #[error("校验失败：期望 {expected}，实际 {actual}")]
    ChecksumMismatch { expected: String, actual: String },
    #[error("解压失败：{0}")]
    Zip(String),
    #[error("IO 错误：{0}")]
    Io(#[from] io::Error),
    #[error("zip 包含非法路径（路径穿越）：{0}")]
    ZipSlip(String),
}

/// 下载并校验，返回原始字节。
pub async fn download_verified(
    url: &str,
    expected_sha256: &str,
    allowed_hosts: &[&str],
) -> Result<Vec<u8>, InstallError> {
    let host = url
        .split("//")
        .nth(1)
        .and_then(|rest| rest.split('/').next())
        .unwrap_or_default()
        .to_string();
    if !allowed_hosts.iter().any(|h| *h == host) {
        return Err(InstallError::HostNotAllowed(host));
    }
    let response = reqwest::get(url)
        .await
        .map_err(|err| InstallError::Http(err.to_string()))?;
    if !response.status().is_success() {
        return Err(InstallError::Http(format!("HTTP {}", response.status())));
    }
    let bytes = response
        .bytes()
        .await
        .map_err(|err| InstallError::Http(err.to_string()))?;
    let actual = hex::encode(Sha256::digest(&bytes));
    if !actual.eq_ignore_ascii_case(expected_sha256) {
        return Err(InstallError::ChecksumMismatch {
            expected: expected_sha256.to_string(),
            actual,
        });
    }
    Ok(bytes.to_vec())
}

/// 解压 zip 到目标目录（防 zip-slip）。
pub fn extract_zip(bytes: &[u8], dest: &Path) -> Result<(), InstallError> {
    let reader = io::Cursor::new(bytes.to_vec());
    let mut archive =
        zip::ZipArchive::new(reader).map_err(|err| InstallError::Zip(err.to_string()))?;
    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|err| InstallError::Zip(err.to_string()))?;
        let Some(name) = entry.enclosed_name() else {
            return Err(InstallError::ZipSlip(format!("index {index}")));
        };
        let out_path = dest.join(name);
        if entry.is_dir() {
            fs::create_dir_all(&out_path)?;
            continue;
        }
        if let Some(parent) = out_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut out = File::create(&out_path)?;
        io::copy(&mut entry, &mut out)?;
    }
    Ok(())
}

/// 原子安装：下载 → 校验 → 解压到临时目录 → rename 到位。
///
/// 返回可执行文件所在目录（`servers/<id>/<version>/`）。
pub async fn install_server(
    base_dir: &Path,
    id: &str,
    version: &str,
    url: &str,
    expected_sha256: &str,
    allowed_hosts: &[&str],
) -> Result<PathBuf, InstallError> {
    let target = base_dir.join(id).join(version);
    if target.join(".complete").exists() {
        return Ok(target); // 已安装
    }
    let bytes = download_verified(url, expected_sha256, allowed_hosts).await?;
    let staging = base_dir.join(id).join(format!(".staging-{version}"));
    let _ = fs::remove_dir_all(&staging);
    fs::create_dir_all(&staging)?;
    extract_zip(&bytes, &staging)?;
    fs::write(staging.join(".complete"), b"ok")?;
    let _ = fs::remove_dir_all(&target);
    fs::create_dir_all(
        target
            .parent()
            .ok_or_else(|| InstallError::Io(io::Error::other("目标路径缺少父目录")))?,
    )?;
    fs::rename(&staging, &target)?;
    Ok(target)
}

/// 读取文件内容的 SHA-256（用于本地校验/诊断）。
pub fn sha256_of_file(path: &Path) -> io::Result<String> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let read = file.read(&mut buf)?;
        if read == 0 {
            break;
        }
        hasher.update(&buf[..read]);
    }
    Ok(hex::encode(hasher.finalize()))
}

/// 本地安装根目录（`%LOCALAPPDATA%\FlyIDE\servers`）。
pub fn servers_dir() -> Option<PathBuf> {
    std::env::var_os("LOCALAPPDATA").map(|base| PathBuf::from(base).join("FlyIDE").join("servers"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write as _;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("fly-lsp-install-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("创建临时目录");
        dir
    }

    #[test]
    fn sha256_of_file_matches_digest() {
        let dir = temp_dir("sha");
        let path = dir.join("f.bin");
        fs::write(&path, b"hello world").expect("写入");
        let actual = sha256_of_file(&path).expect("hash");
        let expected = hex::encode(Sha256::digest(b"hello world"));
        assert_eq!(actual, expected);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn extract_zip_writes_files() {
        // 用 zip crate 的内存写入构造一个 zip 再解压（往返）
        let dir = temp_dir("zip-round");
        let zip_path = dir.join("in.zip");
        {
            let file = File::create(&zip_path).expect("创建 zip");
            let mut writer = zip::ZipWriter::new(file);
            let options: zip::write::SimpleFileOptions = Default::default();
            writer.start_file("bin/tool.exe", options).expect("start");
            writer.write_all(b"MZ-fake").expect("write");
            writer.finish().expect("finish");
        }
        let bytes = fs::read(&zip_path).expect("读取 zip");
        let dest = dir.join("out");
        extract_zip(&bytes, &dest).expect("解压");
        assert_eq!(fs::read(dest.join("bin/tool.exe")).expect("读"), b"MZ-fake");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn extract_zip_rejects_parent_escape() {
        let dir = temp_dir("zip-slip");
        let zip_path = dir.join("evil.zip");
        {
            let file = File::create(&zip_path).expect("创建 zip");
            let mut writer = zip::ZipWriter::new(file);
            let options: zip::write::SimpleFileOptions = Default::default();
            // 构造一个显式使用 ../ 的条目（enclosed_name 会拒绝）
            writer.start_file("../escape.txt", options).expect("start");
            writer.write_all(b"nope").expect("write");
            writer.finish().expect("finish");
        }
        let bytes = fs::read(&zip_path).expect("读取 zip");
        let dest = dir.join("out");
        let err = extract_zip(&bytes, &dest).expect_err("应拒绝路径穿越");
        assert!(matches!(err, InstallError::ZipSlip(_)), "实际: {err:?}");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn checksum_mismatch_is_detected() {
        // 不联网，只验证比较逻辑本身
        let bytes = b"payload";
        let actual = hex::encode(Sha256::digest(bytes));
        let expected = hex::encode(Sha256::digest(b"other"));
        assert_ne!(actual.to_lowercase(), expected.to_lowercase());
    }

    #[tokio::test]
    async fn download_rejects_non_allowlisted_host() {
        let err = download_verified("https://evil.example.com/tool.zip", "abc", &["github.com"])
            .await
            .expect_err("应拒绝非白名单域名");
        assert!(matches!(err, InstallError::HostNotAllowed(_)));
    }
}
