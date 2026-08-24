//! Chat transcript persistence (doc 04): JSONL per conversation under `~/.flint/chat/`.
//! Best-effort, silent failure — a storage error must never break chat. One conversation
//! in v0 (`current.jsonl`); a multi-conversation store is a later add. Internal helpers
//! take a path so tests run against temp dirs (the canonical `data_dir()` is not a test
//! seam — see `config`).

use std::path::{Path, PathBuf};

use crate::config::data_dir;
use crate::types::ChatMessage;

pub fn conversation_path() -> PathBuf {
    data_dir().join("chat").join("current.jsonl")
}

/// Load persisted messages; corrupt/partial lines are skipped (best-effort).
pub fn load_messages() -> Vec<ChatMessage> {
    load_from(&conversation_path())
}

/// Append one message; create the dir as needed. Failures are swallowed.
pub fn append_message(msg: &ChatMessage) {
    append_to(&conversation_path(), msg);
}

/// Clear the conversation (New chat).
pub fn clear_conversation() {
    let _ = std::fs::remove_file(conversation_path());
}

fn load_from(path: &Path) -> Vec<ChatMessage> {
    let Ok(contents) = std::fs::read_to_string(path) else {
        return vec![];
    };
    contents
        .lines()
        .filter_map(|line| serde_json::from_str::<ChatMessage>(line.trim()).ok())
        .collect()
}

fn append_to(path: &Path, msg: &ChatMessage) {
    if let Some(dir) = path.parent() {
        if std::fs::create_dir_all(dir).is_err() {
            return;
        }
    }
    let Ok(json) = serde_json::to_string(msg) else {
        return;
    };
    use std::io::Write;
    let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) else {
        return;
    };
    let _ = writeln!(f, "{json}");
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn round_trip_persists_messages() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("chat").join("current.jsonl");
        assert!(load_from(&path).is_empty(), "no file → empty");

        append_to(&path, &ChatMessage { role: "user".into(), content: "hi".into(), ts: 1 });
        append_to(&path, &ChatMessage { role: "assistant".into(), content: "hello".into(), ts: 2 });

        let msgs = load_from(&path);
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0].role, "user");
        assert_eq!(msgs[0].content, "hi");
        assert_eq!(msgs[1].role, "assistant");
        assert_eq!(msgs[1].content, "hello");
        assert_eq!(msgs[1].ts, 2);
    }

    #[test]
    fn corrupt_lines_are_skipped() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("current.jsonl");
        append_to(&path, &ChatMessage { role: "user".into(), content: "ok".into(), ts: 1 });
        std::fs::write(&path, format!("{}\nnot-json\n", std::fs::read_to_string(&path).unwrap())).unwrap();
        let msgs = load_from(&path);
        assert_eq!(msgs.len(), 1, "corrupt line must be skipped");
        assert_eq!(msgs[0].content, "ok");
    }

    #[test]
    fn append_creates_dirs() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("a").join("b").join("c.jsonl");
        append_to(&path, &ChatMessage { role: "user".into(), content: "x".into(), ts: 0 });
        assert!(path.exists());
        assert_eq!(load_from(&path).len(), 1);
    }
}
