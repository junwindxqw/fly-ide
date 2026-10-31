import { invoke } from "@tauri-apps/api/core";

/** 与 src-tauri/src/commands.rs 的 DTO 对应（camelCase）。
 *  后续接入 ts-rs/tauri-specta 后由 Rust 侧生成，先手写保持同步。 */

export interface StatusEntry {
  /** 暂存区状态码（porcelain v2 X） */
  x: string;
  /** 工作区状态码（porcelain v2 Y） */
  y: string;
  path: string;
  origPath: string | null;
  untracked: boolean;
  unmerged: boolean;
}

export interface GitStatus {
  branch: string | null;
  oid: string | null;
  upstream: string | null;
  ahead: number | null;
  behind: number | null;
  entries: StatusEntry[];
}

export interface NumStat {
  added: number | null;
  deleted: number | null;
  path: string;
  origPath: string | null;
}

export function loadGitStatus(root: string): Promise<GitStatus> {
  return invoke("git_status", { root });
}

export function loadGitNumstat(root: string, staged: boolean): Promise<NumStat[]> {
  return invoke("git_numstat", { root, staged });
}

export function loadGitVersion(): Promise<string> {
  return invoke("git_version");
}
