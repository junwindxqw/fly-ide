//! 应用设置：JSONC 存储于 %APPDATA%\FlyIDE\settings.json，热重载。

use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

/// 应用设置。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    /// 主题：dark / light / system。
    pub theme: String,
    /// 界面语言：zh-CN / en-US。
    pub language: String,
    /// 编辑器字体。
    pub editor_font: String,
    /// 编辑器字号。
    pub editor_font_size: u32,
    /// Tab 宽度。
    pub tab_size: u32,
    /// 保存时格式化。
    pub format_on_save: bool,
    /// 语言服务器路径覆盖（language → 可执行文件路径）。
    pub server_paths: std::collections::HashMap<String, String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: "dark".into(),
            language: "zh-CN".into(),
            editor_font: "Cascadia Code, Consolas, monospace".into(),
            editor_font_size: 12,
            tab_size: 2,
            format_on_save: false,
            server_paths: std::collections::HashMap::new(),
        }
    }
}

impl Settings {
    /// 从 JSON 字符串解析（容错：非法字段回退默认值）。
    pub fn from_json_str(text: &str) -> Self {
        serde_json::from_str(text).unwrap_or_default()
    }

    /// 序列化为 JSON 字符串（带缩进）。
    pub fn to_json_pretty(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }
}

/// 设置存储：文件持久化 + 内存缓存。
/// Clone 共享同一内部状态（Path + Mutex 在 Arc 中）。
#[derive(Clone)]
pub struct SettingsStore {
    inner: std::sync::Arc<SettingsInner>,
}

struct SettingsInner {
    path: PathBuf,
    current: Mutex<Settings>,
}

impl SettingsStore {
    /// 从应用数据目录加载（文件不存在则用默认值）。
    pub fn load() -> std::io::Result<Self> {
        let path = Self::settings_path()?;
        let settings = match fs::read_to_string(&path) {
            Ok(text) => Settings::from_json_str(&text),
            Err(_) => Settings::default(),
        };
        Ok(Self {
            inner: std::sync::Arc::new(SettingsInner {
                path,
                current: Mutex::new(settings),
            }),
        })
    }

    /// 仅内存的存储（持久化失败时的降级路径；路径指向临时目录）。
    pub fn new_in_memory() -> Self {
        let path = std::env::temp_dir().join("fly-ide-settings-memory.json");
        Self {
            inner: std::sync::Arc::new(SettingsInner {
                path,
                current: Mutex::new(Settings::default()),
            }),
        }
    }

    pub fn get(&self) -> Settings {
        self.inner.current.lock().expect("settings 锁").clone()
    }

    /// 更新并持久化（写临时文件 + rename 原子替换）。
    pub fn update(&self, settings: Settings) -> std::io::Result<()> {
        if let Some(parent) = self.inner.path.parent() {
            fs::create_dir_all(parent)?;
        }
        let text = settings.to_json_pretty();
        let mut temp = self.inner.path.clone();
        temp.set_extension("json.tmp");
        fs::write(&temp, &text)?;
        fs::rename(&temp, &self.inner.path)?;
        *self.inner.current.lock().expect("settings 锁") = settings;
        Ok(())
    }

    fn settings_path() -> std::io::Result<PathBuf> {
        let base = std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .ok_or_else(|| std::io::Error::other("APPDATA 环境变量缺失"))?;
        Ok(base.join("FlyIDE").join("settings.json"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_settings_are_sane() {
        let settings = Settings::default();
        assert_eq!(settings.theme, "dark");
        assert_eq!(settings.language, "zh-CN");
        assert!(settings.tab_size > 0);
    }

    #[test]
    fn parse_ignores_unknown_fields() {
        let parsed = Settings::from_json_str(r#"{"theme":"light","unknownField":42}"#);
        assert_eq!(parsed.theme, "light");
        // 未提供的字段回退默认
        assert_eq!(parsed.language, "zh-CN");
    }

    #[test]
    fn parse_invalid_json_falls_back_to_default() {
        let parsed = Settings::from_json_str("not json at all");
        assert_eq!(parsed, Settings::default());
    }

    #[test]
    fn round_trip_preserves_fields() {
        let settings = Settings {
            theme: "light".into(),
            tab_size: 4,
            format_on_save: true,
            ..Default::default()
        };
        let text = settings.to_json_pretty();
        let parsed = Settings::from_json_str(&text);
        assert_eq!(parsed, settings);
    }
}
