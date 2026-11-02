//! 设置命令：读取/更新应用设置（Rust 侧持久化）。

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::commands::run_blocking;
use fly_workspace::{Settings, SettingsStore};

pub struct SettingsState(pub SettingsStore);

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SettingsDto {
    pub theme: String,
    pub language: String,
    pub editor_font: String,
    pub editor_font_size: u32,
    pub tab_size: u32,
    pub format_on_save: bool,
    pub server_paths: HashMap<String, String>,
}

impl From<&Settings> for SettingsDto {
    fn from(s: &Settings) -> Self {
        Self {
            theme: s.theme.clone(),
            language: s.language.clone(),
            editor_font: s.editor_font.clone(),
            editor_font_size: s.editor_font_size,
            tab_size: s.tab_size,
            format_on_save: s.format_on_save,
            server_paths: s.server_paths.clone(),
        }
    }
}

impl From<SettingsDto> for Settings {
    fn from(d: SettingsDto) -> Self {
        Self {
            theme: d.theme,
            language: d.language,
            editor_font: d.editor_font,
            editor_font_size: d.editor_font_size,
            tab_size: d.tab_size,
            format_on_save: d.format_on_save,
            server_paths: d.server_paths,
        }
    }
}

#[tauri::command]
pub async fn settings_get(state: tauri::State<'_, SettingsState>) -> Result<SettingsDto, String> {
    Ok(SettingsDto::from(&state.0.get()))
}

#[tauri::command]
pub async fn settings_update(
    state: tauri::State<'_, SettingsState>,
    dto: SettingsDto,
) -> Result<SettingsDto, String> {
    let store = state.0.clone();
    run_blocking(move || {
        store
            .update(Settings::from(dto))
            .map_err(|err| format!("保存设置失败：{err}"))?;
        Ok(())
    })
    .await?;
    Ok(SettingsDto::from(&state.0.get()))
}
