//! Pure-Rust model downloader (doc 02): a resumable GGUF fetch from Hugging Face over
//! HTTP Range, sha256-verified against the cookbook's pinned digest. Replaces the Ollama
//! `/api/pull` consent path in Phase 3b. No external downloader binary — `reqwest` only.
//!
//! Layout: downloads write to `<file>.part`, resume from its current size, and only on a
//! passing digest check are renamed to the final `<file>`. A present-but-corrupt final file
//! is reported (the caller's delete affordance is the recovery path).

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use futures_util::StreamExt;
use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;

use crate::error::AppError;
use crate::types::PullProgress;

/// Download `url` to `dest` (resumable), verifying against `expected_sha256`.
/// Emits `PullProgress` samples (status/completed/total/percent) — the same shape the
/// install flow already renders. Cancellation is checked between chunks.
pub async fn download(
    client: &reqwest::Client,
    url: &str,
    dest: &Path,
    expected_sha256: &str,
    progress: &mut (dyn FnMut(PullProgress) + Send),
    cancel: &AtomicBool,
) -> Result<(), AppError> {
    if dest.exists() {
        if sha256_hex(dest).await? == expected_sha256 {
            progress(PullProgress {
                status: "success".into(),
                completed: file_len(dest).await?,
                total: file_len(dest).await?,
                percent: 100.0,
            });
            return Ok(());
        }
        return Err(AppError::Engine(format!(
            "existing model failed verification ({}); delete it and retry",
            dest.display()
        )));
    }

    let part = part_path(dest);
    let mut offset = file_len(&part).await?;

    // Retry once if the server rejects our offset (416 → file shrank on the CDN).
    let mut attempts = 0;
    let (resp, total) = loop {
        let mut req = client.get(url);
        if offset > 0 {
            req = req.header("Range", format!("bytes={offset}-"));
        }
        let resp = req
            .send()
            .await
            .map_err(|e| AppError::Engine(format!("download start: {e}")))?;
        let status = resp.status();
        match status {
            reqwest::StatusCode::PARTIAL_CONTENT => {
                let total = content_range_total(&resp)
                    .ok_or_else(|| AppError::Engine("download: missing Content-Range".into()))?;
                break (resp, total);
            }
            reqwest::StatusCode::OK => {
                // Server ignored Range (or a fresh start) — restart the file from zero.
                if offset > 0 {
                    tokio::fs::remove_file(&part).await.map_err(|e| AppError::Io {
                        path: part.clone(),
                        source: e,
                    })?;
                    offset = 0;
                    attempts += 1;
                    if attempts > 1 {
                        return Err(AppError::Engine(
                            "download: server kept ignoring Range".into(),
                        ));
                    }
                    continue;
                }
                let total = resp
                    .content_length()
                    .ok_or_else(|| AppError::Engine("download: no Content-Length".into()))?;
                break (resp, total);
            }
            reqwest::StatusCode::RANGE_NOT_SATISFIABLE => {
                tokio::fs::remove_file(&part).await.map_err(|e| AppError::Io {
                    path: part.clone(),
                    source: e,
                })?;
                offset = 0;
                attempts += 1;
                if attempts > 1 {
                    return Err(AppError::Engine("download: unsatisfiable Range".into()));
                }
                continue;
            }
            other => {
                return Err(AppError::Engine(format!("download http {other}")));
            }
        }
    };

    let mut file = tokio::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&part)
        .await
        .map_err(|e| AppError::Io { path: part.clone(), source: e })?;

    let mut hasher = Sha256::new();
    if offset > 0 {
        hash_file_prefix(&part, offset, &mut hasher)?;
    }

    let mut stream = resp.bytes_stream();
    let mut written: u64 = offset;
    let mut last_emit = Instant::now() - std::time::Duration::from_secs(1);
    while let Some(chunk) = stream.next().await {
        if cancel.load(Ordering::Relaxed) {
            return Err(AppError::Engine("download cancelled".into()));
        }
        let chunk = chunk.map_err(|e| AppError::Engine(format!("download stream: {e}")))?;
        file.write_all(&chunk)
            .await
            .map_err(|e| AppError::Io { path: part.clone(), source: e })?;
        hasher.update(&chunk);
        written += chunk.len() as u64;
        if last_emit.elapsed() >= std::time::Duration::from_millis(250) || written >= total {
            progress(PullProgress {
                status: "downloading".into(),
                completed: written,
                total,
                percent: pct(written, total),
            });
            last_emit = Instant::now();
        }
    }
    file.flush().await.map_err(|e| AppError::Io { path: part.clone(), source: e })?;

    progress(PullProgress {
        status: "verifying sha256 digest".into(),
        completed: written,
        total,
        percent: 100.0,
    });

    let digest = hex(&hasher.finalize());
    if digest != expected_sha256 {
        let _ = tokio::fs::remove_file(&part).await;
        return Err(AppError::Engine(format!(
            "sha256 mismatch: got {digest}, expected {expected_sha256}"
        )));
    }
    tokio::fs::rename(&part, dest)
        .await
        .map_err(|e| AppError::Io { path: dest.to_path_buf(), source: e })?;

    progress(PullProgress {
        status: "success".into(),
        completed: written,
        total,
        percent: 100.0,
    });
    Ok(())
}

/// The digest of an already-downloaded file (the "already installed" check).
pub async fn sha256_hex(path: &Path) -> Result<String, AppError> {
    let mut f = tokio::fs::File::open(path)
        .await
        .map_err(|e| AppError::Io { path: path.to_path_buf(), source: e })?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        use tokio::io::AsyncReadExt;
        let n = f
            .read(&mut buf)
            .await
            .map_err(|e| AppError::Io { path: path.to_path_buf(), source: e })?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex(&hasher.finalize()))
}

fn hash_file_prefix(path: &Path, bytes: u64, hasher: &mut Sha256) -> Result<(), AppError> {
    use std::io::Read;
    let f = std::fs::File::open(path).map_err(|e| AppError::Io {
        path: path.to_path_buf(),
        source: e,
    })?;
    let mut f = f.take(bytes);
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let n = f
            .read(&mut buf)
            .map_err(|e| AppError::Io { path: path.to_path_buf(), source: e })?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(())
}

fn part_path(dest: &Path) -> PathBuf {
    let mut p = dest.as_os_str().to_owned();
    p.push(".part");
    PathBuf::from(p)
}

async fn file_len(path: &Path) -> Result<u64, AppError> {
    match tokio::fs::metadata(path).await {
        Ok(m) => Ok(m.len()),
        Err(_) => Ok(0),
    }
}

/// Parse `Content-Range: bytes <start>-<end>/<total>` → total.
fn content_range_total(resp: &reqwest::Response) -> Option<u64> {
    let v = resp.headers().get(reqwest::header::CONTENT_RANGE)?.to_str().ok()?;
    let total = v.rsplit('/').next()?;
    total.trim().parse().ok()
}

fn pct(completed: u64, total: u64) -> f64 {
    if total == 0 {
        return 0.0;
    }
    ((completed as f64 / total as f64) * 1000.0).round() / 10.0
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use httpmock::Method::GET;
    use httpmock::prelude::*;
    use std::sync::atomic::AtomicBool;
    use tempfile::tempdir;

    fn sha256_hex_str(s: &str) -> String {
        hex(&Sha256::digest(s.as_bytes()))
    }

    #[tokio::test]
    async fn fresh_download_verifies_and_renames() {
        let dir = tempdir().unwrap();
        let dest = dir.path().join("model.gguf");
        let body = "hello model bytes";
        let digest = sha256_hex_str(body);
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(GET).path("/model.gguf");
            then.status(200)
                .header("content-length", body.len().to_string())
                .body(body);
        });
        let client = reqwest::Client::new();
        let mut progress: Vec<PullProgress> = Vec::new();
        let cancel = AtomicBool::new(false);
        download(
            &client,
            &format!("{}/model.gguf", server.base_url()),
            &dest,
            &digest,
            &mut |p| progress.push(p),
            &cancel,
        )
        .await
        .expect("download ok");
        assert!(dest.exists(), "renamed to final path");
        assert!(!part_path(&dest).exists(), "part removed");
        assert_eq!(progress.last().unwrap().percent, 100.0);
        assert_eq!(progress.last().unwrap().status, "success");
        mock.assert();
    }

    #[tokio::test]
    async fn resumable_from_partial_part() {
        let dir = tempdir().unwrap();
        let dest = dir.path().join("model.gguf");
        let full = "0123456789abcdef";
        let digest = sha256_hex_str(full);
        // Pre-seed a .part with the first half.
        let part = part_path(&dest);
        std::fs::write(&part, "01234567").unwrap();
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(GET)
                .path("/model.gguf")
                .header("range", "bytes=8-");
            then.status(206)
                .header("content-range", "bytes 8-15/16")
                .body("89abcdef");
        });
        let client = reqwest::Client::new();
        let cancel = AtomicBool::new(false);
        download(
            &client,
            &format!("{}/model.gguf", server.base_url()),
            &dest,
            &digest,
            &mut |_| {},
            &cancel,
        )
        .await
        .expect("resume ok");
        assert!(dest.exists());
        assert_eq!(std::fs::read_to_string(&dest).unwrap(), full);
        mock.assert();
    }

    #[tokio::test]
    async fn corrupt_digest_fails_and_cleans_part() {
        let dir = tempdir().unwrap();
        let dest = dir.path().join("model.gguf");
        let body = "some bytes";
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET).path("/model.gguf");
            then.status(200).body(body);
        });
        let client = reqwest::Client::new();
        let cancel = AtomicBool::new(false);
        let res = download(
            &client,
            &format!("{}/model.gguf", server.base_url()),
            &dest,
            &sha256_hex_str("different bytes"),
            &mut |_| {},
            &cancel,
        )
        .await;
        assert!(res.is_err());
        assert!(res.unwrap_err().to_string().contains("sha256 mismatch"));
        assert!(!part_path(&dest).exists(), "corrupt part removed");
    }

    #[tokio::test]
    async fn already_installed_is_a_noop() {
        let dir = tempdir().unwrap();
        let dest = dir.path().join("model.gguf");
        let body = "installed model";
        std::fs::write(&dest, body).unwrap();
        // No server involved — a verified dest must short-circuit before any request.
        let client = reqwest::Client::new();
        let cancel = AtomicBool::new(false);
        download(
            &client,
            "http://127.0.0.1:1/model.gguf",
            &dest,
            &sha256_hex_str(body),
            &mut |_| {},
            &cancel,
        )
        .await
        .expect("noop ok");
    }

    #[tokio::test]
    async fn cancel_aborts() {
        let dir = tempdir().unwrap();
        let dest = dir.path().join("model.gguf");
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET).path("/model.gguf");
            then.status(200).body("x".repeat(1024 * 1024));
        });
        let client = reqwest::Client::new();
        let cancel = AtomicBool::new(true);
        let res = download(
            &client,
            &format!("{}/model.gguf", server.base_url()),
            &dest,
            &sha256_hex_str("y"),
            &mut |_| {},
            &cancel,
        )
        .await;
        assert!(res.is_err());
        assert!(res.unwrap_err().to_string().contains("cancelled"));
    }
}
