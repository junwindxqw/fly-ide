//! 真实 git 仓库上的集成测试：fixture 仓库在临时目录中现场创建。
//!
//! 与 `src/status.rs`、`src/numstat.rs` 的纯解析单测互补，
//! 这里验证「命令参数 → 真实 git 输出 → 解析」整条链路的格式假设。

use std::path::Path;
use std::process::Command;

use fly_git::{FileStatus, Repo};

fn git(dir: &Path, args: &[&str]) {
    let output = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .expect("git 可执行文件应存在");
    assert!(
        output.status.success(),
        "git {:?} 失败：{}",
        args,
        String::from_utf8_lossy(&output.stderr)
    );
}

fn write(dir: &Path, rel: &str, content: &str) {
    let path = dir.join(rel);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("创建目录");
    }
    std::fs::write(path, content).expect("写入文件");
}

fn temp_repo() -> (tempfile::TempDir, Repo) {
    let dir = tempfile::tempdir().expect("创建临时目录");
    git(dir.path(), &["init", "-b", "main"]);
    git(dir.path(), &["config", "user.name", "fly"]);
    git(dir.path(), &["config", "user.email", "fly@example.com"]);
    // CRLF 行为确定性：不自动转换，避免 CI 与本机 autocrlf 配置差异
    git(dir.path(), &["config", "core.autocrlf", "false"]);
    let repo = Repo::discover(dir.path()).expect("应发现仓库根");
    (dir, repo)
}

#[test]
fn git_version_meets_minimum() {
    let version = fly_git::detect_version().expect("测试机应安装 git");
    assert!(fly_git::is_version_supported(version));
    assert!(fly_git::ensure_min_version().is_ok());
}

#[test]
fn discover_finds_root_and_ignores_plain_dirs() {
    let (dir, repo) = temp_repo();
    assert_eq!(repo.root(), dir.path());

    let plain = tempfile::tempdir().expect("临时目录");
    let nested = plain.path().join("a").join("b");
    std::fs::create_dir_all(&nested).expect("创建目录");
    assert!(Repo::discover(&nested).is_none());
}

#[test]
fn status_on_fresh_repo_has_no_entries() {
    let (_dir, repo) = temp_repo();
    let status = repo.status().expect("status 成功");
    assert_eq!(status.branch.as_deref(), Some("main"));
    assert!(status.oid.is_none(), "空仓库 oid 应为 (initial) → None");
    assert!(status.entries.is_empty());
}

#[test]
fn status_reports_staged_unstaged_untracked_and_deleted() {
    let (dir, repo) = temp_repo();
    write(dir.path(), "a.txt", "hello\n");
    write(dir.path(), "b.txt", "world\n");
    git(dir.path(), &["add", "-A"]);
    git(dir.path(), &["commit", "-m", "init"]);

    write(dir.path(), "a.txt", "hello v2\n"); // 工作区修改
    write(dir.path(), "c.txt", "new\n"); // 暂存的新文件
    git(dir.path(), &["add", "c.txt"]);
    write(dir.path(), "d.txt", "untracked\n"); // 未跟踪
    std::fs::remove_file(dir.path().join("b.txt")).expect("删除文件"); // 未暂存的删除

    let status = repo.status().expect("status 成功");
    assert_eq!(status.branch.as_deref(), Some("main"));
    assert!(status.oid.is_some());

    let find = |path: &str| {
        status
            .entries
            .iter()
            .find(|entry| entry.path == path)
            .unwrap_or_else(|| panic!("缺少 {path} 条目"))
    };

    let a = find("a.txt");
    assert_eq!(a.index, FileStatus::Unmodified);
    assert_eq!(a.worktree, FileStatus::Modified);

    let c = find("c.txt");
    assert_eq!(c.index, FileStatus::Added);
    assert_eq!(c.worktree, FileStatus::Unmodified);

    let d = find("d.txt");
    assert!(d.untracked);

    let b = find("b.txt");
    assert_eq!(b.worktree, FileStatus::Deleted);
    assert!(!b.untracked);
}

#[test]
fn status_and_numstat_handle_rename() {
    let (dir, repo) = temp_repo();
    write(dir.path(), "old.txt", "content\n");
    git(dir.path(), &["add", "-A"]);
    git(dir.path(), &["commit", "-m", "init"]);
    git(dir.path(), &["mv", "old.txt", "new.txt"]);

    let status = repo.status().expect("status 成功");
    let renamed = status
        .entries
        .iter()
        .find(|entry| entry.index == FileStatus::Renamed)
        .expect("应出现重命名条目");
    assert_eq!(renamed.path, "new.txt");
    assert_eq!(renamed.orig_path.as_deref(), Some("old.txt"));

    // 验证 numstat -z 的重命名格式假设：path 之后跟随 origPath
    let staged = repo.diff_numstat(true).expect("numstat 成功");
    let stat = staged
        .iter()
        .find(|stat| stat.path == "new.txt")
        .expect("numstat 应包含重命名");
    assert_eq!(stat.orig_path.as_deref(), Some("old.txt"));
    assert_eq!(stat.added, Some(0));
    assert_eq!(stat.deleted, Some(0));
}

#[test]
fn numstat_counts_worktree_changes() {
    let (dir, repo) = temp_repo();
    write(dir.path(), "a.txt", "one\n");
    git(dir.path(), &["add", "-A"]);
    git(dir.path(), &["commit", "-m", "init"]);

    write(dir.path(), "a.txt", "one\ntwo\nthree\n");
    let stats = repo.diff_numstat(false).expect("numstat 成功");
    let stat = stats
        .iter()
        .find(|stat| stat.path == "a.txt")
        .expect("numstat 应包含 a.txt");
    assert_eq!(stat.added, Some(2));
    assert_eq!(stat.deleted, Some(0));
    assert_eq!(stat.orig_path, None);
}

#[test]
fn stage_single_hunk_round_trip() {
    let (dir, repo) = temp_repo();
    let body: String = (1..=12).map(|i| format!("line{i}\n")).collect();
    write(dir.path(), "f.txt", &body);
    git(dir.path(), &["add", "-A"]);
    git(dir.path(), &["commit", "-m", "init"]);

    // 两处相距足够远的修改 → 恰好 2 个 hunk
    let modified = body
        .replacen("line2\n", "LINE2\n", 1)
        .replacen("line11\n", "LINE11\n", 1);
    assert_ne!(modified, body);
    write(dir.path(), "f.txt", &modified);

    let file = repo
        .file_patch("f.txt", false)
        .expect("patch 生成失败")
        .expect("应有 patch");
    assert_eq!(file.hunks.len(), 2, "两处远距离修改应产生两个 hunk");

    // 只暂存第一个 hunk → index 含 LINE2、不含 LINE11，工作区两者都有
    repo.stage_hunks("f.txt", &[0]).expect("暂存 hunk");
    let status = repo.status().expect("status");
    let entry = status
        .entries
        .iter()
        .find(|entry| entry.path == "f.txt")
        .expect("f.txt 应在状态中");
    assert_eq!(entry.index, FileStatus::Modified);
    assert_eq!(entry.worktree, FileStatus::Modified, "应为 MM（部分暂存）");

    let index_text = repo
        .index_text("f.txt")
        .expect("读取 index 失败")
        .expect("index 有文件");
    assert!(index_text.contains("LINE2\n"));
    assert!(!index_text.contains("LINE11\n"));
    assert!(index_text.contains("line11\n"));

    // 取消暂存同一 hunk → index 回到 HEAD
    repo.unstage_hunks("f.txt", &[0]).expect("取消暂存 hunk");
    let index_text = repo
        .index_text("f.txt")
        .expect("读取 index 失败")
        .expect("index 有文件");
    assert!(!index_text.contains("LINE2\n"));
    assert!(!index_text.contains("LINE11\n"));
}

#[test]
fn crlf_survives_hunk_staging() {
    let (dir, repo) = temp_repo();
    write(dir.path(), "crlf.txt", "alpha\r\nbeta\r\ngamma\r\n");
    git(dir.path(), &["add", "-A"]);
    git(dir.path(), &["commit", "-m", "init"]);

    write(dir.path(), "crlf.txt", "alpha\r\nBETA\r\ngamma\r\n");
    repo.stage_hunks("crlf.txt", &[0]).expect("暂存 hunk");

    let index_text = repo
        .index_text("crlf.txt")
        .expect("读取 index 失败")
        .expect("index 有文件");
    assert!(index_text.contains("BETA"), "改动应已入 index");
    assert!(
        index_text.contains("\r\n"),
        "index 内容必须保留 CRLF，实际：{index_text:?}"
    );
}

#[test]
fn file_level_stage_unstage_discard() {
    let (dir, repo) = temp_repo();
    write(dir.path(), "m.txt", "hello\n");
    write(dir.path(), "k.txt", "keep\n");
    git(dir.path(), &["add", "-A"]);
    git(dir.path(), &["commit", "-m", "init"]);

    write(dir.path(), "m.txt", "changed\n");
    write(dir.path(), "new.txt", "brand new\n");

    repo.stage_paths(&["m.txt", "new.txt"]).expect("批量暂存");
    let status = repo.status().expect("status");
    let m = status.entries.iter().find(|e| e.path == "m.txt").unwrap();
    assert_eq!(m.index, FileStatus::Modified);
    assert_eq!(m.worktree, FileStatus::Unmodified);
    let new = status.entries.iter().find(|e| e.path == "new.txt").unwrap();
    assert_eq!(new.index, FileStatus::Added);

    repo.unstage_paths(&["new.txt"]).expect("取消暂存");
    let status = repo.status().expect("status");
    let new = status.entries.iter().find(|e| e.path == "new.txt").unwrap();
    assert!(new.untracked, "取消暂存后应回到未跟踪");

    repo.discard_paths(&["m.txt"]).expect("丢弃改动");
    let status = repo.status().expect("status");
    assert!(
        !status.entries.iter().any(|e| e.path == "m.txt"),
        "丢弃后 m.txt 应无变更"
    );
    assert_eq!(
        repo.worktree_text("m.txt").expect("读取失败").as_deref(),
        Some("hello\n")
    );
}

#[test]
fn diff_view_sources_handle_untracked_and_deleted() {
    let (dir, repo) = temp_repo();
    write(dir.path(), "tracked.txt", "v1\n");
    write(dir.path(), "gone.txt", "bye\n");
    git(dir.path(), &["add", "-A"]);
    git(dir.path(), &["commit", "-m", "init"]);

    // 未跟踪：index 无、工作区有 → 工作区 diff 视图 old=""
    write(dir.path(), "fresh.txt", "hello\n");
    assert_eq!(repo.index_text("fresh.txt").expect("读取失败"), None);
    assert_eq!(
        repo.worktree_text("fresh.txt")
            .expect("读取失败")
            .as_deref(),
        Some("hello\n")
    );
    assert!(repo
        .file_patch("fresh.txt", false)
        .expect("patch 生成失败")
        .is_none());

    // 工作区删除：index 有、工作区无 → old=index 内容、new=""
    std::fs::remove_file(dir.path().join("gone.txt")).expect("删除");
    assert_eq!(repo.worktree_text("gone.txt").expect("读取失败"), None);
    assert_eq!(
        repo.index_text("gone.txt").expect("读取失败").as_deref(),
        Some("bye\n")
    );

    // HEAD 内容读取
    assert_eq!(
        repo.show("HEAD", "tracked.txt")
            .expect("读取失败")
            .as_deref(),
        Some("v1\n")
    );
    assert_eq!(repo.show("HEAD", "missing.txt").expect("读取失败"), None);
}
