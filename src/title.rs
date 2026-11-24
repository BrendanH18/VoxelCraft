//! v0.18 Track 7: the rules behind the title screen's panorama and the world
//! list's icons, kept here so they are tested without a window and checked
//! against Java 26.3 (`tests/fixtures/java/26.3/title.json`,
//! `tests/title_identity.rs`). The app draws them; see
//! `docs/verification/2026-11-23-v0.18-title.md`.

/// Java's title panorama (`Panorama`, `CubeMap`, `GuiRenderer`): a cube of
/// six pre-rendered faces seen from its centre, turning slowly.
pub mod panorama {
    use glam::Vec3;

    /// `CubeMap.render`'s vertical field of view.
    pub const FOV_DEGREES: f32 = 85.0;
    /// `GuiRenderer` draws the cube tilted down by this much.
    pub const PITCH_DEGREES: f32 = 10.0;
    /// `Panorama.extractRenderState`: degrees turned per tick of real time
    /// at Panorama Scroll Speed 100%.
    pub const SPIN_PER_TICK: f32 = 0.1;
    pub const TICKS_PER_SECOND: f32 = 20.0;
    /// VoxelCraft's faces are square PNGs of this size (Java's are 1024).
    pub const FACE_SIZE: u32 = 1024;
    /// Java's default Menu Background Blur (`menuBackgroundBlurriness`).
    /// Screens over the panorama other than the title menu blur it.
    pub const DEFAULT_BLUR_RADIUS: u32 = 5;

    /// `Mth.wrapDegrees`: an angle in [-180, 180).
    pub fn wrap_degrees(angle: f32) -> f32 {
        let a = angle % 360.0;
        let a = if a >= 180.0 { a - 360.0 } else { a };
        if a < -180.0 { a + 360.0 } else { a }
    }

    /// The spin after `seconds` of real time at Panorama Scroll Speed
    /// `speed` (0..1), as Java adds `realtimeDeltaTicks * speed * 0.1`.
    pub fn advance(spin: f32, seconds: f32, speed: f64) -> f32 {
        let ticks = seconds * TICKS_PER_SECOND;
        wrap_degrees(spin + (ticks as f64 * speed) as f32 * SPIN_PER_TICK)
    }

    /// One face of the cube: the direction its centre looks along, the
    /// image's up and right (`right = forward x up`).
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct Face {
        pub forward: Vec3,
        pub up: Vec3,
    }

    impl Face {
        pub fn right(&self) -> Vec3 {
            self.forward.cross(self.up)
        }
    }

    /// `panorama_0` .. `panorama_5`: north, east, south, west, up, down,
    /// Java's order for its six images. Each is a 90-degree square view
    /// from the cube's centre; the game renders them with this up vector.
    pub const FACES: [Face; 6] = [
        Face { forward: Vec3::NEG_Z, up: Vec3::Y },
        Face { forward: Vec3::X, up: Vec3::Y },
        Face { forward: Vec3::Z, up: Vec3::Y },
        Face { forward: Vec3::NEG_X, up: Vec3::Y },
        Face { forward: Vec3::Y, up: Vec3::Z },
        Face { forward: Vec3::NEG_Y, up: Vec3::NEG_Z },
    ];

    /// The face a direction falls on and its texture coordinates (0..1,
    /// v down), as `panorama.wgsl` samples them.
    pub fn lookup(dir: Vec3) -> (usize, [f32; 2]) {
        let a = dir.abs();
        let face = if a.x >= a.y && a.x >= a.z {
            if dir.x > 0.0 { 1 } else { 3 }
        } else if a.y >= a.z {
            if dir.y > 0.0 { 4 } else { 5 }
        } else if dir.z > 0.0 {
            2
        } else {
            0
        };
        let f = FACES[face];
        let p = dir / dir.dot(f.forward);
        (face, [(p.dot(f.right()) + 1.0) * 0.5, (1.0 - p.dot(f.up)) * 0.5])
    }

    /// The direction through the centre of pixel (x, y) of a face `size`
    /// pixels square: the inverse of [`lookup`].
    pub fn direction(face: usize, x: u32, y: u32, size: u32) -> Vec3 {
        let f = FACES[face];
        let u = (x as f32 + 0.5) / size as f32 * 2.0 - 1.0;
        let v = 1.0 - (y as f32 + 0.5) / size as f32 * 2.0;
        (f.forward + f.right() * u + f.up * v).normalize()
    }

    /// The camera's view direction and up at `spin` (Java turns the cube by
    /// `-spin` about Y after tilting it down by [`PITCH_DEGREES`]): the
    /// panorama starts facing north (face 0) and a growing spin turns the
    /// view clockwise seen from above, so the scenery drifts left.
    pub fn view(spin: f32) -> (Vec3, Vec3) {
        let yaw = spin.to_radians();
        let pitch = (-PITCH_DEGREES).to_radians();
        let forward = Vec3::new(yaw.sin() * pitch.cos(), pitch.sin(), -yaw.cos() * pitch.cos());
        let right = Vec3::new(yaw.cos(), 0.0, yaw.sin());
        (forward, right.cross(forward).normalize())
    }

    /// How far the blur's samples reach, in screen pixels per unit of Menu
    /// Background Blur (VoxelCraft's match by eye of Java's default of 5).
    pub const BLUR_SPREAD: f32 = 2.0;

    /// The mip level that blurs the panorama by about `radius` screen pixels
    /// in a window `height` pixels tall: an approximation of Java's
    /// two-pass blur of the screen (see the verification record).
    pub fn blur_lod(radius: u32, height: f32, face_size: u32) -> f32 {
        if radius == 0 || height <= 0.0 {
            return 0.0;
        }
        let focal = height * 0.5 / (FOV_DEGREES.to_radians() * 0.5).tan();
        let pixels_per_texel = focal / (face_size as f32 * 0.5);
        (2.0 * radius as f32 / pixels_per_texel).max(1.0).log2()
    }
}

/// World icons (Java's `icon.png`): captured from the first frames of a
/// world, shown 32 x 32 in Select World.
pub mod icon {
    /// The file in each world folder (`LevelResource.ICON_FILE`).
    pub const FILE: &str = "icon.png";
    /// Java's icon is 64 x 64 (`FaviconTexture`, `GameRenderer`).
    pub const SIZE: u32 = 64;
    /// Drawn this size in the world list, in GUI pixels.
    pub const LIST_SIZE: f32 = 32.0;
    /// `GameRenderer.tryTakeScreenshotIfNeeded` tries once a second.
    pub const ATTEMPT_INTERVAL_MS: u64 = 1000;
    /// `takeAutoScreenshot` waits for more than this many rendered sections
    /// (16 x 16 x 16) and every section rendered.
    pub const MIN_RENDERED_SECTIONS: usize = 10;
    /// Sections in one of VoxelCraft's 32-block chunk meshes.
    pub const SECTIONS_PER_CHUNK: usize = 8;
    /// The world list's hover wash over the icon (`0xA0909090`).
    pub const HOVER_FILL: u32 = 0xA090_9090;
    /// Larger files are not read (an icon is a few kilobytes); Java has no
    /// limit, but the list reads files from a folder anyone can write to.
    pub const MAX_FILE_BYTES: u64 = 1 << 20;

    /// What the capture rule does this frame.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum Step {
        /// Nothing to do (too soon, or already done).
        Wait,
        /// Take the icon from this frame.
        Capture,
    }

    /// Java's first-join capture (`GameRenderer.tryTakeScreenshotIfNeeded`
    /// and `takeAutoScreenshot`): in a world this game hosts, once a second,
    /// stop if the world already has an icon file; otherwise, once more than
    /// ten sections are drawn and every one is, capture the frame. The flag
    /// resets when a world is entered, so Reset Icon (which deletes the
    /// file) makes the next visit capture a new one.
    #[derive(Clone, Debug, Default)]
    pub struct Capture {
        done: bool,
        last_attempt_ms: Option<u64>,
        /// A capture is being read back or written; never two at once.
        in_flight: bool,
    }

    impl Capture {
        /// One frame's check. `now_ms` is a monotonic clock; `local` is
        /// false for a LAN client (Java's `isLocalServer`). `icon_exists`
        /// is asked at most once a second.
        pub fn step(
            &mut self,
            now_ms: u64,
            local: bool,
            icon_exists: impl FnOnce() -> bool,
            rendered_sections: usize,
            all_rendered: bool,
        ) -> Step {
            if self.done || !local || self.in_flight {
                return Step::Wait;
            }
            if self.last_attempt_ms.is_some_and(|t| now_ms.saturating_sub(t) < ATTEMPT_INTERVAL_MS) {
                return Step::Wait;
            }
            self.last_attempt_ms = Some(now_ms);
            if icon_exists() {
                self.done = true;
                return Step::Wait;
            }
            if rendered_sections > MIN_RENDERED_SECTIONS && all_rendered {
                self.in_flight = true;
                return Step::Capture;
            }
            Step::Wait
        }

        /// The capture finished (written or failed); the next attempt looks
        /// for the file again.
        pub fn finished(&mut self) {
            self.in_flight = false;
        }

        pub fn is_done(&self) -> bool {
            self.done
        }
    }

    /// The centred square Java keeps of a `width` x `height` frame:
    /// (x, y, side).
    pub fn square_crop(width: u32, height: u32) -> (u32, u32, u32) {
        if width > height { ((width - height) / 2, 0, height) } else { (0, (height - width) / 2, width) }
    }

    /// Shrinks a square RGBA image `side` pixels wide (rows `stride` bytes
    /// apart) to [`SIZE`] x [`SIZE`] by averaging the pixels each output
    /// pixel covers, with an opaque alpha.
    pub fn downscale(rgba: &[u8], side: u32, stride: usize) -> Vec<u8> {
        let n = SIZE as usize;
        let side = side as usize;
        let mut out = vec![0u8; n * n * 4];
        if side == 0 {
            return out;
        }
        for oy in 0..n {
            let (y0, y1) = (oy * side / n, ((oy + 1) * side / n).max(oy * side / n + 1).min(side));
            for ox in 0..n {
                let (x0, x1) = (ox * side / n, ((ox + 1) * side / n).max(ox * side / n + 1).min(side));
                let mut sum = [0u64; 3];
                for y in y0..y1 {
                    let row = &rgba[y * stride..];
                    for x in x0..x1 {
                        for (c, s) in sum.iter_mut().enumerate() {
                            *s += u64::from(row[x * 4 + c]);
                        }
                    }
                }
                let count = ((y1 - y0) * (x1 - x0)) as u64;
                let o = (oy * n + ox) * 4;
                for c in 0..3 {
                    out[o + c] = ((sum[c] + count / 2) / count) as u8;
                }
                out[o + 3] = 255;
            }
        }
        out
    }

    /// Why a world's icon file isn't shown; the list falls back to the
    /// default icon.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub enum Problem {
        Missing,
        /// A link, a folder, too large, unreadable or not a 64 x 64 PNG.
        Invalid(String),
    }

    /// Java's checks before reading (`WorldListEntry.validateIconFile` and
    /// `loadIcon`): a regular file, not a symbolic link; and VoxelCraft's
    /// size cap.
    pub fn check_file(path: &std::path::Path) -> Result<(), Problem> {
        let meta = match std::fs::symlink_metadata(path) {
            Ok(m) => m,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Err(Problem::Missing),
            Err(e) => return Err(Problem::Invalid(e.to_string())),
        };
        if meta.file_type().is_symlink() {
            return Err(Problem::Invalid("icon.png is a symbolic link".into()));
        }
        if !meta.is_file() {
            return Err(Problem::Invalid("icon.png isn't a file".into()));
        }
        if meta.len() > MAX_FILE_BYTES {
            return Err(Problem::Invalid(format!("icon.png is {} bytes", meta.len())));
        }
        Ok(())
    }

    /// Reads a world's icon as 64 x 64 RGBA, or says why it can't be shown.
    #[cfg(feature = "client")]
    pub fn read(path: &std::path::Path) -> Result<Vec<u8>, Problem> {
        check_file(path)?;
        let bytes = std::fs::read(path).map_err(|e| Problem::Invalid(e.to_string()))?;
        decode(&bytes)
    }

    /// Decodes PNG bytes that must be a 64 x 64 image (Java's
    /// `FaviconTexture.upload` refuses any other size) into RGBA.
    #[cfg(feature = "client")]
    pub fn decode(bytes: &[u8]) -> Result<Vec<u8>, Problem> {
        let bad = |e: &dyn std::fmt::Display| Problem::Invalid(e.to_string());
        let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
        decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
        let mut reader = decoder.read_info().map_err(|e| bad(&e))?;
        let (w, h) = (reader.info().width, reader.info().height);
        if (w, h) != (SIZE, SIZE) {
            return Err(Problem::Invalid(format!("icon is {w} x {h}, not {SIZE} x {SIZE}")));
        }
        let mut buf = vec![0; reader.output_buffer_size().ok_or_else(|| bad(&"icon too large"))?];
        let info = reader.next_frame(&mut buf).map_err(|e| bad(&e))?;
        let px = (SIZE * SIZE) as usize;
        let pixels = &buf[..info.buffer_size()];
        let out = match info.color_type {
            png::ColorType::Rgba => pixels.to_vec(),
            png::ColorType::Rgb => pixels.as_chunks::<3>().0.iter().flat_map(|p| [p[0], p[1], p[2], 255]).collect(),
            png::ColorType::GrayscaleAlpha => {
                pixels.as_chunks::<2>().0.iter().flat_map(|p| [p[0], p[0], p[0], p[1]]).collect()
            }
            png::ColorType::Grayscale => pixels.iter().flat_map(|&g| [g, g, g, 255]).collect(),
            png::ColorType::Indexed => return Err(Problem::Invalid("unexpanded palette".into())),
        };
        if out.len() != px * 4 {
            return Err(Problem::Invalid("short icon data".into()));
        }
        Ok(out)
    }

    /// Writes a 64 x 64 RGBA icon as an RGB PNG, through a temporary file
    /// renamed into place, so the list never reads half an icon.
    #[cfg(feature = "client")]
    pub fn write(path: &std::path::Path, rgba: &[u8]) -> Result<(), String> {
        let rgb: Vec<u8> = rgba.as_chunks::<4>().0.iter().flat_map(|p| [p[0], p[1], p[2]]).collect();
        let tmp = path.with_extension("png.tmp");
        let result = (|| {
            let file = std::fs::File::create(&tmp).map_err(|e| e.to_string())?;
            let mut enc = png::Encoder::new(std::io::BufWriter::new(file), SIZE, SIZE);
            enc.set_color(png::ColorType::Rgb);
            enc.set_depth(png::BitDepth::Eight);
            let mut writer = enc.write_header().map_err(|e| e.to_string())?;
            writer.write_image_data(&rgb).map_err(|e| e.to_string())?;
            writer.finish().map_err(|e| e.to_string())?;
            std::fs::rename(&tmp, path).map_err(|e| e.to_string())
        })();
        if result.is_err() {
            let _ = std::fs::remove_file(&tmp);
        }
        result
    }
}

/// Panorama Scroll Speed (`panoramaScrollSpeed`) in Accessibility Settings.
pub mod options {
    pub const PANORAMA_SPEED_KEY: &str = "panoramaScrollSpeed";
    pub const PANORAMA_SPEED_LABEL: &str = "Panorama Scroll Speed";
    /// `OptionInstance.UnitDouble`, default 1.0.
    pub const PANORAMA_SPEED_DEFAULT: f64 = 1.0;
}
