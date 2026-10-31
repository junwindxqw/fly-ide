//! 文件系统命令：文本读写、目录列表。
//!
//! 领域逻辑在 `fly-fs` / `fly-workspace`，本模块只做 DTO 映射与
//! spawn_blocking。文件夹选择由前端直接调用 `tauri-plugin-dialog`。

use serde::Serialize;

use crate::commands::run_blocking;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TextFileDto {
    pub text: String,
    pub encoding: String,
    pub bom: bool,
    pub eol: String,
    pub size: u64,
    pub tier: String,
    pub binary: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DirEntryDto {
    pub name: String,
    pub is_dir: bool,
    pub ignored: bool,
}

/// 读取文本文件。超大文件（>50MB）返回错误由前端提示。
#[tauri::command]
pub async fn fs_read_text(path: String) -> Result<TextFileDto, String> {
    run_blocking(move || {
        let file = fly_fs::read_text_file(std::path::Path::new(&path), fly_fs::READ_ONLY_LIMIT)
            .map_err(|err| err.to_string())?;
        Ok(TextFileDto {
            text: file.text,
            encoding: file.encoding.as_str().to_string(),
            bom: file.bom,
            eol: file.eol.as_str().to_string(),
            size: file.size,
            tier: file.tier.as_str().to_string(),
            binary: file.binary,
        })
    })
    .await
}

/// 原子保存文本文件。`text` 为规范 `\n` 行尾的编辑器文本，
/// 本函数按 `eol` 还原、按 `encoding`/`bom` 编码。返回写入字节数。
#[tauri::command]
pub async fn fs_write_text(
    path: String,
    text: String,
    eol: String,
    encoding: String,
    bom: bool,
) -> Result<u64, String> {
    run_blocking(move || {
        let eol = fly_fs::text::parse_eol(&eol).map_err(|err| err.to_string())?;
        let encoding = fly_fs::text::parse_encoding(&encoding).map_err(|err| err.to_string())?;
        fly_fs::write_text_file(std::path::Path::new(&path), &text, eol, encoding, bom)
            .map_err(|err| err.to_string())
    })
    .await
}

/// 列出目录一层内容（目录在前、名称不区分大小写排序、ignored 标记）。
#[tauri::command]
pub async fn fs_list_dir(path: String) -> Result<Vec<DirEntryDto>, String> {
    run_blocking(move || {
        let entries =
            fly_workspace::list_dir(std::path::Path::new(&path)).map_err(|err| err.to_string())?;
        Ok(entries
            .into_iter()
            .map(|entry| DirEntryDto {
                name: entry.name,
                is_dir: entry.is_dir,
                ignored: entry.ignored,
            })
            .collect())
    })
    .await
}

/// 判断路径是否为目录（文件树懒加载展开前校验）。
#[tauri::command]
pub async fn fs_is_dir(path: String) -> Result<bool, String> {
    run_blocking(move || Ok(std::path::Path::new(&path).is_dir())).await
}
