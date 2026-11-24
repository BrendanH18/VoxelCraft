//! Video Settings: Java 26.3's three sections (Display, Quality &
//! Performance, Preferences) with the options VoxelCraft can honour, in
//! Java's order (Quality & Performance: `tests/fixtures/java/26.3/mipmap_levels.json`). Options that would need renderer work VoxelCraft doesn't
//! have (Biome Blend, Entity Distance and the like) are left out. v0.18
//! Track 8 adds Clouds and Cloud Distance.

use super::{OptionsHost, change, close, frac, lerp};
use crate::app::gui::{OptionList, Rect, Screen, Size, Step, Widget, on_off};
use crate::app::settings::{MAX_FPS, MIPMAP_LEVELS, RENDER_DISTANCE};
use crate::render::ui::Ui;
use crate::world::activity::SIMULATION_DISTANCE_RANGE;
use voxelcraft::atmosphere::CLOUD_RANGE;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(in crate::app) enum Id {
    DisplayHeader,
    MaxFps,
    Vsync,
    GuiScale,
    Fullscreen,
    Brightness,
    QualityHeader,
    Graphics,
    RenderDistance,
    SimulationDistance,
    Clouds,
    Particles,
    MipmapLevels,
    MenuBlur,
    CloudDistance,
    PreferencesHeader,
    Fps,
    Done,
}

fn range((lo, hi): (i32, i32)) -> (f32, f32) {
    (lo as f32, hi as f32)
}

fn range_u((lo, hi): (u32, u32)) -> (f32, f32) {
    (lo as f32, hi as f32)
}

/// Cloud Distance's slider range in Java chunks.
fn cloud_range() -> (f32, f32) {
    (CLOUD_RANGE.0 as f32, CLOUD_RANGE.1 as f32)
}

/// Java's Max Framerate label: "Unlimited" at the top.
pub(in crate::app) fn max_fps_label(fps: u32) -> String {
    if fps >= MAX_FPS.1 { "Max Framerate: Unlimited".into() } else { format!("Max Framerate: {fps} fps") }
}

/// Java's Brightness label: Moody, Default and Bright at 0, 50 and 100.
pub(in crate::app) fn brightness_label(v: f32) -> String {
    // Rounded: f32 makes 0.53 * 100 come out as 52.999996.
    match (v * 100.0).round() as i32 {
        0 => "Brightness: Moody".into(),
        50 => "Brightness: Default".into(),
        100 => "Brightness: Bright".into(),
        v => format!("Brightness: {v}"),
    }
}

/// Java's Mipmap Levels label: OFF at 0, otherwise the number.
pub(in crate::app) fn mipmap_label(levels: u32) -> String {
    if levels == 0 { "Mipmap Levels: OFF".into() } else { format!("Mipmap Levels: {levels}") }
}

/// GUI Scale's top value for this window (Auto picks it).
pub(in crate::app) fn max_gui_scale<H: OptionsHost>(host: &H) -> u32 {
    host.window_size().map_or(4, |(w, h)| Ui::max_gui_scale(w as f32, h as f32))
}

/// Java's GUI Scale cycle: Auto, 1, 2 ... the largest that fits, Auto.
pub(in crate::app) fn next_gui_scale(scale: u32, max: u32) -> u32 {
    if scale >= max { 0 } else { scale + 1 }
}

pub(in crate::app) struct VideoSettings;

impl<H: OptionsHost> Screen<H> for VideoSettings {
    type Id = Id;

    fn title(&self, _: &H) -> String {
        "Video Settings".into()
    }

    fn list(&self, _: &H, size: Size) -> Option<Rect> {
        Some(OptionList::<Id>::rect(size))
    }

    fn widgets(&self, host: &H, size: Size) -> Vec<Widget<Id>> {
        let s = host.settings();
        let z = [0.0; 4];
        let (rd, sd) = (range(RENDER_DISTANCE), range(SIMULATION_DISTANCE_RANGE));
        let fps_steps = (MAX_FPS.1 - MAX_FPS.0) / 10;
        let gui_scale = match s.gui_scale.min(max_gui_scale(host)) {
            0 => "GUI Scale: Auto".to_string(),
            n => format!("GUI Scale: {n}"),
        };
        let mut list = OptionList::new(size);
        list.wide(Widget::label(Id::DisplayHeader, z, "Display"));
        list.pairs(vec![
            Widget::slider(
                Id::MaxFps,
                z,
                max_fps_label(s.max_fps),
                (s.max_fps - MAX_FPS.0) as f32 / (MAX_FPS.1 - MAX_FPS.0) as f32,
                fps_steps,
            ),
            Widget::button(Id::Vsync, z, format!("VSync: {}", on_off(s.vsync))),
            Widget::button(Id::GuiScale, z, gui_scale),
            Widget::button(Id::Fullscreen, z, format!("Fullscreen: {}", s.fullscreen.name())),
            Widget::slider(Id::Brightness, z, brightness_label(s.brightness), s.brightness, 100),
        ]);
        list.wide(Widget::label(Id::QualityHeader, z, "Quality & Performance"));
        list.wide(Widget::button(
            Id::Graphics,
            z,
            format!("Graphics: {}", if s.enhanced_graphics { "Enhanced" } else { "Classic" }),
        ));
        list.pairs(vec![
            // Stored in 32-block chunks; labelled in Java's 16-block chunks
            // so a distance reads as it does in Java.
            Widget::slider(
                Id::RenderDistance,
                z,
                format!("Render Distance: {} Chunks", s.render_distance * 2),
                frac(s.render_distance as f32, rd),
                (rd.1 - rd.0) as u32,
            ),
            Widget::slider(
                Id::SimulationDistance,
                z,
                format!("Simulation Distance: {} Chunks", s.simulation_distance),
                frac(s.simulation_distance as f32, sd),
                (sd.1 - sd.0) as u32,
            ),
            // Java's Quality order: Clouds before Particles.
            Widget::button(Id::Clouds, z, format!("Clouds: {}", s.clouds.caption())),
            Widget::button(Id::Particles, z, format!("Particles: {}", s.particles.name())),
            Widget::slider(
                Id::MipmapLevels,
                z,
                mipmap_label(s.mipmap_levels),
                frac(s.mipmap_levels as f32, range_u(MIPMAP_LEVELS)),
                MIPMAP_LEVELS.1 - MIPMAP_LEVELS.0,
            ),
            // v0.18 Track 2: Java lists it after Entity Distance.
            super::menu_blur_slider(Id::MenuBlur, s.menu_blur),
            // Java's Quality order: Clouds before Particles; Cloud Distance
            // after Menu Background Blur.
            Widget::slider(
                Id::CloudDistance,
                z,
                format!("Cloud Distance: {} Chunks", s.cloud_range),
                frac(s.cloud_range as f32, cloud_range()),
                CLOUD_RANGE.1 - CLOUD_RANGE.0,
            ),
        ]);
        list.wide(Widget::label(Id::PreferencesHeader, z, "Preferences"));
        list.pair(
            Widget::button(Id::Fps, z, format!("FPS Counter: {}", on_off(s.show_fps)))
                .tooltip("A compact frame rate at the top left. F3 shows the full debug screen."),
            None,
        );
        list.done(Widget::button(Id::Done, z, "Done"), size)
    }

    fn activate(&mut self, host: &mut H, id: Id) -> Step<H> {
        match id {
            Id::Graphics => change(host, |s| s.enhanced_graphics = !s.enhanced_graphics),
            Id::Vsync => change(host, |s| s.vsync = !s.vsync),
            Id::GuiScale => {
                let max = max_gui_scale(host);
                change(host, |s| s.gui_scale = next_gui_scale(s.gui_scale.min(max), max));
            }
            Id::Fullscreen => {
                change(host, |s| s.fullscreen = s.fullscreen.next());
                host.apply_fullscreen();
            }
            Id::Particles => change(host, |s| s.particles = s.particles.next()),
            Id::Clouds => change(host, |s| s.clouds = s.clouds.next()),
            Id::Fps => change(host, |s| s.show_fps = !s.show_fps),
            Id::Done => return close(host),
            _ => {}
        }
        Step::Stay
    }

    fn slide(&mut self, host: &mut H, id: Id, t: f32) {
        match id {
            Id::RenderDistance => change(host, |s| s.render_distance = lerp(range(RENDER_DISTANCE), t).round() as i32),
            Id::SimulationDistance => {
                change(host, |s| s.simulation_distance = lerp(range(SIMULATION_DISTANCE_RANGE), t).round() as i32)
            }
            Id::MaxFps => {
                let steps = ((MAX_FPS.1 - MAX_FPS.0) / 10) as f32;
                change(host, |s| s.max_fps = MAX_FPS.0 + (t * steps).round() as u32 * 10);
            }
            Id::Brightness => change(host, |s| s.brightness = (t * 100.0).round() / 100.0),
            Id::MipmapLevels => change(host, |s| s.mipmap_levels = lerp(range_u(MIPMAP_LEVELS), t).round() as u32),
            Id::MenuBlur => change(host, |s| s.menu_blur = super::menu_blur_at(t)),
            Id::CloudDistance => change(host, |s| s.cloud_range = lerp(cloud_range(), t).round() as u32),
            _ => {}
        }
    }

    fn back(&mut self, host: &mut H) -> Step<H> {
        close(host)
    }
}
