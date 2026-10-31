import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";

/** 文件系统 API，与 src-tauri/src/fs_commands.rs 的 DTO 对应（camelCase）。 */

export interface TextFile {
  /** 已规范为 \n 行尾的文本；二进制文件为空 */
  text: string;
  encoding: string;
  bom: boolean;
  eol: string;
  size: number;
  tier: "full" | "reduced" | "readOnlyPlain" | "tooLarge";
  binary: boolean;
}

export interface DirEntry {
  name: string;
  isDir: boolean;
  ignored: boolean;
}

export function fsReadText(path: string): Promise<TextFile> {
  return invoke("fs_read_text", { path });
}

export function fsWriteText(
  path: string,
  text: string,
  eol: string,
  encoding: string,
  bom: boolean,
): Promise<number> {
  return invoke("fs_write_text", { path, text, eol, encoding, bom });
}

export function fsListDir(path: string): Promise<DirEntry[]> {
  return invoke("fs_list_dir", { path });
}

export async function pickFolder(): Promise<string | null> {
  const selection = await open({ directory: true, multiple: false, title: "打开文件夹" });
  return typeof selection === "string" ? selection : null;
}
