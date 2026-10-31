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
