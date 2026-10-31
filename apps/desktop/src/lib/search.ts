import { invoke } from "@tauri-apps/api/core";

/** 搜索与文件监听 API，对应 search_commands.rs / watch_commands.rs（camelCase）。 */

export interface FileMatch {
  path: string;
  score: number;
}

export function searchFiles(root: string, query: string, limit = 50): Promise<FileMatch[]> {
  return invoke("search_files", { root, query, limit });
}

export interface ContentMatch {
  /** 相对路径（/ 分隔） */
  path: string;
  line: number;
  text: string;
  start: number;
  end: number;
}

export interface ContentSearchResult {
  matches: ContentMatch[];
  truncated: boolean;
  canceled: boolean;
  filesSearched: number;
}

export interface SearchOptionsDto {
  caseSensitive: boolean;
  wholeWord: boolean;
  regex: boolean;
  maxResults?: number;
}

let searchIdCounter = 1;

export function nextSearchId(): number {
  return searchIdCounter++;
}

/** 搜索内容；返回 searchId 供取消。 */
export function searchContent(
  root: string,
  query: string,
  options: SearchOptionsDto,
  searchId: number,
): Promise<ContentSearchResult> {
  return invoke("search_content", { root, query, options, searchId });
}

export function searchCancel(searchId: number): Promise<void> {
  return invoke("search_cancel", { searchId });
}

export function watchStart(root: string): Promise<void> {
  return invoke("watch_start", { root });
}

export function watchStop(): Promise<void> {
  return invoke("watch_stop");
}

/** fs-changed 事件负载（Rust watch_commands::FsChangedPayload）。 */
export interface FsChangedPayload {
  paths: string[];
}
