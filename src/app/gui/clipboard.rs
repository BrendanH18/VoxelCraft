//! The system clipboard for text fields and Copy Seed: `pbcopy`/`pbpaste` on
//! macOS, the Win32 clipboard on Windows, and `wl-copy`/`wl-paste`, `xclip`
//! or `xsel` on Linux, whichever is installed. Clipboard crates were avoided:
//! `arboard`'s Windows backend is BSL-1.0, outside `about.toml`'s licenses.

pub(in crate::app) trait Clipboard {
    fn get(&mut self) -> Option<String>;
    fn set(&mut self, text: &str);
}

pub(in crate::app) struct SystemClipboard;

impl Clipboard for SystemClipboard {
    fn get(&mut self) -> Option<String> {
        platform::get()
    }
    fn set(&mut self, text: &str) {
        platform::set(text)
    }
}

/// How long a clipboard tool gets before it's killed and the next is tried.
#[cfg(not(windows))]
const TOOL_TIME: std::time::Duration = std::time::Duration::from_millis(500);

/// Waits for `child` until `deadline`, killing and reaping it if it's late.
#[cfg(not(windows))]
fn finish(child: &mut std::process::Child, deadline: std::time::Instant) -> Option<std::process::ExitStatus> {
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Some(status),
            Ok(None) if std::time::Instant::now() < deadline => std::thread::sleep(std::time::Duration::from_millis(5)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
}

/// The output of the first of `commands` that succeeds. Each tool gets
/// [`TOOL_TIME`] before it's killed and the next is tried (`wl-paste` can
/// hang on some Wayland desktops), so a paste never freezes the game.
#[cfg(not(windows))]
fn read(commands: &[&[&str]]) -> Option<String> {
    use std::io::Read;
    use std::process::{Command, Stdio};
    commands.iter().find_map(|cmd| {
        let mut child = Command::new(cmd[0])
            .args(&cmd[1..])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .ok()?;
        // Read on a helper thread, so a large clipboard can't fill the pipe
        // and stall the tool before it exits.
        let mut stdout = child.stdout.take()?;
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let mut bytes = Vec::new();
            let _ = tx.send(stdout.read_to_end(&mut bytes).map(|_| bytes));
        });
        let status = finish(&mut child, std::time::Instant::now() + TOOL_TIME)?;
        // The pipe closes once the tool exits, unless something it started
        // still holds it: don't wait on that for long.
        let bytes = rx.recv_timeout(std::time::Duration::from_millis(200)).ok()?.ok()?;
        status.success().then(|| String::from_utf8_lossy(&bytes).into_owned())
    })
}

/// Pipes `text` into the first of `commands` that succeeds, on a thread of
/// its own. The Linux tools fork to serve the selection and exit once it's
/// theirs; one that hasn't exited within [`TOOL_TIME`] is killed and the
/// next is tried.
#[cfg(not(windows))]
fn write(commands: &'static [&'static [&'static str]], text: &str) {
    use std::io::Write;
    use std::process::{Command, Stdio};
    let text = text.to_string();
    std::thread::spawn(move || {
        for cmd in commands {
            let Ok(mut child) = Command::new(cmd[0])
                .args(&cmd[1..])
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
            else {
                continue;
            };
            // Dropping stdin closes it, so the tool sees the end of the text.
            let written = child.stdin.take().is_some_and(|mut stdin| stdin.write_all(text.as_bytes()).is_ok());
            let status = finish(&mut child, std::time::Instant::now() + TOOL_TIME);
            if written && status.is_some_and(|s| s.success()) {
                return;
            }
        }
        log::warn!("couldn't copy: no clipboard tool worked (install wl-clipboard, xclip or xsel)");
    });
}

#[cfg(target_os = "macos")]
mod platform {
    pub fn get() -> Option<String> {
        super::read(&[&["pbpaste"]])
    }
    pub fn set(text: &str) {
        super::write(&[&["pbcopy"]], text)
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
mod platform {
    pub fn get() -> Option<String> {
        super::read(&[
            &["wl-paste", "--no-newline"],
            &["xclip", "-selection", "clipboard", "-o"],
            &["xsel", "--clipboard", "--output"],
        ])
    }
    pub fn set(text: &str) {
        super::write(&[&["wl-copy"], &["xclip", "-selection", "clipboard"], &["xsel", "--clipboard", "--input"]], text)
    }
}

#[cfg(windows)]
mod platform {
    use std::ffi::c_void;
    use std::ptr::null_mut;

    type Handle = *mut c_void;
    const CF_UNICODETEXT: u32 = 13;
    const GMEM_MOVEABLE: u32 = 2;

    #[link(name = "user32")]
    unsafe extern "system" {
        fn OpenClipboard(owner: Handle) -> i32;
        fn CloseClipboard() -> i32;
        fn EmptyClipboard() -> i32;
        fn GetClipboardData(format: u32) -> Handle;
        fn SetClipboardData(format: u32, memory: Handle) -> Handle;
        fn GetActiveWindow() -> Handle;
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GlobalAlloc(flags: u32, bytes: usize) -> Handle;
        fn GlobalLock(memory: Handle) -> *mut c_void;
        fn GlobalUnlock(memory: Handle) -> i32;
        fn GlobalFree(memory: Handle) -> Handle;
        fn GlobalSize(memory: Handle) -> usize;
    }

    pub fn get() -> Option<String> {
        // SAFETY: the clipboard is opened and closed around the read, and the
        // locked handle is only read within its allocation (some programs
        // leave out the terminating zero).
        unsafe {
            if OpenClipboard(null_mut()) == 0 {
                return None;
            }
            let data = GetClipboardData(CF_UNICODETEXT);
            let text = if data.is_null() {
                None
            } else {
                let p = GlobalLock(data) as *const u16;
                if p.is_null() {
                    None
                } else {
                    let units = std::slice::from_raw_parts(p, GlobalSize(data) / 2);
                    let n = units.iter().position(|&u| u == 0).unwrap_or(units.len());
                    let text = String::from_utf16_lossy(&units[..n]);
                    GlobalUnlock(data);
                    Some(text)
                }
            };
            CloseClipboard();
            text
        }
    }

    pub fn set(text: &str) {
        let wide: Vec<u16> = text.encode_utf16().chain([0]).collect();
        // SAFETY: the allocation is `wide.len()` u16s long; the clipboard owns
        // it once SetClipboardData succeeds, and it's freed otherwise. The
        // game window owns the clipboard: with no owner, EmptyClipboard makes
        // SetClipboardData fail.
        unsafe {
            if OpenClipboard(GetActiveWindow()) == 0 {
                return;
            }
            EmptyClipboard();
            let memory = GlobalAlloc(GMEM_MOVEABLE, wide.len() * 2);
            if !memory.is_null() {
                let p = GlobalLock(memory) as *mut u16;
                if p.is_null() {
                    GlobalFree(memory);
                } else {
                    std::ptr::copy_nonoverlapping(wide.as_ptr(), p, wide.len());
                    GlobalUnlock(memory);
                    if SetClipboardData(CF_UNICODETEXT, memory).is_null() {
                        GlobalFree(memory);
                    }
                }
            }
            CloseClipboard();
        }
    }
}
