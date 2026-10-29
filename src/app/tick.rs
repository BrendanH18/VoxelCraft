//! Fixed gameplay tick coordination, separate from frame rendering.
use super::*;
impl Game {
    /// One fixed gameplay step. Rendering and worker polling never
    /// change the amount of simulation time advanced here.
    pub(super) fn tick(&mut self) {
        self.world.set_simulation_players(
            std::iter::once(self.session.player.pos)
                .chain(self.agents.players.values().filter(|b| b.active).map(|b| b.agent.session.player.pos)),
        );
        self.world.set_simulation_spectators(
            std::iter::once(self.session.mode == voxelcraft::rules::GameMode::Spectator).chain(
                self.agents
                    .players
                    .values()
                    .filter(|b| b.active)
                    .map(|b| b.agent.session.mode == voxelcraft::rules::GameMode::Spectator),
            ),
        );
        self.world.set_tile_drops(self.gamerules.bool("block_drops"));
        // Before any interaction this tick: breaking a cart or lighting a TNT
        // cart reads these rules directly in Java.
        self.mobs.entities.entity_drops = self.gamerules.bool("entity_drops");
        self.mobs.entities.tnt_explodes = self.gamerules.bool("tnt_explodes");
        self.world.rules =
            crate::world::WorldRules { difficulty: self.difficulty.id(), ..self.gamerules.world_rules() };
        let dt = crate::simulation::TICK_SECONDS;
        self.session.settle_wake(&self.world);
        let arriving = self.update_arrival();
        let input = self.movement_input(arriving);
        self.jump_pressed = false;
        self.previous_eye = self.session.player.eye();
        let before = self.session.player.pos;
        let portal_before = self.portal_time;
        if !arriving {
            let on_cushion = self
                .session
                .player
                .vehicle
                .and_then(|id| self.mobs.entities.cart(id))
                .is_some_and(|c| c.kind.cushion().is_some());
            if self.session.player.vehicle.is_some()
                && input.descend
                && let Some(pos) = self.mobs.entities.dismount(&self.world, crate::entity::PlayerId::HOST)
            {
                // `Cushion.removePassenger`: `CUSHION_GET_UP`, from the shared wool set.
                if on_cushion {
                    let source = crate::audio::SoundSource::Neutral;
                    let sound = crate::audio::sounds::Sound::Step(crate::audio::sounds::Material::Snow);
                    self.audio.play_in(source, sound, Some(pos), 1.0, (1.1, 1.2));
                }
                self.session.player.pos = pos;
                self.session.player.vehicle = None;
                self.session.player.vel = DVec3::ZERO;
            }
            // `LivingEntity.canFreeze` reads the current armour, riding or not.
            self.session.player.powder_snow_equipment(&self.session.inventory.armor, self.session.vitals.ticks_frozen);
            if self.session.player.vehicle.is_none() {
                self.session.player.apply_effects(&self.session.vitals.effects);
                let armor = &self.session.inventory.armor;
                self.session
                    .player
                    .wear_boots(crate::enchant::armor_level(armor, crate::enchant::Enchantment::DepthStrider));
                self.session.player.tick(input, &self.world);
                self.world.powder_snow_fire(&self.session.player, self.session.vitals.burning());
                // `BigDripleafBlock.entityInside`: a leaf stood on starts to tilt.
                self.world.dripleaf_step(self.session.player.pos, self.session.player.on_ground);
                crate::particles::movement_effects(&self.session.player, before, &mut self.world);
                self.update_portal(dt);
            } else {
                self.session.player.vel = DVec3::ZERO;
                self.session.player.on_ground = true;
                self.session.player.refresh_riding_contact(&self.world);
            }
        }
        let nausea = self.session.vitals.effects.get(voxelcraft::simulation::effects::Effect::Nausea).map(|e| e.ticks);
        self.distortion.tick(nausea, self.portal_time > portal_before);
        let moved = (self.session.player.pos - before).with_y(0.0).length();
        self.session.tick_lodestones(&self.world);
        self.weather.update_rules(dt, self.gamerules.bool("advance_weather"));
        self.update_sleep(dt);
        // Java's isRaining compares the faded rain level, not the target state.
        self.world.raining = self.weather.is_raining() && self.dimension.has_sky();
        let env = crate::simulation::survival::Env {
            respiration: crate::enchant::armor_level(
                &self.session.inventory.armor,
                crate::enchant::Enchantment::Respiration,
            ),
            frost_walker: crate::enchant::armor_level(
                &self.session.inventory.armor,
                crate::enchant::Enchantment::FrostWalker,
            ) > 0,
            turtle_helmet: self.session.inventory.armor[0].is_some_and(|s| s.item == Item::TURTLE_HELMET),
            stalactite: std::mem::take(&mut self.session.stalactite),
            ..crate::simulation::player_environment(&self.session.player, &self.world, moved)
        };
        let burning = self.session.vitals.burning();
        let hurts = if arriving || self.arrival.is_some() {
            Default::default()
        } else {
            self.session.vitals.tick_rules(
                dt as f32,
                &env,
                self.session.mode.invulnerable(),
                self.difficulty,
                self.gamerules.bool("natural_health_regeneration"),
            )
        };
        if burning && self.session.mode.can_build() {
            let top = self.session.player.collision_shape().aabb(self.session.player.pos).1.y;
            self.world.douse_in_cauldron(self.session.player.pos, top);
        }
        if hurts.freeze > 0.0 {
            self.damage_player(hurts.freeze, survival::CAUSE_FREEZE);
        }
        self.trample(hurts.landed);
        if hurts.fall > 0.0 {
            self.damage_player(hurts.fall, survival::CAUSE_FALL);
        }
        if hurts.drown > 0.0 {
            self.damage_player(hurts.drown, survival::CAUSE_DROWN);
        }
        if hurts.lava > 0.0 {
            self.damage_player_armored(hurts.lava, survival::CAUSE_LAVA);
        }
        if hurts.stalactite > 0.0 {
            self.damage_player_armored(hurts.stalactite, survival::CAUSE_STALACTITE);
        }
        if hurts.fire > 0.0 {
            self.damage_player_armored(hurts.fire, survival::CAUSE_FIRE);
        }
        if hurts.burn > 0.0 {
            self.damage_player(hurts.burn, survival::CAUSE_FIRE);
        }
        if hurts.starve > 0.0 {
            self.damage_player(hurts.starve, survival::CAUSE_STARVE);
        }

        let acting = !self.menu.is_open() && !self.console.open && self.mouse_grabbed && !self.inventory_open;
        self.act(acting, dt);
        self.drive_lan(dt);
        self.drive_pads(dt);
        self.tick_agents();
        self.explode_anchors();
        self.ride_minecarts(input);
        // Detectors share Java's skyDarken with spawning, then apply the sun
        // angle themselves (DaylightDetectorBlock.updateSignalStrength).
        let ticks = (self.day_time.rem_euclid(1.) * 24000.).floor() as u32;
        let darken = crate::simulation::lighting::SpawnLight::for_dimension(
            self.dimension,
            ticks,
            self.weather.rain_level(),
            self.weather.thunder_level(),
        )
        .sky_darken;
        let sun_angle = if self.dimension == crate::world::terrain::Dimension::Overworld {
            crate::simulation::lighting::sun_angle(ticks)
        } else {
            0.0
        };
        self.world.set_redstone_daylight(self.day_time, darken, sun_angle);
        let players = std::iter::once((self.session.player.pos, self.session.player.collision_shape()))
            .filter(|_| self.session.mode != GameMode::Spectator && self.session.vitals.health > 0.0)
            .chain(
                self.agents
                    .players
                    .values()
                    .filter(|b| {
                        b.active && b.agent.session.mode != GameMode::Spectator && b.agent.session.vitals.health > 0.0
                    })
                    .map(|b| (b.agent.session.player.pos, b.agent.session.player.collision_shape())),
            );
        self.world.redstone_contacts(players, &self.mobs.entities);
        // Every chest a player has open, whichever input path opened it.
        let keyboard = match self.body.container {
            super::Container::Chest(p) if self.inventory_open => Some(p),
            _ => None,
        };
        let open: Vec<_> = keyboard.into_iter().chain(self.pads.open_chests()).chain(self.open_chests()).collect();
        self.world.set_chest_viewers(open);
        crate::simulation::tick_world_rules(
            &mut self.world,
            self.session.player.pos,
            self.gamerules.int("fire_spread_radius_around_player") != 0,
            self.gamerules.int("random_tick_speed") as u32,
        );
        self.play_notes();
        self.mobs.entities.stalactite_hits(self.world.stalactite_hits());
        // Players in a stalactite's cell as it lands are hit too, wherever
        // they move before their next vitals tick applies it.
        let landed = |world: &crate::world::World, s: &mut voxelcraft::session::PlayerSession| {
            let (min, max) = s.player.collision_shape().aabb(s.player.pos);
            s.stalactite = s.stalactite.max(world.stalactite_damage(min, max));
        };
        landed(&self.world, &mut self.session);
        for bot in self.agents.players.values_mut().filter(|b| b.active) {
            landed(&self.world, &mut bot.agent.session);
        }
        self.world.tick_automation_entities(&mut self.mobs.entities);
        self.update_mobs(dt);
        self.update_items();
        self.tick_particles();
        (self.day_time, self.day_count) =
            crate::simulation::advance_day(self.day_time, self.day_count, dt, &self.gamerules);
    }
}
