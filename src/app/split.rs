//! Split-screen: extra first-person views of other players in the same
//! window, each with its own HUD. The host always keeps the top (or left)
//! view, so mouse coordinates for its menus need no offset.

use std::time::Instant;

use glam::DVec3;

use super::Game;
use super::agents::Bot;
use crate::render::ui::{Ui, UiVertex, WHITE};
use crate::render::{Frame, FrameParams, Viewport};

/// Most views in one window, like Bedrock's four-player split-screen.
pub(super) const MAX_VIEWS: usize = 4;

#[derive(Default)]
pub(super) struct Split {
    /// Agent profiles shown in the extra views, in order.
    pub follow: Vec<String>,
    /// Two views sit left/right instead of top/bottom.
    pub side_by_side: bool,
}

/// Lighting and sky shared by every view of a frame.
pub(super) struct Scene {
    pub sky: super::SkyState,
    pub rain: f32,
    pub time: f32,
    pub alpha: f64,
    pub now: Instant,
}

/// Fog for an eye: colour, start and end, and whether it's under a fluid.
pub(super) struct Fog {
    pub color: [f32; 3],
    pub start: f32,
    pub end: f32,
    pub underwater: bool,
}

impl Game {
    /// Followed profiles that are online, in view order.
    pub(super) fn followed(&self) -> impl Iterator<Item = (&String, &Bot)> {
        self.split
            .follow
            .iter()
            .filter_map(|name| self.agents.players.get_key_value(name))
            .filter(|(_, b)| b.active)
            .take(MAX_VIEWS - 1)
    }

    /// This frame's views: the host's first, then each followed player's.
    pub(super) fn viewports(&self) -> Vec<Viewport> {
        Viewport::split(self.renderer.size(), 1 + self.followed().count(), self.split.side_by_side)
    }

    /// Size of the host's view, which its HUD, menus and inventory use.
    pub(super) fn ui_size(&self) -> (u32, u32) {
        let v = self.viewports()[0];
        (v.width, v.height)
    }

    /// `/splitscreen <player>|off|side|stacked`: follow an agent in another
    /// view (again to stop), close the extra views, or pick the layout.
    pub(super) fn split_command(&mut self, arg: &str) -> Result<String, String> {
        match arg {
            "" => Ok(format!(
                "Split-screen: {} ({})",
                if self.split.follow.is_empty() { "off".into() } else { self.split.follow.join(", ") },
                if self.split.side_by_side { "side by side" } else { "stacked" }
            )),
            "off" => {
                // Controller players keep their views until they leave.
                let pads = &self.pads;
                self.split.follow.retain(|n| pads.seated(n));
                Ok("Split-screen off".into())
            }
            "side" | "stacked" => {
                self.split.side_by_side = arg == "side";
                Ok(format!("Two views now sit {}", if arg == "side" { "side by side" } else { "top and bottom" }))
            }
            name => {
                if let Some(i) = self.split.follow.iter().position(|n| n == name) {
                    self.split.follow.remove(i);
                    return Ok(format!("Stopped following {name}"));
                }
                if !self.agents.players.contains_key(name) {
                    return Err(format!("no player named {name}"));
                }
                if self.split.follow.len() >= MAX_VIEWS - 1 {
                    return Err(format!("at most {} extra views", MAX_VIEWS - 1));
                }
                self.split.follow.push(name.to_string());
                Ok(format!("Following {name} in split-screen"))
            }
        }
    }

    /// Fog seen by `player`, in the frame's sky.
    pub(super) fn fog(&self, scene: &Scene, player: &crate::player::Player) -> Fog {
        let in_lava = player.head_in_lava(&self.world);
        let underwater = in_lava || player.head_in_water(&self.world);
        let view_dist = (self.world.render_distance() * 32) as f32;
        let horizon = scene.sky.horizon;
        let (color, start, end) = if in_lava {
            (super::LAVA_FOG, 0.0, 2.0)
        } else if underwater {
            (super::WATER_FOG.map(|c| c * scene.sky.daylight), 0.0, 28.0)
        } else if self.dimension == crate::world::terrain::Dimension::End {
            (horizon, view_dist * 0.45, view_dist * 0.95)
        } else if !self.dimension.has_sky() {
            (horizon, 8.0, view_dist.min(160.0) * 0.8)
        } else {
            (horizon, view_dist * 0.55, view_dist * 0.95)
        };
        Fog { color, start, end, underwater }
    }

    /// Swing, bob and item changes for every online player's hand.
    pub(super) fn animate_hands(&mut self, dt: f32, alpha: f64, paused: bool) {
        for bot in self.agents.players.values_mut().filter(|b| b.active) {
            let a = &bot.agent;
            let feet = a.previous_pos.lerp(a.player.pos, alpha);
            let distance = (feet - bot.drawn_feet).with_y(0.0).length();
            bot.drawn_feet = feet;
            let walked = if paused || a.player.flying || distance > 4.0 { 0.0 } else { distance as f32 };
            if a.swings != bot.seen_swings {
                bot.seen_swings = a.swings;
                bot.hand.swing();
            }
            bot.hand.update(dt, a.inventory.get(a.selected).map(|s| s.item), walked, a.player.on_ground);
        }
    }

    /// Draws every followed player's view after the host's.
    pub(super) fn draw_followers(&mut self, frame: &mut Frame, scene: &Scene, viewports: &[Viewport]) {
        let names: Vec<String> = self.followed().map(|(n, _)| n.clone()).collect();
        let host_feet = self.rendered_eye - (self.player.eye() - self.player.pos);
        for (name, &vp) in names.iter().zip(&viewports[1..]) {
            let bot = &self.agents.players[name];
            let a = &bot.agent;
            let feet = a.previous_pos.lerp(a.player.pos, scene.alpha);
            let camera = feet + (a.player.eye() - a.player.pos);
            let forward = a.player.forward();
            let fog = self.fog(scene, &a.player);
            let models = self.block_models(scene.alpha);
            let highlight = a.target(&self.world).map(|(p, _)| {
                let (min, max) = self.world.outline(p);
                (p, min, max)
            });
            let crack = a
                .breaking(&self.world)
                .filter(|&(_, f)| f > 0.0)
                .map(|(p, f)| (p, crate::world::block::tex::CRACK_0 + (f * 10.0).min(9.0) as u8));
            let ui = if self.show_hud || a.vitals.is_dead() {
                self.follower_ui(name, bot, (vp.width, vp.height), scene.now)
            } else {
                Vec::new()
            };
            // Everyone else is visible from this view, the host included.
            let mut others: Vec<(&crate::player::Player, DVec3)> = Vec::new();
            if !self.vitals.is_dead() {
                others.push((&self.player, host_feet));
            }
            for (other, b) in &self.agents.players {
                if other != name && b.active && !b.agent.vitals.is_dead() {
                    others.push((&b.agent.player, b.agent.previous_pos.lerp(b.agent.player.pos, scene.alpha)));
                }
            }
            let verts = self.mobs.entities.mesh(camera, forward, fog.end, scene.time, scene.alpha);
            super::push_avatars(&self.world, &others, camera, fog.end, scene.time, verts);
            self.renderer.set_entities(verts);
            super::weather::sheets(&self.world, camera, scene.rain, &mut self.weather_verts);
            self.renderer.set_weather(&self.weather_verts);
            let params = FrameParams {
                camera,
                forward,
                fov_y: self.settings.fov.to_radians(),
                sky_color: fog.color.map(|c| c as f64),
                fog_color: fog.color,
                fog_start: fog.start,
                fog_end: fog.end,
                daylight: scene.sky.daylight,
                zenith_color: if fog.underwater { fog.color } else { scene.sky.zenith },
                sun_dir: scene.sky.sun_dir,
                dimension: self.dimension,
                enhanced_graphics: self.settings.enhanced_graphics,
                time: scene.time,
                highlight,
                crack,
                block_models: models,
                hand: (self.show_hud && !a.vitals.is_dead()).then(|| {
                    bot.hand.view(a.eating(), crate::entity::sky_light(&self.world, camera), self.torch_light(camera))
                }),
                rain: scene.rain,
                ui,
            };
            self.renderer.draw_view(frame, &params, vp);
        }
    }

    /// A followed player's HUD: name, crosshair, hotbar, health and hunger,
    /// and their death message.
    fn follower_ui(&self, name: &str, bot: &Bot, (w, h): (u32, u32), now: Instant) -> Vec<UiVertex> {
        let a = &bot.agent;
        let mut ui = Ui::new(w as f32, h as f32, self.renderer.scale_factor());
        let (sw, sh) = ui.size();
        let underwater = a.player.head_in_water(&self.world);
        if underwater {
            ui.rect(0.0, 0.0, sw, sh, [0.05, 0.15, 0.4, 0.3]);
        }
        let hurt = a.vitals.since_damage();
        if hurt < super::hud::HURT_FLASH {
            ui.rect(0.0, 0.0, sw, sh, [0.8, 0.0, 0.0, 0.35 * (1.0 - hurt / super::hud::HURT_FLASH)]);
        }
        if a.vitals.is_dead() {
            ui.rect(0.0, 0.0, sw, sh, [0.5, 0.0, 0.0, 0.45]);
            let msg = format!("{name} {}", a.vitals.death.as_deref().unwrap_or("died"));
            ui.text(((sw - Ui::text_width(&msg)) / 2.0).floor(), (sh / 2.0 - 10.0).floor(), &msg, WHITE);
            let hint = if self.pads.seated(name) { "Press A to respawn" } else { "Waiting to respawn" };
            ui.text(((sw - Ui::text_width(hint)) / 2.0).floor(), (sh / 2.0 + 4.0).floor(), hint, WHITE);
        } else {
            let (cx, cy) = ((sw / 2.0).floor(), (sh / 2.0).floor());
            ui.rect(cx - 5.0, cy - 0.5, 10.0, 1.0, [1.0, 1.0, 1.0, 0.85]);
            ui.rect(cx - 0.5, cy - 5.0, 1.0, 10.0, [1.0, 1.0, 1.0, 0.85]);
            if a.eating() > 0.0 {
                super::hud::eating_bar(&mut ui, cx, cy, a.eating());
            }
        }
        let hud = super::hud::HudPlayer {
            inventory: &a.inventory,
            vitals: &a.vitals,
            selected: a.selected,
            survival: !a.creative,
            underwater,
        };
        // Screens cover the hotbar, as Minecraft's do.
        match self.pad_menu(name) {
            Some(menu) => self.pad_menu_ui(&mut ui, name, menu),
            None => {
                self.bar_ui(&mut ui, &hud, now);
            }
        }
        let label = if self.pads.seated(name) { name.to_string() } else { format!("{name} (agent)") };
        ui.rect(2.0, 2.0, Ui::text_width(&label) + 6.0, 12.0, [0.0, 0.0, 0.0, 0.5]);
        ui.text(5.0, 4.0, &label, WHITE);
        // A thin border separates the views.
        ui.rect(0.0, 0.0, sw, 1.0, [0.0, 0.0, 0.0, 0.8]);
        ui.rect(0.0, 0.0, 1.0, sh, [0.0, 0.0, 0.0, 0.8]);
        ui.verts
    }
}
