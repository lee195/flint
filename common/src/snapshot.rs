//! Pre-run workspace snapshot (doc 04 + spark doc 07): a detached-git repo per workspace
//! under `~/.flint/snapshots/<hash>/` whose work tree IS the user's workspace folder
//! (never touched — no `.git/` created inside it). Snapshot before each file-writing agent
//! run; "restore" checks the working files back to the pre-run commit. Uses `git2`, never
//! system git (Macs don't ship git; the safety net must not depend on it).

use std::path::{Path, PathBuf};

use git2::build::CheckoutBuilder;
use git2::{IndexAddOption, Repository, ResetType, Signature};

use crate::config::data_dir;
use crate::error::AppError;

/// Keep at most this many snapshot repos, oldest evicted on the next snapshot.
const MAX_SNAPSHOT_REPOS: usize = 10;

pub fn snapshot_root() -> PathBuf {
    data_dir().join("snapshots")
}

/// Snapshot the workspace before a run. Returns the pre-run commit to restore against.
pub fn before_run(workspace: &Path, label: &str) -> Result<git2::Oid, AppError> {
    before_run_in(&snapshot_root(), workspace, label)
}

fn repo_path_for(root: &Path, workspace: &Path) -> PathBuf {
    root.join(workspace_hash(workspace))
}

/// Stable short hash of the workspace path (FNV-1a, hex). A moved/renamed workspace is
/// treated as new (spark doc 07's hash-by-path stance).
fn workspace_hash(workspace: &Path) -> String {
    use std::hash::Hasher;
    let mut h = std::collections::hash_map::DefaultHasher::new();
    std::hash::Hash::hash(&workspace.to_string_lossy(), &mut h);
    format!("{:016x}", h.finish())
}

fn signature() -> Signature<'static> {
    // The safety net's commits are ours, not the user's — a stable synthetic identity.
    Signature::now("Flint safety net", "flint@local")
        .expect("signature")
}

fn before_run_in(root: &Path, workspace: &Path, label: &str) -> Result<git2::Oid, AppError> {
    let repo_path = repo_path_for(root, workspace);
    std::fs::create_dir_all(&repo_path).map_err(|e| AppError::Io {
        path: repo_path.clone(),
        source: e,
    })?;

    let repo = match Repository::open(&repo_path) {
        Ok(r) => {
            r.set_workdir(workspace, false)
                .map_err(|e| AppError::Engine(format!("snapshot workdir: {e}")))?;
            r
        }
        Err(_) => {
            let r = Repository::init(&repo_path).map_err(|e| {
                AppError::Engine(format!("snapshot init: {e}"))
            })?;
            r.set_workdir(workspace, false)
                .map_err(|e| AppError::Engine(format!("snapshot workdir: {e}")))?;
            r
        }
    };

    // Stage the current workspace state.
    let mut idx = repo.index().map_err(|e| AppError::Engine(format!("snapshot index: {e}")))?;
    idx.add_all(["*"], IndexAddOption::DEFAULT, None)
        .map_err(|e| AppError::Engine(format!("snapshot add: {e}")))?;
    idx.write().map_err(|e| AppError::Engine(format!("snapshot index write: {e}")))?;
    let tree = idx.write_tree().map_err(|e| AppError::Engine(format!("snapshot tree: {e}")))?;

    let sig = signature();
    let parent = repo.head().ok().and_then(|h| h.target());
    let parents: Vec<git2::Commit> = parent
        .iter()
        .map(|p| repo.find_commit(*p))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| AppError::Engine(format!("snapshot parent: {e}")))?;
    let parent_refs: Vec<&git2::Commit> = parents.iter().collect();
    let tree_obj = repo
        .find_tree(tree)
        .map_err(|e| AppError::Engine(format!("snapshot tree find: {e}")))?;
    let commit = repo
        .commit(Some("HEAD"), &sig, &sig, label, &tree_obj, &parent_refs)
        .map_err(|e| AppError::Engine(format!("snapshot commit: {e}")))?;

    prune_old_repos(root);
    Ok(commit)
}

/// Restore the workspace to a pre-run snapshot commit (spark doc 07's undo).
pub fn restore(workspace: &Path, commit_id: &str) -> Result<(), AppError> {
    restore_in(&snapshot_root(), workspace, commit_id)
}

fn restore_in(root: &Path, workspace: &Path, commit_id: &str) -> Result<(), AppError> {
    let repo_path = repo_path_for(root, workspace);
    let repo = Repository::open(&repo_path)
        .map_err(|e| AppError::Engine(format!("snapshot open: {e}")))?;
    repo.set_workdir(workspace, false)
        .map_err(|e| AppError::Engine(format!("snapshot workdir: {e}")))?;
    let oid = git2::Oid::from_str(commit_id)
        .map_err(|e| AppError::Engine(format!("snapshot id: {e}")))?;
    let commit = repo.find_commit(oid).map_err(|e| AppError::Engine(format!("snapshot commit: {e}")))?;
    let tree = commit.tree().map_err(|e| AppError::Engine(format!("snapshot tree: {e}")))?;

    // Reset index to the snapshot, then force-checkout the working files (spark doc 07).
    let obj = repo
        .find_object(commit.id(), None)
        .map_err(|e| AppError::Engine(format!("snapshot obj: {e}")))?;
    repo.reset(&obj, ResetType::Mixed, None)
        .map_err(|e| AppError::Engine(format!("snapshot reset: {e}")))?;
    let mut cb = CheckoutBuilder::new();
    cb.force();
    cb.remove_untracked(true);
    let tree_obj = repo
        .find_object(tree.id(), None)
        .map_err(|e| AppError::Engine(format!("snapshot tree obj: {e}")))?;
    repo.checkout_tree(&tree_obj, Some(&mut cb))
        .map_err(|e| AppError::Engine(format!("snapshot restore: {e}")))?;
    Ok(())
}

/// Whether a snapshot repo exists for this workspace (for the review step).
pub fn has_snapshot(workspace: &Path) -> bool {
    Repository::open(repo_path_for(&snapshot_root(), workspace)).is_ok()
}

fn prune_old_repos(root: &Path) {
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    let mut dirs: Vec<PathBuf> = entries
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.is_dir())
        .collect();
    if dirs.len() <= MAX_SNAPSHOT_REPOS {
        return;
    }
    dirs.sort_by_key(|p| std::fs::metadata(p).and_then(|m| m.modified()).ok());
    for old in dirs.iter().take(dirs.len() - MAX_SNAPSHOT_REPOS) {
        let _ = std::fs::remove_dir_all(old);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn write(path: &Path, name: &str, content: &str) {
        std::fs::write(path.join(name), content).unwrap();
    }

    #[test]
    fn snapshot_then_restore_round_trip() {
        let dir = tempdir().unwrap();
        let root = dir.path().join("snap");
        let ws = dir.path().join("ws");
        std::fs::create_dir_all(&ws).unwrap();
        write(&ws, "a.txt", "one");

        let commit = before_run_in(&root, &ws, "pre-run").unwrap();
        // the agent "changes" the workspace
        write(&ws, "a.txt", "changed");
        write(&ws, "b.txt", "new");

        restore_in(&root, &ws, &commit.to_string()).unwrap();
        assert_eq!(std::fs::read_to_string(ws.join("a.txt")).unwrap(), "one");
        assert!(!ws.join("b.txt").exists(), "restore must remove files added by the run");
    }

    #[test]
    fn snapshot_does_not_create_git_in_workspace() {
        let dir = tempdir().unwrap();
        let root = dir.path().join("snap");
        let ws = dir.path().join("ws");
        std::fs::create_dir_all(&ws).unwrap();
        write(&ws, "a.txt", "x");
        before_run_in(&root, &ws, "pre").unwrap();
        assert!(!ws.join(".git").exists(), "the user's folder must stay untouched");
        assert!(Repository::open(root.join(workspace_hash(&ws))).is_ok());
    }
}
