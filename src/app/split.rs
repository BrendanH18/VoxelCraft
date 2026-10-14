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

/// The blended biome fog around `camera`, from the sampler that belongs to `view`.
fn view_haze(
    samplers: &mut Vec<crate::world::nether_biome::FogSampler>,
    view: usize,
    camera: DVec3,
    biome: impl Fn(i32, i32) -> crate::world::nether_biome::NetherBiome,
) -> [f32; 3] {
    if samplers.len() <= view {
        samplers.resize_with(view + 1, Default::default);
    }
    samplers[view].sample(camera, biome)
}

/// Fog for an eye: colour, sky overhead, start and end.
pub(super) struct Fog {
    pub color: [f32; 3],
    /// Sky colour overhead, which follows the fog under a fluid.
    pub zenith: [f32; 3],
    pub start: f32,
    pub end: f32,
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

    /// Physical pixels per UI pixel for the host's HUD and screens, smaller
    /// when split-screen shrinks its view.
    pub(super) fn ui_scale(&self) -> f32 {
        let (w, h) = self.ui_size();
        Ui::fitted_scale(w as f32, h as f32, self.renderer.scale_factor())
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

    /// The Nether's haze at `camera` for view `view` (0 is the host's): Java's `FogRenderer`
    /// blends the biome fog around each eye, so every split-screen view has its own sampler.
    pub(super) fn nether_haze(&self, view: usize, camera: DVec3) -> [f32; 3] {
        let generator = &self.world.generator;
        if generator.dimension != crate::world::terrain::Dimension::Nether {
            return super::dimension::NETHER_FOG;
        }
        view_haze(&mut self.nether_fog.borrow_mut(), view, camera, |qx, qz| {
            generator.nether_biome(qx * 4, qz * 4).unwrap_or(crate::world::nether_biome::NetherBiome::NetherWastes)
        })
    }

    /// Horizon and zenith colours seen from `camera` in view `view`.
    fn view_sky(&self, scene: &Scene, view: usize, camera: DVec3) -> ([f32; 3], [f32; 3]) {
        if self.dimension == crate::world::terrain::Dimension::Nether {
            let haze = self.nether_haze(view, camera);
            (haze, haze)
        } else {
            (scene.sky.horizon, scene.sky.zenith)
        }
    }

    /// Fog seen from `camera` in view `view`, in the frame's sky.
    pub(super) fn fog(&self, scene: &Scene, view: usize, camera: DVec3) -> Fog {
        let cell = camera.floor().as_ivec3();
        let fluid = self.world.get_block(cell).filter(|b| b.is_water() || b.is_lava()).filter(|b| {
            let above = self.world.get_block(cell + glam::IVec3::Y);
            let covered = above.is_some_and(|a| if b.is_water() { a.is_water() } else { a.is_lava() });
            camera.y - camera.y.floor() < 1.0 - if covered { 0.0 } else { b.fluid_drop() as f64 / 16.0 }
        });
        let in_lava = fluid.is_some_and(|b| b.is_lava());
        let underwater = fluid.is_some();
        let view_dist = (self.world.render_distance() * 32) as f32;
        let (horizon, zenith) = self.view_sky(scene, view, camera);
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
        Fog { color, zenith: if underwater { color } else { zenith }, start, end }
    }

    /// Plays what agents did since last frame: breaking, placing, chewing.
    pub(super) fn agent_sounds(&mut self) {
        use voxelcraft::agent::Event;
        for bot in self.agents.players.values_mut() {
            for event in bot.agent.events.drain(..) {
                match event {
                    Event::Broke(pos, block) => self.audio.block_break(block, pos),
                    Event::Placed(pos, block) => self.audio.block_place(block, pos),
                    Event::Bucket(effect) => super::bucket::feedback(&mut self.audio, effect),
                    Event::Chew => {
                        let sound = crate::audio::sounds::Sound::Step(crate::audio::sounds::Material::Snow);
                        self.audio.play(sound, Some(bot.agent.session.player.eye()), 0.7, (1.4, 1.7));
                    }
                    Event::Xp(chime) => {
                        super::items::xp_sounds(&mut self.audio, Some(bot.agent.session.player.eye()), chime)
                    }
                    Event::EyeThrown(from) => super::items::eye_thrown_sound(&mut self.audio, from),
                    Event::FrameFilled(pos, opened) => super::items::frame_filled_sounds(&mut self.audio, pos, opened),
                }
            }
        }
    }

    /// Swing, bob and item changes for every online player's hand, and their footsteps.
    pub(super) fn animate_hands(&mut self, dt: f32, alpha: f64, paused: bool) {
        for bot in self.agents.players.values_mut().filter(|b| b.active) {
            let a = &bot.agent;
            let feet = a.previous_pos.lerp(a.player.pos, alpha);
            let distance = (feet - bot.drawn_feet).with_y(0.0).length();
            bot.drawn_feet = feet;
            let walked = if paused || a.player.flying || distance > 4.0 { 0.0 } else { distance as f32 };
            self.audio.other_steps(&a.player, &self.world, walked as f64, &mut bot.stride);
            if a.swings != bot.seen_swings {
                bot.seen_swings = a.swings;
                bot.hand.swing();
            }
            bot.hand.update(dt, a.inventory.get(a.selected).map(|s| s.item));
        }
    }

    /// Draws every followed player's view after the host's.
    pub(super) fn draw_followers(&mut self, frame: &mut Frame, scene: &Scene, viewports: &[Viewport]) {
        let names: Vec<String> = self.followed().map(|(n, _)| n.clone()).collect();
        let host_feet = self.rendered_eye - (self.session.player.eye() - self.session.player.pos);
        let host_dial = self.dial_of(&self.session.player);
        for (view, (name, &vp)) in names.iter().zip(&viewports[1..]).enumerate() {
            let bot = &self.agents.players[name];
            let a = &bot.agent;
            let feet = a.previous_pos.lerp(a.player.pos, scene.alpha);
            let eye = feet + (a.player.eye() - a.player.pos);
            let (camera, forward) = bot.camera.view(&self.world, eye, a.player.forward());
            let fog = self.fog(scene, view + 1, camera);
            let models = self.block_models(scene.alpha);
            let highlight = a.target(&self.world).map(|(p, _)| {
                let (min, max) = self.world.outline(p);
                (p, min, max)
            });
            let pad = self.pad_view(name).map(|p| (p.breaking, p.eating));
            let cracks = self.cracks_seen_from(eye);
            let ui = if self.show_hud || a.vitals.is_dead() {
                self.follower_ui(name, bot, (vp.width, vp.height), scene.now)
            } else {
                Vec::new()
            };
            // Everyone else is visible from this view, the host included.
            let mut others: Vec<(&crate::player::Player, DVec3, crate::entity::model::PlayerAppearance, &str)> =
                Vec::new();
            if !self.session.vitals.is_dead() || self.session.vitals.since_damage() < 1.0 {
                others.push((
                    &self.session.player,
                    host_feet,
                    Self::stamp_look(
                        host_dial,
                        self.hand.appearance(
                            &self.session.vitals,
                            (self.body.actions.eat_timer / super::EAT_TIME) as f32,
                            scene.alpha,
                            self.session.inventory.armor,
                        ),
                    ),
                    self.lan.profile.as_str(),
                ));
            }
            for (other, b) in &self.agents.players {
                if other != name
                    && b.active
                    && (!b.agent.session.vitals.is_dead() || b.agent.session.vitals.since_damage() < 1.0)
                {
                    others.push((
                        &b.agent.session.player,
                        b.agent.previous_pos.lerp(b.agent.session.player.pos, scene.alpha),
                        Self::stamp_look(
                            host_dial.at(
                                b.agent.session.player.yaw,
                                b.agent.session.player.pos.x,
                                b.agent.session.player.pos.z,
                            ),
                            b.hand.appearance(
                                &b.agent.session.vitals,
                                b.agent.eating(),
                                scene.alpha,
                                b.agent.session.inventory.armor,
                            ),
                        ),
                        other.as_str(),
                    ));
                }
            }
            let verts = self.mobs.entities.mesh(camera, forward, fog.end, scene.time, scene.alpha);
            super::push_avatars(&self.world, &others, camera, fog.end, scene.time, verts);
            if !bot.camera.first_person() {
                crate::entity::model::build_player(
                    &a.player,
                    feet,
                    camera,
                    (
                        crate::entity::sky_light(&self.world, eye),
                        self.world.block_light(eye.floor().as_ivec3()) as f32 / 15.0,
                    ),
                    scene.time,
                    Self::stamp_look(
                        host_dial.at(a.player.yaw, a.player.pos.x, a.player.pos.z),
                        bot.hand.appearance(&a.vitals, pad.map_or(a.eating(), |p| p.1), scene.alpha, a.inventory.armor),
                    ),
                    verts,
                );
            }
            self.renderer.set_entities(verts);
            super::weather::sheets(&self.world, camera, scene.rain, &mut self.weather_verts);
            self.renderer.set_weather(&self.weather_verts);
            let params = FrameParams {
                camera,
                forward,
                view_effect: voxelcraft::camera::view_effect(
                    &a.player,
                    &a.vitals,
                    scene.alpha as f32,
                    self.settings.view_bobbing,
                ),
                fov_y: self.settings.fov.to_radians(),
                sky_color: fog.color.map(|c| c as f64),
                fog_color: fog.color,
                fog_start: fog.start,
                fog_end: fog.end,
                daylight: scene.sky.daylight,
                zenith_color: fog.zenith,
                sun_dir: scene.sky.sun_dir,
                dimension: self.dimension,
                enhanced_graphics: self.settings.enhanced_graphics,
                time: scene.time,
                highlight,
                cracks,
                block_models: models,
                hand: (bot.camera.first_person() && self.show_hud && !a.vitals.is_dead()).then(|| {
                    let eating = pad.map_or(a.eating(), |p| p.1);
                    Self::stamp_hand(
                        host_dial.at(a.player.yaw, a.player.pos.x, a.player.pos.z),
                        bot.hand.view(eating, crate::entity::sky_light(&self.world, camera), self.torch_light(camera)),
                    )
                }),
                rain: scene.rain,
                night_vision: a.vitals.effects.night_vision(scene.time),
                ui,
            };
            self.renderer.set_particles(&self.particles.pool, &self.world, camera, forward, scene.alpha);
            self.renderer.draw_view(frame, &params, vp);
        }
    }

    /// A followed player's HUD: name, crosshair, hotbar, health and hunger,
    /// and their death message.
    fn follower_ui(&self, name: &str, bot: &Bot, (w, h): (u32, u32), now: Instant) -> Vec<UiVertex> {
        let a = &bot.agent;
        let mut ui =
            Ui::with_scale(w as f32, h as f32, Ui::fitted_scale(w as f32, h as f32, self.renderer.scale_factor()));
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
            let pad = self.pad_view(name).unwrap_or_default();
            let eating = if self.pads.seated(name) { pad.eating } else { a.eating() };
            if eating > 0.0 {
                super::hud::eating_bar(&mut ui, cx, cy, eating);
            }
            if let Some(power) = pad.bow {
                super::hud::bow_bar(&mut ui, cx, cy, power);
            }
        }
        let hud = super::hud::HudPlayer {
            inventory: &a.inventory,
            vitals: &a.vitals,
            selected: a.selected,
            survival: !a.creative,
            underwater,
            dial: self.dial_of(&a.player),
            vehicle: a.player.vehicle,
        };
        // Screens cover the hotbar, as Minecraft's do.
        let pad = self.pad_view(name).unwrap_or_default();
        match self.pad_menu(name) {
            Some(menu) => self.pad_menu_ui(&mut ui, name, menu),
            None => {
                let top = self.bar_ui(&mut ui, &hud, now);
                if let Some(msg) = pad.message {
                    let x = ((sw - Ui::text_width(msg)) / 2.0).floor();
                    ui.text(x, (top - 12.0).floor(), msg, WHITE);
                }
            }
        }
        if let Some(t) = a.sleeping {
            // Falling asleep fades the view out, then waits for everyone.
            let k = (t / super::bed::SLEEP_TIME).min(1.0);
            ui.rect(0.0, 0.0, sw, sh, [0.0, 0.0, 0.02, 0.97 * k]);
            let (asleep, players) = self.sleep_count();
            if k >= 1.0 && asleep < players {
                let msg = format!("{asleep}/{players} players sleeping");
                ui.text(((sw - Ui::text_width(&msg)) / 2.0).floor(), (sh / 2.0 - 10.0).floor(), &msg, WHITE);
                let hint = if self.pads.seated(name) { "Press A to leave the bed" } else { "Waiting for others" };
                ui.text(((sw - Ui::text_width(hint)) / 2.0).floor(), (sh / 2.0 + 4.0).floor(), hint, WHITE);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::nether_biome::NetherBiome;

    #[test]
    fn each_split_screen_view_blends_the_fog_around_its_own_eye() {
        // Soul sand valley to the west of x = 0 and crimson forest to the east.
        let biome = |qx: i32, _qz: i32| if qx < 0 { NetherBiome::SoulSandValley } else { NetherBiome::CrimsonForest };
        let mut samplers = Vec::new();
        let host = view_haze(&mut samplers, 0, DVec3::new(-200.0, 70.0, 0.0), biome);
        let follower = view_haze(&mut samplers, 1, DVec3::new(200.0, 70.0, 0.0), biome);
        assert_eq!(host, NetherBiome::SoulSandValley.fog_color());
        assert_eq!(follower, NetherBiome::CrimsonForest.fog_color());
        // The host's reading is unchanged by the follower sampling elsewhere in the same frame.
        assert_eq!(view_haze(&mut samplers, 0, DVec3::new(-200.0, 70.0, 0.0), biome), host);
    }
}
