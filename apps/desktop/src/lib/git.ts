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

export interface HunkInfo {
  index: number;
  oldStart: number;
  oldLines: number;
  newStart: number;
  newLines: number;
  section: string | null;
}

export interface FileDiff {
  path: string;
  staged: boolean;
  binary: boolean;
  oldText: string;
  newText: string;
  hunks: HunkInfo[];
}

export type HunkAction = "stage" | "unstage" | "discard";

export function loadGitStatus(root: string): Promise<GitStatus> {
  return invoke("git_status", { root });
}

export function loadGitNumstat(root: string, staged: boolean): Promise<NumStat[]> {
  return invoke("git_numstat", { root, staged });
}

export function loadGitVersion(): Promise<string> {
  return invoke("git_version");
}

/** 单文件 diff 数据：old/new 文本 + hunk 列表。 */
export function loadFileDiff(root: string, path: string, staged: boolean): Promise<FileDiff> {
  return invoke("git_diff_file", { root, path, staged });
}

/** 文件级暂存。 */
export function stageFiles(root: string, paths: string[]): Promise<void> {
  return invoke("git_stage", { root, paths });
}

/** 文件级取消暂存。 */
export function unstageFiles(root: string, paths: string[]): Promise<void> {
  return invoke("git_unstage", { root, paths });
}

/** 文件级丢弃（Rollback 到 HEAD；新文件删除）。 */
export function discardFiles(root: string, paths: string[]): Promise<void> {
  return invoke("git_discard", { root, paths });
}

/** hunk 级操作。 */
export function applyHunks(
  root: string,
  path: string,
  hunks: number[],
  action: HunkAction,
): Promise<void> {
  return invoke("git_apply_hunks", { root, path, hunks, action });
}

export interface CommitOptions {
  amend: boolean;
  signoff: boolean;
  noVerify: boolean;
}

/** 提交暂存区，返回命令输出（提交摘要 + hooks 输出）。 */
export function commitFiles(
  root: string,
  message: string,
  options: CommitOptions,
): Promise<string> {
  return invoke("git_commit", { root, message, options });
}

export interface CommitInfo {
  hash: string;
  short: string;
  author: string;
  date: string;
  subject: string;
  refs: string[];
}

export interface BlameLine {
  hash: string;
  author: string;
  time: number;
  summary: string;
  line: number;
}

/** 提交历史。 */
export function loadGitLog(root: string, limit = 50): Promise<CommitInfo[]> {
  return invoke("git_log", { root, limit });
}

/** 单文件行内 blame。 */
export function loadGitBlame(root: string, path: string): Promise<BlameLine[]> {
  return invoke("git_blame", { root, path });
}

/** 提交的变更摘要。 */
export function showCommit(root: string, hash: string): Promise<string> {
  return invoke("git_show_commit", { root, hash });
}
