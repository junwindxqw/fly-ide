//! 语言包清单：语言 → 语法包 / LSP 服务器 / 安装来源。
//!
//! 内置清单（`language-packs.json`）声明每种语言的：
//! - 语法高亮用的 CodeMirror 语言包（前端懒加载）；
//! - LSP 服务器命令、参数、初始化选项；
//! - 安装来源（prebuilt 下载 URL + SHA-256）或系统 PATH 检测；
//! - 服务器所需的外部运行时（Node / Go 工具链等）提示。
//!
//! 清单可在后续版本支持用户覆盖（`.flyide/language-packs.json`）。

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// 一种语言的服务器定义。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LanguagePack {
    pub id: String,
    pub name: String,
    /// 该语言的文件扩展名（不含点）。
    pub extensions: Vec<String>,
    /// 前端语法包标识（对应 src/lib/editor/syntax.ts 的 loaders 键）。
    pub grammar: String,
    /// LSP 服务器配置。
    pub server: ServerSpec,
}

/// 服务器启动规格。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerSpec {
    /// 可执行文件名（用于 PATH 检测）。
    pub command: String,
    /// 启动参数。
    #[serde(default)]
    pub args: Vec<String>,
    /// 安装来源：None 表示仅检测 PATH。
    #[serde(default)]
    pub install: Option<InstallSpec>,
    /// 启动所需的外部运行时提示（展示给用户）。
    #[serde(default)]
    pub runtime_hint: Option<String>,
}

/// 预编译包安装来源。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallSpec {
    /// 下载 URL（必须 HTTPS 且域名在白名单内）。
    pub url: String,
    /// SHA-256（小写十六进制）。
    pub sha256: String,
    /// 解压后可执行文件在包内的相对路径。
    pub executable: String,
    /// 版本号（用于安装目录隔离）。
    pub version: String,
}

/// 内置语言包清单（当前版本：服务器仅检测 PATH，安装来源随各语言版本
/// 稳定性逐步补充——URL + 校验和需要与上游发布节奏同步维护）。
pub fn builtin_packs() -> Vec<LanguagePack> {
    vec![
        LanguagePack {
            id: "go".into(),
            name: "Go".into(),
            extensions: vec!["go".into()],
            grammar: "go".into(),
            server: ServerSpec {
                command: "gopls".into(),
                args: vec!["serve".into()],
                install: None,
                runtime_hint: Some("需要 Go 工具链（gopls 为 Go 编写）".into()),
            },
        },
        LanguagePack {
            id: "rust".into(),
            name: "Rust".into(),
            extensions: vec!["rs".into()],
            grammar: "rust".into(),
            server: ServerSpec {
                command: "rust-analyzer".into(),
                args: vec![],
                install: None,
                runtime_hint: Some("需要 rustup 工具链".into()),
            },
        },
        LanguagePack {
            id: "python".into(),
            name: "Python".into(),
            extensions: vec!["py".into()],
            grammar: "python".into(),
            server: ServerSpec {
                command: "basedpyright-langserver".into(),
                args: vec!["--stdio".into()],
                install: None,
                runtime_hint: Some("需要 Node.js 运行时（npm 包 basedpyright）".into()),
            },
        },
        LanguagePack {
            id: "php".into(),
            name: "PHP".into(),
            extensions: vec!["php".into()],
            grammar: "php".into(),
            server: ServerSpec {
                command: "phpactor".into(),
                args: vec!["language-server".into()],
                install: None,
                runtime_hint: Some("需要 PHP 8.1+ 运行时".into()),
            },
        },
        LanguagePack {
            id: "typescript".into(),
            name: "TypeScript / JavaScript".into(),
            extensions: vec!["ts".into(), "tsx".into(), "js".into(), "mjs".into()],
            grammar: "javascript".into(),
            server: ServerSpec {
                command: "typescript-language-server".into(),
                args: vec!["--stdio".into()],
                install: None,
                runtime_hint: Some("需要 Node.js 运行时".into()),
            },
        },
    ]
}

/// PATH 中检测可执行文件（Windows 含 PATHEXT 解析）。
///
/// 返回完整路径；未找到返回 None。
pub fn which(command: &str) -> Option<PathBuf> {
    // 含路径分隔符时按直接路径判断
    let direct = Path::new(command);
    if direct.components().count() > 1 && direct.is_file() {
        return Some(direct.to_path_buf());
    }
    let path_var = std::env::var_os("PATH")?;
    let exts: Vec<String> = std::env::var("PATHEXT")
        .unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".to_string())
        .split(';')
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect();
    for dir in std::env::split_paths(&path_var) {
        let candidate = dir.join(command);
        if candidate.is_file() {
            return Some(candidate);
        }
        for ext in &exts {
            let with_ext = dir.join(format!("{command}{ext}"));
            if with_ext.is_file() {
                return Some(with_ext);
            }
        }
    }
    None
}

/// 服务器可用性状态。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ServerAvailability {
    /// PATH 中可用（含解析出的路径）。
    Available(String),
    /// 需要下载安装（清单有安装来源）。
    Installable,
    /// 不可用（无 PATH、无安装来源）。
    Missing,
}

/// 检测某语言包的服务器可用性。
pub fn detect_server(pack: &LanguagePack) -> ServerAvailability {
    if let Some(path) = which(&pack.server.command) {
        return ServerAvailability::Available(path.to_string_lossy().into_owned());
    }
    if pack.server.install.is_some() {
        ServerAvailability::Installable
    } else {
        ServerAvailability::Missing
    }
}

/// 按语言 ID 查找语言包。
pub fn find_pack<'a>(packs: &'a [LanguagePack], id: &str) -> Option<&'a LanguagePack> {
    packs.iter().find(|pack| pack.id == id)
}

/// 按文件扩展名查找语言包（不含点）。
pub fn pack_for_extension<'a>(packs: &'a [LanguagePack], ext: &str) -> Option<&'a LanguagePack> {
    packs
        .iter()
        .find(|pack| pack.extensions.iter().any(|e| e == ext))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_packs_cover_target_languages() {
        let packs = builtin_packs();
        for id in ["go", "rust", "python", "php", "typescript"] {
            assert!(find_pack(&packs, id).is_some(), "缺少语言包 {id}");
        }
    }

    #[test]
    fn pack_for_extension_maps_files() {
        let packs = builtin_packs();
        assert_eq!(
            pack_for_extension(&packs, "go").map(|p| p.id.as_str()),
            Some("go")
        );
        assert_eq!(
            pack_for_extension(&packs, "rs").map(|p| p.id.as_str()),
            Some("rust")
        );
        assert_eq!(
            pack_for_extension(&packs, "ts").map(|p| p.id.as_str()),
            Some("typescript")
        );
        assert!(pack_for_extension(&packs, "unknown").is_none());
    }

    #[test]
    fn deserialize_custom_pack() {
        let json = r#"{
            "id": "custom",
            "name": "Custom",
            "extensions": ["cst"],
            "grammar": "go",
            "server": {
                "command": "custom-ls",
                "args": ["--stdio"],
                "runtimeHint": "需要 X"
            }
        }"#;
        let pack: LanguagePack = serde_json::from_str(json).expect("解析");
        assert_eq!(pack.id, "custom");
        assert_eq!(pack.server.command, "custom-ls");
        assert!(pack.server.install.is_none());
    }

    #[test]
    fn which_finds_common_windows_executable() {
        // cmd.exe 在 PATH 中必然存在
        assert!(which("cmd").is_some(), "PATH 中应能找到 cmd");
    }

    #[test]
    fn which_rejects_missing() {
        assert!(which("definitely-not-a-real-binary-xyz").is_none());
    }

    #[test]
    fn detect_server_marks_missing_for_absent_binary() {
        let mut packs = builtin_packs();
        let pack = packs.iter_mut().find(|p| p.id == "go").expect("go pack");
        // 用不存在的命令模拟缺失
        pack.server.command = "definitely-not-a-real-binary-xyz".into();
        assert_eq!(detect_server(pack), ServerAvailability::Missing);
    }

    #[test]
    fn detect_server_reports_available_for_cmd() {
        let mut packs = builtin_packs();
        let pack = packs.iter_mut().find(|p| p.id == "go").expect("go pack");
        pack.server.command = "cmd".into();
        match detect_server(pack) {
            ServerAvailability::Available(path) => {
                assert!(path.to_lowercase().contains("cmd"));
            }
            other => panic!("应可用: {other:?}"),
        }
    }
}
