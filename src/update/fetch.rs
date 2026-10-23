//! Where releases come from. Official builds ask GitHub through the system's
//! `curl` (built into macOS and Windows 10 and later), which brings the
//! operating system's TLS and certificate store without adding a TLS stack
//! to the game. `--update-source <folder>` reads a local fixture instead.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

const LATEST: &str = "https://api.github.com/repos/BrendanH18/VoxelCraft/releases/latest";
/// Every official download lives under this prefix; GitHub redirects it to
/// its download host, over HTTPS only.
const DOWNLOADS: &str = "https://github.com/BrendanH18/VoxelCraft/releases/download/";

#[derive(Clone, Debug)]
pub enum Source {
    GitHub,
    /// A folder holding `latest.json` (GitHub's release JSON, with each
    /// asset's `browser_download_url` naming a file in the folder) and the
    /// files it lists. For tests and release rehearsals.
    Folder(PathBuf),
}

impl Source {
    /// The latest release's JSON.
    pub fn latest(&self) -> Result<String, String> {
        match self {
            Source::GitHub => {
                let out = curl(LATEST, 20)?
                    .args(["--header", "Accept: application/vnd.github+json"])
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .output()
                    .map_err(|e| format!("could not run curl: {e}"))?;
                if !out.status.success() {
                    return Err(curl_error(&out.stderr));
                }
                String::from_utf8(out.stdout).map_err(|_| "GitHub's reply was not text".into())
            }
            Source::Folder(dir) => std::fs::read_to_string(dir.join("latest.json"))
                .map_err(|e| format!("could not read {}: {e}", dir.join("latest.json").display())),
        }
    }

    /// Downloads `url` to `dest`, calling `progress` with the bytes so far.
    /// Stops early, with an error, once `cancel` is set.
    pub fn download(
        &self,
        url: &str,
        dest: &Path,
        cancel: &AtomicBool,
        mut progress: impl FnMut(u64),
    ) -> Result<(), String> {
        let partial = dest.with_extension("part");
        let _ = std::fs::remove_file(&partial);
        match self {
            Source::GitHub => {
                if !url.starts_with(DOWNLOADS) {
                    return Err(format!("refusing to download from {url}"));
                }
                let mut child = curl(url, 3600)?
                    // Give up on a stalled connection rather than hang.
                    .args(["--speed-limit", "1024", "--speed-time", "60", "--output"])
                    .arg(&partial)
                    .stdout(Stdio::null())
                    .stderr(Stdio::piped())
                    .spawn()
                    .map_err(|e| format!("could not run curl: {e}"))?;
                let status = loop {
                    if cancel.load(Ordering::Relaxed) {
                        let _ = child.kill();
                        let _ = child.wait();
                        let _ = std::fs::remove_file(&partial);
                        return Err(CANCELLED.into());
                    }
                    if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
                        break status;
                    }
                    progress(std::fs::metadata(&partial).map_or(0, |m| m.len()));
                    std::thread::sleep(Duration::from_millis(100));
                };
                if !status.success() {
                    let mut stderr = Vec::new();
                    if let Some(mut pipe) = child.stderr.take() {
                        use std::io::Read;
                        let _ = pipe.read_to_end(&mut stderr);
                    }
                    let _ = std::fs::remove_file(&partial);
                    return Err(curl_error(&stderr));
                }
            }
            Source::Folder(dir) => {
                if cancel.load(Ordering::Relaxed) {
                    return Err(CANCELLED.into());
                }
                // Only plain file names inside the folder.
                if url.contains(['/', '\\']) || url.starts_with('.') {
                    return Err(format!("refusing to read {url} outside the update folder"));
                }
                std::fs::copy(dir.join(url), &partial).map_err(|e| format!("could not copy {url}: {e}"))?;
            }
        }
        progress(std::fs::metadata(&partial).map_or(0, |m| m.len()));
        std::fs::rename(&partial, dest).map_err(|e| format!("could not save the download: {e}"))
    }
}

pub const CANCELLED: &str = "cancelled";

fn curl_path() -> PathBuf {
    #[cfg(windows)]
    {
        let root = std::env::var_os("SystemRoot").unwrap_or_else(|| "C:\\Windows".into());
        PathBuf::from(root).join("System32").join("curl.exe")
    }
    #[cfg(not(windows))]
    {
        PathBuf::from("/usr/bin/curl")
    }
}

/// A `curl` command for `url` over HTTPS only, failing on HTTP errors.
fn curl(url: &str, max_seconds: u32) -> Result<Command, String> {
    let path = curl_path();
    if !path.exists() {
        return Err(format!("{} is missing, so the game can't check for updates", path.display()));
    }
    let mut command = Command::new(path);
    command
        .args(["--fail", "--location", "--silent", "--show-error"])
        .args(["--proto", "=https", "--proto-redir", "=https", "--connect-timeout", "15"])
        .args(["--max-time", &max_seconds.to_string()])
        .args(["--user-agent", concat!("VoxelCraft/", env!("CARGO_PKG_VERSION"))])
        .arg(url)
        .stdin(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // The game is a windowed program; don't flash a console.
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    Ok(command)
}

fn curl_error(stderr: &[u8]) -> String {
    let text = String::from_utf8_lossy(stderr);
    let text = text.trim().trim_start_matches("curl: ");
    if text.is_empty() { "the download failed".into() } else { text.to_string() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folder_source_reads_and_copies_plain_names() {
        let dir = std::env::temp_dir().join(format!("voxelcraft-fetch-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("latest.json"), "{}").unwrap();
        std::fs::write(dir.join("pkg"), "12345").unwrap();
        let source = Source::Folder(dir.clone());
        assert_eq!(source.latest().unwrap(), "{}");
        let dest = dir.join("out");
        let mut seen = 0;
        source.download("pkg", &dest, &AtomicBool::new(false), |n| seen = n).unwrap();
        assert_eq!((std::fs::read_to_string(&dest).unwrap().as_str(), seen), ("12345", 5));
        assert!(source.download("../pkg", &dest, &AtomicBool::new(false), |_| {}).is_err());
        assert_eq!(source.download("pkg", &dest, &AtomicBool::new(true), |_| {}), Err(CANCELLED.into()));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn github_downloads_stay_on_the_release_host() {
        let dest = std::env::temp_dir().join("voxelcraft-never-written");
        let err = Source::GitHub.download("https://example.com/x.exe", &dest, &AtomicBool::new(false), |_| {});
        assert!(err.unwrap_err().starts_with("refusing"));
    }
}
