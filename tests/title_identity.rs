//! v0.18 Track 7: the title panorama, Panorama Scroll Speed and world icons
//! against Java 26.3 (`tests/fixtures/java/26.3/title.json`, extracted from
//! the pinned client by `tools/extract_java_title_fixtures.py`), and the
//! icon capture rules, crop, shrink and safe fallback.

use serde_json::Value;
use voxelcraft::title::{icon, options, panorama};

fn fixture() -> Value {
    serde_json::from_str(include_str!("fixtures/java/26.3/title.json")).unwrap()
}

fn f(v: &Value) -> f32 {
    v.as_f64().unwrap() as f32
}

#[test]
fn panorama_matches_javas_camera_and_spin() {
    let java = &fixture()["panorama"];
    assert_eq!(f(&java["fov_degrees"]), panorama::FOV_DEGREES);
    assert_eq!(f(&java["pitch_degrees"]), panorama::PITCH_DEGREES);
    assert_eq!(f(&java["spin_per_tick_degrees"]), panorama::SPIN_PER_TICK);
    assert_eq!(java["faces"], 6);
    assert_eq!(panorama::FACES.len(), 6);
    // 0.1 degrees a tick at 100%: 2 degrees a second, a turn in three minutes.
    let second = panorama::advance(0.0, 1.0, 1.0);
    assert!((second - 2.0).abs() < 1e-4, "{second}");
    assert_eq!(panorama::advance(30.0, 5.0, 0.0), 30.0, "0% holds it still");
    assert!((panorama::advance(0.0, 1.0, 0.5) - 1.0).abs() < 1e-4);
    // Java wraps the spin into [-180, 180).
    let mut spin = 0.0;
    for _ in 0..1000 {
        spin = panorama::advance(spin, 0.5, 1.0);
        assert!((-180.0..180.0).contains(&spin), "{spin}");
    }
    assert_eq!(panorama::wrap_degrees(190.0), -170.0);
    assert_eq!(panorama::wrap_degrees(-190.0), 170.0);
    assert_eq!(panorama::wrap_degrees(180.0), -180.0);
}

#[test]
fn panorama_view_starts_north_tilted_down_and_turns() {
    let (forward, up) = panorama::view(0.0);
    assert!(forward.z < -0.9 && forward.y < 0.0, "{forward}");
    assert!((forward.y - (-panorama::PITCH_DEGREES.to_radians()).sin()).abs() < 1e-5);
    assert!(up.y > 0.9 && forward.dot(up).abs() < 1e-5);
    let (east, _) = panorama::view(90.0);
    assert!(east.x > 0.9, "a quarter turn faces east: {east}");
}

#[test]
fn faces_cover_every_direction_and_round_trip() {
    for (i, face) in panorama::FACES.iter().enumerate() {
        assert!(face.forward.dot(face.up).abs() < 1e-6 && face.right().length() > 0.99, "{i}");
        let (hit, uv) = panorama::lookup(face.forward);
        assert_eq!(hit, i);
        assert!((uv[0] - 0.5).abs() < 1e-6 && (uv[1] - 0.5).abs() < 1e-6);
        // The image's top is the face's up; its right edge its right.
        let (_, top) = panorama::lookup(face.forward + face.up * 0.9);
        assert!(top[1] < 0.1);
        let (_, right) = panorama::lookup(face.forward + face.right() * 0.9);
        assert!(right[0] > 0.9);
        for (x, y) in [(0, 0), (17, 900), (1023, 1023), (512, 3)] {
            let d = panorama::direction(i, x, y, 1024);
            let (back, uv) = panorama::lookup(d);
            assert_eq!(back, i);
            assert!(
                (uv[0] * 1024.0 - (x as f32 + 0.5)).abs() < 0.01 && (uv[1] * 1024.0 - (y as f32 + 0.5)).abs() < 0.01
            );
        }
    }
    // Side faces share edges: just past north's right edge is east's left.
    let (face, uv) = panorama::lookup(glam::Vec3::new(1.0, 0.0, -0.999));
    assert_eq!(face, 1);
    assert!(uv[0] < 0.01);
}

#[test]
fn blur_grows_with_the_radius() {
    assert_eq!(panorama::blur_lod(0, 1080.0, 1024), 0.0);
    let a = panorama::blur_lod(panorama::DEFAULT_BLUR_RADIUS, 1080.0, 1024);
    let b = panorama::blur_lod(10, 1080.0, 1024);
    assert!(a > 1.0 && b > a && b < 6.0, "{a} {b}");
}

#[test]
fn panorama_speed_option_matches_java() {
    let java = &fixture()["panorama_speed_option"];
    assert_eq!(java["key"], options::PANORAMA_SPEED_KEY);
    assert_eq!(java["label"], options::PANORAMA_SPEED_LABEL);
    assert_eq!(java["default"].as_f64().unwrap(), options::PANORAMA_SPEED_DEFAULT);
    assert_eq!(java["slider"], "unit (0..1)");
    assert_eq!(java["value_label"], "percent");
    // VoxelCraft lists it after Damage Tilt: the options between them in
    // Java's order (glint, lightning flash, Monochrome Logo) aren't in the game.
    let fixture = fixture();
    let order: Vec<&str> =
        fixture["accessibility_order"].as_array().unwrap().iter().map(|v| v.as_str().unwrap()).collect();
    let tilt = order.iter().position(|&o| o == "damageTiltStrength").unwrap();
    let speed = order.iter().position(|&o| o == "panoramaSpeed").unwrap();
    assert_eq!(
        &order[tilt + 1..speed],
        ["glintSpeed", "glintStrength", "hideLightningFlash", "darkMojangStudiosBackground"]
    );
}

#[test]
fn world_icon_constants_match_java() {
    let java = &fixture()["world_icon"];
    assert_eq!(java["file"], icon::FILE);
    assert_eq!(java["size"], icon::SIZE);
    assert_eq!(java["list_size"].as_f64().unwrap() as f32, icon::LIST_SIZE);
    assert_eq!(java["hover_fill"], format!("0x{:08X}", icon::HOVER_FILL));
    assert_eq!(java["attempt_interval_ms"], icon::ATTEMPT_INTERVAL_MS);
    assert_eq!(java["min_rendered_sections_exclusive"], icon::MIN_RENDERED_SECTIONS);
    // Reset Icon is the first button under the name field.
    let column = fixture()["edit_world_column"].clone();
    assert_eq!(column[0], "selectWorld.edit.resetIcon");
    assert_eq!(fixture()["english"]["selectWorld.edit.resetIcon"], "Reset Icon");
}

#[test]
fn icons_are_taken_once_a_second_after_enough_is_drawn() {
    let mut rule = icon::Capture::default();
    // A LAN client never takes one.
    assert_eq!(rule.step(0, false, || false, 100, true), icon::Step::Wait);
    // Too little drawn: wait, and don't look again for a second.
    assert_eq!(rule.step(0, true, || false, 10, true), icon::Step::Wait, "ten sections isn't more than ten");
    assert_eq!(rule.step(500, true, || panic!("asked too soon"), 100, true), icon::Step::Wait);
    // Still meshing: wait.
    assert_eq!(rule.step(1000, true, || false, 100, false), icon::Step::Wait);
    assert_eq!(rule.step(2000, true, || false, 11, true), icon::Step::Capture);
    // One at a time while it is read back and written.
    assert_eq!(rule.step(9000, true, || false, 100, true), icon::Step::Wait);
    rule.finished();
    // Written: the next look finds the file and stops for this visit.
    assert_eq!(rule.step(10_000, true, || true, 100, true), icon::Step::Wait);
    assert!(rule.is_done());
    assert_eq!(rule.step(20_000, true, || panic!("done"), 100, true), icon::Step::Wait);
    // A world that already has an icon keeps it (only Reset Icon replaces it).
    let mut rule = icon::Capture::default();
    assert_eq!(rule.step(0, true, || true, 100, true), icon::Step::Wait);
    assert!(rule.is_done());
    // A failed write is retried a second later.
    let mut rule = icon::Capture::default();
    assert_eq!(rule.step(0, true, || false, 100, true), icon::Step::Capture);
    rule.finished();
    assert_eq!(rule.step(999, true, || false, 100, true), icon::Step::Wait);
    assert_eq!(rule.step(1000, true, || false, 100, true), icon::Step::Capture);
}

/// A frame read back for another world (left over after quitting to the
/// title) isn't this rule's; a frame that can't be read at all ends the
/// visit's capture instead of leaving it, and `--world-icon`, waiting.
#[test]
fn icons_only_accept_their_own_frame_and_give_up_when_unreadable() {
    let mut rule = icon::Capture::default();
    assert!(!rule.awaiting(), "a new world asked for nothing");
    assert_eq!(rule.step(0, true, || false, 100, true), icon::Step::Capture);
    assert!(rule.awaiting());
    rule.give_up();
    assert!(!rule.awaiting() && rule.is_done());
    assert_eq!(rule.step(5000, true, || panic!("given up"), 100, true), icon::Step::Wait);
}

/// A dimension change drops the frame a capture asked for: the capture
/// stops waiting and takes one again once the destination has drawn, rather
/// than hanging.
#[test]
fn a_dropped_icon_frame_is_taken_again_later() {
    let mut rule = icon::Capture::default();
    assert_eq!(rule.step(0, true, || false, 100, true), icon::Step::Capture);
    rule.canceled();
    assert!(!rule.awaiting() && !rule.is_done());
    // Not again within the second, nor before the destination has drawn.
    assert_eq!(rule.step(500, true, || false, 100, true), icon::Step::Wait);
    assert_eq!(rule.step(1000, true, || false, 3, false), icon::Step::Wait);
    assert_eq!(rule.step(2000, true, || false, 100, true), icon::Step::Capture);
}

#[test]
fn icons_keep_the_centred_square_shrunk_to_64() {
    assert_eq!(icon::square_crop(1920, 1080), (420, 0, 1080));
    assert_eq!(icon::square_crop(720, 1280), (0, 280, 720));
    assert_eq!(icon::square_crop(64, 64), (0, 0, 64));
    // A 128-pixel square of four coloured quadrants keeps them, averaged.
    let side = 128;
    let stride = side * 4 + 16;
    let mut px = vec![0u8; stride * side];
    for y in 0..side {
        for x in 0..side {
            let c = match (x < 64, y < 64) {
                (true, true) => [255, 0, 0, 255],
                (false, true) => [0, 255, 0, 255],
                (true, false) => [0, 0, 255, 255],
                (false, false) => [((x + y) % 2 * 255) as u8, 0, 0, 7],
            };
            px[y * stride + x * 4..y * stride + x * 4 + 4].copy_from_slice(&c);
        }
    }
    let out = icon::downscale(&px, side as u32, stride);
    assert_eq!(out.len(), 64 * 64 * 4);
    assert_eq!(&out[..4], &[255, 0, 0, 255]);
    assert_eq!(&out[63 * 4..64 * 4], &[0, 255, 0, 255]);
    assert_eq!(&out[(63 * 64) * 4..(63 * 64) * 4 + 4], &[0, 0, 255, 255]);
    // Each output pixel averages a 2 x 2 checker: half red, opaque.
    assert_eq!(&out[(63 * 64 + 63) * 4..], &[128, 0, 0, 255]);
    // Uneven sizes cover every source pixel once.
    let odd = icon::downscale(&vec![200u8; 4 * 100 * 100], 100, 400);
    assert!(odd.as_chunks::<4>().0.iter().all(|p| *p == [200, 200, 200, 255]));
}

#[test]
fn icon_files_are_checked_before_reading() {
    let dir = std::env::temp_dir().join(format!("voxelcraft-icon-check-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    assert_eq!(icon::check_file(&dir.join("icon.png")), Err(icon::Problem::Missing));
    std::fs::create_dir(dir.join("folder.png")).unwrap();
    assert!(matches!(icon::check_file(&dir.join("folder.png")), Err(icon::Problem::Invalid(_))));
    std::fs::write(dir.join("big.png"), vec![0; icon::MAX_FILE_BYTES as usize + 1]).unwrap();
    assert!(matches!(icon::check_file(&dir.join("big.png")), Err(icon::Problem::Invalid(_))));
    std::fs::write(dir.join("ok.png"), b"x").unwrap();
    assert_eq!(icon::check_file(&dir.join("ok.png")), Ok(()));
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(dir.join("ok.png"), dir.join("link.png")).unwrap();
        assert!(matches!(icon::check_file(&dir.join("link.png")), Err(icon::Problem::Invalid(_))));
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[cfg(feature = "client")]
#[test]
fn icons_written_by_the_game_read_back_and_bad_ones_fall_back() {
    let dir = std::env::temp_dir().join(format!("voxelcraft-icon-io-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(icon::FILE);
    let pixels: Vec<u8> = (0..64 * 64u32).flat_map(|i| [(i % 251) as u8, (i % 13) as u8, 99, 255]).collect();
    icon::write(&path, &pixels).unwrap();
    assert!(!dir.join("icon.png.tmp").exists(), "written through a temporary file");
    assert_eq!(icon::read(&path).unwrap(), pixels);
    // Not a PNG, a truncated one, and a PNG of the wrong size.
    std::fs::write(&path, b"GIF89a").unwrap();
    assert!(matches!(icon::read(&path), Err(icon::Problem::Invalid(_))));
    icon::write(&path, &pixels).unwrap();
    let whole = std::fs::read(&path).unwrap();
    std::fs::write(&path, &whole[..whole.len() / 2]).unwrap();
    assert!(matches!(icon::read(&path), Err(icon::Problem::Invalid(_))));
    let mut enc = png::Encoder::new(std::fs::File::create(&path).unwrap(), 65, 64);
    enc.set_color(png::ColorType::Rgba);
    enc.write_header().unwrap().write_image_data(&vec![1; 65 * 64 * 4]).unwrap();
    let Err(icon::Problem::Invalid(why)) = icon::read(&path) else { panic!() };
    assert!(why.contains("65 x 64"), "{why}");
    let _ = std::fs::remove_dir_all(&dir);
}
