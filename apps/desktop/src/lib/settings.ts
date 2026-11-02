import { invoke } from "@tauri-apps/api/core";

/** 应用设置 API（settings_commands.rs，camelCase DTO）。 */

export interface Settings {
  theme: "dark" | "light" | "system";
  language: string;
  editorFont: string;
  editorFontSize: number;
  tabSize: number;
  formatOnSave: boolean;
  serverPaths: Record<string, string>;
}

export function getSettings(): Promise<Settings> {
  return invoke("settings_get");
}

export function updateSettings(settings: Settings): Promise<Settings> {
  return invoke("settings_update", { dto: settings });
}
