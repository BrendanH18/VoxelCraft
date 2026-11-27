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
    /// Game ticks since the last frame (0 while paused), for the fog's smoothing.
    pub ticks: f32,
}

/// One eye's atmosphere state between frames: its biome blend, smoothed
/// rain fog and water vision (`LocalPlayer.waterVisionTime`, in ticks).
#[derive(Default)]
pub(super) struct ViewAtmosphere {
    sampler: voxelcraft::atmosphere::Sampler,
    rain: voxelcraft::atmosphere::RainFog,
    water_ticks: f32,
}

/// How far terrain is drawn around `camera` in every direction, in blocks:
/// Java's render distance in blocks, or less where VoxelCraft's 32-block
/// chunks stop short of it. Chunks are meshed while their horizontal chunk
/// distance from the camera's chunk is at most `rd`, so the nearest chunk
/// that isn't bounds the distance. Java's render-distance fog ends there,
/// so the fog closes before the terrain's ragged edge instead of past it.
pub(super) fn meshed_radius(rd: i32, camera: DVec3) -> f32 {
    let size = crate::world::chunk::CHUNK_SIZE_I;
    let (cx, cz) = ((camera.x / f64::from(size)).floor() as i32, (camera.z / f64::from(size)).floor() as i32);
    let mut nearest = f64::from(rd * size);
    for a in -rd - 1..=rd + 1 {
        for b in -rd - 1..=rd + 1 {
            if a * a + b * b <= rd * rd {
                continue;
            }
            // Distance from the camera to that chunk's column.
            let (x0, z0) = (f64::from((cx + a) * size), f64::from((cz + b) * size));
            let dx = (x0 - camera.x).max(camera.x - (x0 + f64::from(size))).max(0.0);
            let dz = (z0 - camera.z).max(camera.z - (z0 + f64::from(size))).max(0.0);
            nearest = nearest.min(dx.hypot(dz));
        }
    }
    nearest as f32
}

/// One eye's fog, sky and clouds for this frame (sRGB colours).
#[derive(Clone, Copy)]
pub(super) struct Atmosphere {
    pub fog: voxelcraft::atmosphere::Fog,
    /// `visual/sky_light_factor` and `visual/sky_light_color`.
    pub sky_light: (f32, [f32; 3]),
    pub cloud_color: [f32; 4],
}

impl Atmosphere {
    /// The renderer's linear fog colour.
    pub fn fog_color(&self) -> [f32; 3] {
        voxelcraft::atmosphere::linear(self.fog.color)
    }

    /// How far entities and beams need building: nothing past this shows.
    pub fn end(&self) -> f32 {
        self.fog.end()
    }

    /// v1.0 Track 2: Java's entity, beam and conduit draw distances for a
    /// view of `player`'s world drawn `render_distance` storage chunks
    /// around it, within this fog.
    pub fn draw_distance(
        &self,
        player: DVec3,
        render_distance: i32,
    ) -> voxelcraft::entity::draw_distance::DrawDistance {
        voxelcraft::entity::draw_distance::DrawDistance::new(
            player,
            render_distance,
            self.fog.environmental_end,
            self.fog.render_distance_end,
        )
    }

    /// Fills the fog, sky and cloud fields of a frame.
    pub fn apply(&self, p: &mut FrameParams) {
        use voxelcraft::atmosphere::linear;
        p.fog_color = self.fog_color();
        p.sky_color = p.fog_color.map(f64::from);
        p.zenith_color = linear(self.fog.sky);
        p.fog_start = self.fog.environmental_start;
        p.fog_end = self.fog.environmental_end;
        p.fog_render = [self.fog.render_distance_start, self.fog.render_distance_end];
        p.sky_end = self.fog.sky_end;
        p.cloud_end = self.fog.cloud_end;
        let [r, g, b] = linear([self.fog.sunrise[0], self.fog.sunrise[1], self.fog.sunrise[2]]);
        p.sunrise = [r, g, b, self.fog.sunrise[3]];
        p.sky_light_color = self.sky_light.1;
        p.cloud_color = self.cloud_color;
    }
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

    /// Physical pixels per UI pixel for the host's HUD and screens: the GUI
    /// Scale option, smaller when split-screen shrinks its view.
    pub(super) fn ui_scale(&self) -> f32 {
        let (w, h) = self.ui_size();
        self.view_scale((w, h))
    }

    /// The GUI scale for a view `size` physical pixels big.
    fn view_scale(&self, (w, h): (u32, u32)) -> f32 {
        let (ww, wh) = self.renderer.size();
        Ui::view_scale((w as f32, h as f32), (ww as f32, wh as f32), self.settings.gui_scale)
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

    /// Java 26.3's fog, sky and clouds seen from `camera` in view `view`
    /// (0 is the host's) by a player with `vitals`, looking along `forward`.
    /// Each view keeps its own biome blend, rain fog and water vision, as
    /// each of Java's eyes would.
    pub(super) fn atmosphere(
        &self,
        scene: &Scene,
        view: usize,
        camera: DVec3,
        forward: glam::Vec3,
        vitals: &voxelcraft::simulation::survival::Vitals,
        spectator: bool,
    ) -> Atmosphere {
        use crate::world::terrain::Dimension;
        use voxelcraft::atmosphere::{self as atm, Medium};
        use voxelcraft::simulation::effects::Effect;
        let cell = camera.floor().as_ivec3();
        let fluid = self.world.get_block(cell).filter(|b| b.is_water() || b.is_lava()).filter(|b| {
            let above = self.world.get_block(cell + glam::IVec3::Y);
            let covered = above.is_some_and(|a| if b.is_water() { a.is_water() } else { a.is_lava() });
            camera.y - camera.y.floor() < 1.0 - if covered { 0.0 } else { b.fluid_drop() as f64 / 16.0 }
        });
        let medium = if self.world.get_block(cell) == Some(crate::world::scaffolding::POWDER_SNOW) {
            Medium::PowderSnow
        } else {
            match fluid {
                Some(b) if b.is_lava() => Medium::Lava,
                Some(_) => Medium::Water,
                None => Medium::Air,
            }
        };
        let dimension = self.dimension;
        let base = atm::dimension(dimension);
        let generator = &self.world.generator;
        let world = &self.world;
        let visuals = |p: glam::IVec3| match dimension {
            Dimension::Overworld => atm::overworld_biome(world.biome_at(p)),
            Dimension::Nether => atm::nether_biome(
                generator.nether_biome(p.x, p.z).unwrap_or(crate::world::nether_biome::NetherBiome::NetherWastes),
            ),
            Dimension::End => atm::biome_visuals("the_end").expect("the End biome"),
        };
        let mut views = self.atmosphere_views.borrow_mut();
        if views.len() <= view {
            views.resize_with(view + 1, Default::default);
        }
        let state = &mut views[view];
        let local = state.sampler.sample(camera.x, camera.z, |qx, qz| {
            atm::Local::resolve(base, visuals(glam::IVec3::new(qx * 4, cell.y, qz * 4)))
        });
        let effects = &vitals.effects;
        // `LocalPlayer.waterVisionTime`: up a tick per tick under water, down ten out of it.
        if medium == Medium::Water {
            state.water_ticks = (state.water_ticks + scene.ticks * if spectator { 10.0 } else { 1.0 }).min(600.0);
        } else {
            state.water_ticks = (state.water_ticks - scene.ticks * 10.0).max(0.0);
        }
        // `DarknessFogEnvironment` applies while the effect is on, scaled by
        // its blend factor (`getEffectBlendFactor`), which fades in and out.
        let darkness = effects.has(Effect::Darkness).then(|| effects.darkness_blend(scene.alpha as f32));
        let weather = if dimension == Dimension::Overworld {
            atm::Weather { rain: self.weather.rain_level(), thunder: self.weather.thunder_level() }
        } else {
            atm::Weather::default()
        };
        let eye = atm::Eye {
            dimension,
            local,
            precipitation: dimension == Dimension::Overworld
                && atm::overworld_biome(self.world.biome_at(cell)).precipitation,
            day_ticks: self.day_time * atm::DAY_TICKS,
            weather,
            sky_flash: dimension == Dimension::Overworld
                && !self.settings.hide_lightning_flashes
                && self.mobs.entities.lightning.iter().any(|b| b.visible()),
            render_distance: meshed_radius(self.world.render_distance(), camera),
            cloud_range: self.settings.cloud_range,
            sky_light: self.world.sky_light(cell).unwrap_or(15),
            camera_y: camera.y as f32,
            forward_x: forward.x,
            medium,
            spectator,
            fire_resistance: effects.has(Effect::FireResistance),
            blindness: effects.get(Effect::Blindness).map(|e| e.ticks),
            darkness,
            night_vision: if darkness.is_some() { 0.0 } else { effects.night_vision(scene.time) },
            water_vision: if medium == Medium::Water { atm::water_vision(state.water_ticks as u32) } else { 0.0 },
        };
        state.rain.update(&eye, scene.ticks);
        Atmosphere {
            fog: atm::fog(&eye, state.rain),
            sky_light: atm::sky_light(&eye),
            cloud_color: atm::cloud_color(&eye),
        }
    }

    /// The clouds this frame draws: Java draws none where the cloud colour
    /// is transparent (the Nether and the End) or with Clouds OFF. They
    /// drift with the animation clock, so held captures keep them still.
    pub(super) fn cloud_params(&self, scene: &Scene, color: [f32; 4]) -> Option<crate::render::clouds::CloudParams> {
        (color[3] > 0.0 && self.settings.clouds != voxelcraft::atmosphere::Clouds::Off).then(|| {
            crate::render::clouds::CloudParams {
                mode: self.settings.clouds,
                range: self.settings.cloud_range,
                ticks: f64::from(scene.time) * 20.0,
                height: voxelcraft::atmosphere::dimension(self.dimension).cloud_height,
            }
        })
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
                    Event::HoneyDrink => {
                        self.audio.play(
                            crate::audio::Sound::HoneyDrink,
                            Some(bot.agent.session.player.eye()),
                            0.7,
                            (0.9, 1.1),
                        );
                    }
                    Event::Chew => {
                        let sound = crate::audio::sounds::Sound::Step(crate::audio::sounds::Material::Snow);
                        self.audio.play(sound, Some(bot.agent.session.player.eye()), 0.7, (1.4, 1.7));
                    }
                    Event::Shot(shot) => super::items::shot_sound(&mut self.audio, shot.ranged),
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
            bot.hand.update(
                dt,
                a.inventory.get(a.selected).map(|s| s.item),
                a.session.item_swap_scale(a.selected, 1.0),
                a.session.draw_ticks(),
            );
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
            let eye = feet
                + (a.player.eye() - a.player.pos)
                + glam::DVec3::Y * a.player.camera_eye_offset(scene.alpha as f32);
            let distance = self.mobs.entities.camera_distance(a.player.vehicle);
            let (camera, forward) = bot.camera.view_at(&self.world, eye, a.player.forward(), distance);
            let atmosphere = self.atmosphere(
                scene,
                view + 1,
                camera,
                forward,
                &a.vitals,
                a.session.mode == super::GameMode::Spectator,
            );
            let fog_end = atmosphere.end();
            let draw = atmosphere.draw_distance(a.player.pos, self.world.render_distance());
            let models = self.block_models(scene.alpha, camera, &draw);
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
            if self.session.vitals.visible() {
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
                            self.session.inventory.get(self.body.actions.selected),
                        ),
                    ),
                    self.lan.profile.as_str(),
                ));
            }
            for (other, b) in &self.agents.players {
                if other != name && b.active && b.agent.session.vitals.visible() {
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
                                b.agent.session.inventory.get(b.agent.selected),
                            ),
                        ),
                        other.as_str(),
                    ));
                }
            }
            self.mobs.entities.mesh(&self.world.maps, camera, forward, draw, scene.time, scene.alpha);
            // v0.18 Track 6: ground shadows, for the players drawn in this view too.
            let mut casters = super::shadow_casters(&self.world, others.iter().map(|o| o.1), scene.alpha);
            if !bot.camera.first_person() && super::casts_shadow(&a.vitals, a.mode) {
                casters.push(voxelcraft::entity::shadow::player(feet));
            }
            let shadows = self.settings.entity_shadows;
            let verts = self.mobs.entities.shadows(&self.world, camera, scene.alpha, &casters, shadows);
            super::push_avatars(&self.world, &others, camera, fog_end, scene.time, verts);
            crate::render::signs::text(
                &mut self.mobs.sign_text,
                crate::render::signs::FONT_VERSION,
                &self.world,
                camera,
                verts,
            );
            crate::entity::model::build_beacon_beams(&self.world, camera, scene.time, draw, verts);
            crate::entity::model::build_conduits(&self.world, camera, draw, verts);
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
                        bot.hand.appearance(
                            &a.vitals,
                            pad.map_or(a.eating(), |p| p.1),
                            scene.alpha,
                            a.inventory.armor,
                            a.inventory.get(bot.agent.selected),
                        ),
                    ),
                    verts,
                );
            }
            self.renderer.set_entities(verts);
            super::weather::sheets(&self.world, camera, scene.rain, &mut self.weather_verts);
            self.renderer.set_weather(&self.weather_verts);
            let mut params = FrameParams {
                camera,
                forward,
                view_effect: voxelcraft::camera::view_effect(
                    &a.player,
                    &a.vitals,
                    scene.alpha as f32,
                    self.settings.view_bobbing,
                    self.settings.damage_tilt,
                ),
                // v0.15 Track B: a scoping player's view zooms to a tenth.
                fov_y: self.settings.fov.to_radians()
                    * if bot.camera.first_person() && a.session.scoping() {
                        voxelcraft::content::spyglass::ZOOM_FOV_MODIFIER
                    } else {
                        1.0
                    },
                sky_color: [0.0; 3],
                fog_color: [0.0; 3],
                fog_start: 0.0,
                fog_end: 0.0,
                fog_render: [0.0; 2],
                sky_end: 0.0,
                cloud_end: 0.0,
                sunrise: [0.0; 4],
                sky_light_color: [1.0; 3],
                clouds: self.cloud_params(scene, atmosphere.cloud_color),
                cloud_color: [0.0; 4],
                daylight: if self.dimension.has_sky() { atmosphere.sky_light.0 } else { scene.sky.daylight },
                zenith_color: [0.0; 3],
                sun_dir: scene.sky.sun_dir,
                dimension: self.dimension,
                enhanced_graphics: self.settings.enhanced_graphics,
                time: scene.time,
                highlight,
                cracks,
                block_models: {
                    let mut models = models;
                    crate::render::signs::shelf_items(&self.world, camera, &mut models);
                    crate::render::brushable::emerging_items(
                        &self.world,
                        camera,
                        crate::render::signs::RANGE,
                        &mut models,
                    );
                    models
                },
                signs: crate::render::signs::models(&self.world, camera),
                hand: (bot.camera.first_person() && self.show_hud && !a.vitals.is_dead() && !a.session.scoping()).then(
                    || {
                        let eating = pad.map_or(a.eating(), |p| p.1);
                        let held = a.session.inventory.get(a.selected);
                        self.map_hand(
                            &a.session,
                            a.selected,
                            Self::stamp_hand(
                                held.map_or(host_dial, |s| host_dial.for_stack(&s)).at(
                                    a.player.yaw,
                                    a.player.pos.x,
                                    a.player.pos.z,
                                ),
                                bot.hand.view(
                                    eating,
                                    crate::entity::sky_light(&self.world, camera),
                                    self.torch_light(camera),
                                ),
                            ),
                        )
                    },
                ),
                rain: scene.rain,
                night_vision: a.vitals.effects.vision(scene.time, a.player.head_in_water(&self.world)),
                darkness: crate::render::Darkness::of(&a.vitals.effects, scene.alpha as f32),
                ui,
                // Controller players' screens are in-game panels: no blur.
                blur: 0,
                ui_under_blur: 0,
            };
            atmosphere.apply(&mut params);
            self.renderer.set_particles(&self.particles.pool, &self.world, camera, forward, scene.alpha);
            self.renderer.draw_view(frame, &params, vp);
        }
    }

    /// A followed player's HUD: name, crosshair, hotbar, health and hunger,
    /// and their death message.
    fn follower_ui(&self, name: &str, bot: &Bot, (w, h): (u32, u32), now: Instant) -> Vec<UiVertex> {
        let a = &bot.agent;
        let mut ui = Ui::with_scale(w as f32, h as f32, self.view_scale((w, h)));
        let (sw, sh) = ui.size();
        super::hud::frozen_overlay(&mut ui, sw, sh, a.vitals.ticks_frozen);
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
            shield_cooldown: a.session.shield_cooldown_fraction(),
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
    use crate::world::{nether_biome::NetherBiome, terrain::Dimension};
    use voxelcraft::atmosphere::{Local, Sampler, biome_visuals, dimension, nether_biome};

    #[test]
    fn the_fog_reaches_only_as_far_as_terrain_is_drawn() {
        use glam::DVec3;
        // From a chunk's centre at render distance 6 (12 Java chunks, 192
        // blocks), the nearest undrawn chunk's column is 176.7 blocks away.
        let r = super::meshed_radius(6, DVec3::new(16.0, 70.0, 16.0));
        assert!((r - 176.726).abs() < 0.01, "{r}");
        // Never past Java's distance, and at most a chunk and a half short of it.
        for (rd, x) in [(8, 0.5), (8, 31.5), (16, 12.0), (2, 5.0)] {
            let r = super::meshed_radius(rd, DVec3::new(x, 0.0, 3.0));
            assert!(r <= (rd * 32) as f32 && r >= (rd as f32 - 1.5) * 32.0, "{rd} {x}: {r}");
        }
    }

    #[test]
    fn each_split_screen_view_blends_the_fog_around_its_own_eye() {
        // Soul sand valley to the west of x = 0 and crimson forest to the east.
        let local = |qx: i32, _qz: i32| {
            let biome = if qx < 0 { NetherBiome::SoulSandValley } else { NetherBiome::CrimsonForest };
            Local::resolve(dimension(Dimension::Nether), nether_biome(biome))
        };
        let (mut host, mut follower) = (Sampler::default(), Sampler::default());
        let west = host.sample(-200.0, 0.0, local);
        let east = follower.sample(200.0, 0.0, local);
        assert_eq!(west.fog, NetherBiome::SoulSandValley.fog_color());
        assert_eq!(east.fog, NetherBiome::CrimsonForest.fog_color());
        // The host's reading is unchanged by the follower sampling elsewhere.
        assert_eq!(host.sample(-200.0, 0.0, local), west);
        assert!(biome_visuals("the_end").is_some());
    }
}
