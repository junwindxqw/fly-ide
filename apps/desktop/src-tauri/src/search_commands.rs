//! 搜索命令：快速打开（文件索引缓存）+ 内容搜索（取消注册表）。
//!
//! 索引在每次 search_files 时按 root 是否变化决定重建；内容搜索一次性
//! 返回（上限内），取消通过 search_id 注册表传递 AtomicBool。
//!
//! `tauri::State<'_>` 的借用不能进入 `'static` 的阻塞闭包，
//! 因此统一通过 `AppHandle` 在闭包内取 state。

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use tauri::Manager;

use fly_search::{ContentMatch, FileMatch, SearchOptions, SearchSummary};

use crate::commands::run_blocking;

pub struct SearchState(Mutex<SearchInner>);

struct SearchInner {
    index_root: Option<PathBuf>,
    files: Vec<String>,
    cancels: HashMap<u32, Arc<AtomicBool>>,
}

impl SearchState {
    pub fn new() -> Self {
        Self(Mutex::new(SearchInner {
            index_root: None,
            files: Vec::new(),
            cancels: HashMap::new(),
        }))
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileMatchDto {
    pub path: String,
    pub score: u32,
}

#[tauri::command]
pub async fn search_files(
    app: tauri::AppHandle,
    root: String,
    query: String,
    limit: usize,
) -> Result<Vec<FileMatchDto>, String> {
    run_blocking(move || {
        let state = app.state::<SearchState>();
        let root_path = PathBuf::from(&root);
        let files = {
            let mut inner = state.0.lock().expect("search state 锁");
            let stale = inner
                .index_root
                .as_ref()
                .is_none_or(|cached| *cached != root_path);
            if stale {
                let index = fly_search::build_file_index(&root_path)
                    .map_err(|err| format!("构建文件索引失败：{err}"))?;
                inner.files = index;
                inner.index_root = Some(root_path.clone());
            }
            inner.files.clone()
        };
        Ok(fly_search::fuzzy_search(&files, &query, limit.max(1))
            .into_iter()
            .map(|FileMatch { path, score }| FileMatchDto { path, score })
            .collect())
    })
    .await
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchOptionsDto {
    pub case_sensitive: bool,
    pub whole_word: bool,
    pub regex: bool,
    pub max_results: Option<usize>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContentMatchDto {
    pub path: String,
    pub line: u64,
    pub text: String,
    pub start: usize,
    pub end: usize,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContentSearchResultDto {
    pub matches: Vec<ContentMatchDto>,
    pub truncated: bool,
    pub canceled: bool,
    pub files_searched: usize,
}

/// 注册取消令牌；同 id 重复搜索先清理旧令牌。
fn register_cancel(state: &SearchState, search_id: u32) -> Arc<AtomicBool> {
    let mut inner = state.0.lock().expect("search state 锁");
    let cancel = Arc::new(AtomicBool::new(false));
    inner.cancels.insert(search_id, cancel.clone());
    cancel
}

#[tauri::command]
pub async fn search_content(
    app: tauri::AppHandle,
    root: String,
    query: String,
    options: SearchOptionsDto,
    search_id: u32,
) -> Result<ContentSearchResultDto, String> {
    // 注册必须在阻塞任务外完成（借用不可进入 'static 闭包）
    let cancel = {
        let state = app.state::<SearchState>();
        register_cancel(&state, search_id)
    };
    run_blocking(move || {
        let opts = SearchOptions {
            case_sensitive: options.case_sensitive,
            whole_word: options.whole_word,
            regex: options.regex,
            max_results: options
                .max_results
                .unwrap_or(fly_search::DEFAULT_RESULT_LIMIT),
        };
        let (matches, summary): (Vec<ContentMatch>, SearchSummary) =
            fly_search::search_content(std::path::Path::new(&root), &query, &opts, &cancel)
                .map_err(|err| err.to_string())?;
        let state = app.state::<SearchState>();
        state
            .0
            .lock()
            .expect("search state 锁")
            .cancels
            .remove(&search_id);
        Ok(ContentSearchResultDto {
            matches: matches
                .into_iter()
                .map(|m| ContentMatchDto {
                    path: m.path,
                    line: m.line,
                    text: m.text,
                    start: m.start,
                    end: m.end,
                })
                .collect(),
            truncated: summary.truncated,
            canceled: summary.canceled,
            files_searched: summary.files_searched,
        })
    })
    .await
}

#[tauri::command]
pub async fn search_cancel(app: tauri::AppHandle, search_id: u32) -> Result<(), String> {
    let state = app.state::<SearchState>();
    if let Some(cancel) = state
        .0
        .lock()
        .expect("search state 锁")
        .cancels
        .get(&search_id)
    {
        cancel.store(true, Ordering::Relaxed);
    }
    Ok(())
}
