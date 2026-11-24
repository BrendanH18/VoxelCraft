//! v0.18 Track 7: the world icon (`icon.png`), taken the way Java's
//! `GameRenderer.tryTakeScreenshotIfNeeded` does: in a world this game
//! hosts, once a second until the world has an icon file, take the first
//! frame with more than ten sections drawn and nothing left to draw. The
//! frame is copied before its HUD, read back without waiting for the GPU,
//! then cropped to its centred square, shrunk to 64 x 64 and written on a
//! thread of its own through a temporary file, so the world never waits
//! for it and nothing in its save changes. See `voxelcraft::title::icon`.

use std::path::PathBuf;
use std::time::Instant;

use voxelcraft::title::icon::{self, Capture, Step};

use crate::render::Renderer;

pub(super) struct WorldIcon {
    path: PathBuf,
    rule: Capture,
    started: Instant,
    writer: Option<std::thread::JoinHandle<Result<(), String>>>,
}

impl WorldIcon {
    /// The icon of the world saved in `world_dir`.
    pub fn new(world_dir: PathBuf) -> Self {
        Self { path: world_dir.join(icon::FILE), rule: Capture::default(), started: Instant::now(), writer: None }
    }

    /// Before a frame is drawn: finishes earlier work and asks the renderer
    /// for this frame's view when the rule says so. `chunks_drawn` is the
    /// last frame's visible chunk meshes; `settled` that nothing is left to
    /// generate or mesh.
    pub fn before_frame(&mut self, renderer: &mut Renderer, chunks_drawn: usize, settled: bool) {
        if self.writer.as_ref().is_some_and(|w| w.is_finished()) {
            match self.writer.take().unwrap().join() {
                Ok(Ok(())) => log::info!("saved world icon {}", self.path.display()),
                Ok(Err(e)) => log::warn!("couldn't save the world icon {}: {e}", self.path.display()),
                Err(_) => log::warn!("the world icon writer stopped"),
            }
            self.rule.finished();
        }
        if let Some(result) = renderer.poll_icon() {
            match result {
                // Not asked for by this world (left over from another): dropped.
                _ if !self.rule.awaiting() => log::warn!("dropped a world icon this world didn't ask for"),
                Ok((pixels, side)) => {
                    let path = self.path.clone();
                    self.writer = Some(std::thread::spawn(move || {
                        icon::write(&path, &icon::downscale(&pixels, side, side as usize * 4))
                    }));
                }
                Err(e) => {
                    log::warn!("couldn't take the world icon, giving up for this visit: {e}");
                    self.rule.give_up();
                }
            }
        }
        let now = self.started.elapsed().as_millis() as u64;
        let sections = chunks_drawn * icon::SECTIONS_PER_CHUNK;
        if self.rule.step(now, true, || self.path.is_file(), sections, settled) == Step::Capture {
            renderer.request_icon();
        }
    }

    /// The world has its icon (found on disk after it was written), or
    /// taking one is impossible this visit.
    pub fn done(&self) -> bool {
        self.rule.is_done()
    }
}
