//! Fixed gameplay tick coordination, separate from frame rendering.
use super::*;
impl Game {
    /// One fixed gameplay step. Rendering and worker polling never
    /// change the amount of simulation time advanced here.
    pub(super) fn tick(&mut self) {
        self.world.set_tile_drops(self.gamerules.bool("doTileDrops"));
        let dt = crate::simulation::TICK_SECONDS;
        let arriving = self.update_arrival();
        let input = self.movement_input(arriving);
        self.jump_pressed = false;
        self.previous_eye = self.player.eye();
        let before = self.player.pos;
        if !arriving {
            if self.player.vehicle.is_some()
                && input.descend
                && let Some(pos) = self.mobs.entities.dismount(&self.world, crate::entity::PlayerId::HOST)
            {
                self.player.pos = pos;
                self.player.vehicle = None;
                self.player.vel = DVec3::ZERO;
            }
            if self.player.vehicle.is_none() {
                self.player.apply_effects(&self.vitals.effects);
                let armor = &self.inventory.armor;
                self.player.wear_boots(crate::enchant::armor_level(armor, crate::enchant::Enchantment::DepthStrider));
                self.player.update(dt, input, &self.world);
                crate::particles::water_entry(&self.player, before, &mut self.world);
                self.update_portal(dt);
            } else {
                self.player.vel = DVec3::ZERO;
                self.player.on_ground = true;
            }
        }
        let moved = (self.player.pos - before).with_y(0.0).length();
        if self.gamerules.bool("doWeatherCycle") {
            self.weather.update(dt);
        }
        self.update_sleep(dt);
        self.world.raining = self.weather.raining && self.dimension.has_sky();
        let env = crate::simulation::survival::Env {
            respiration: crate::enchant::armor_level(&self.inventory.armor, crate::enchant::Enchantment::Respiration),
            frost_walker: crate::enchant::armor_level(&self.inventory.armor, crate::enchant::Enchantment::FrostWalker)
                > 0,
            turtle_helmet: self.inventory.armor[0].is_some_and(|s| s.item == Item::TURTLE_HELMET),
            ..crate::simulation::player_environment(&self.player, &self.world, input, moved)
        };
        let hurts = if arriving || self.arrival.is_some() {
            Default::default()
        } else {
            self.vitals.tick_rules(
                dt as f32,
                &env,
                self.mode.invulnerable(),
                self.difficulty,
                self.gamerules.bool("naturalRegeneration"),
            )
        };
        self.trample(hurts.landed);
        if hurts.fall > 0.0 && self.gamerules.bool("fallDamage") {
            self.damage_player(hurts.fall, survival::CAUSE_FALL);
        }
        if hurts.drown > 0.0 && self.gamerules.bool("drowningDamage") {
            self.damage_player(hurts.drown, survival::CAUSE_DROWN);
        }
        if hurts.lava > 0.0 && self.gamerules.bool("fireDamage") {
            self.damage_player_armored(hurts.lava, survival::CAUSE_LAVA);
        }
        if hurts.fire > 0.0 && self.gamerules.bool("fireDamage") {
            self.damage_player_armored(hurts.fire, survival::CAUSE_FIRE);
        }
        if hurts.burn > 0.0 && self.gamerules.bool("fireDamage") {
            self.damage_player(hurts.burn, survival::CAUSE_FIRE);
        }
        if hurts.starve > 0.0 {
            self.damage_player(hurts.starve, survival::CAUSE_STARVE);
        }

        let acting = self.menu.is_none() && !self.console.open && self.mouse_grabbed && !self.inventory_open;
        self.act(acting, dt);
        self.drive_lan(dt);
        self.drive_pads(dt);
        self.tick_agents();
        self.ride_minecarts(input);
        let sky_darken = ((1.0 - ((self.day_time - 0.25) * std::f64::consts::TAU).cos()).clamp(0.0, 1.0) * 11.0
            + self.weather.strength as f64 * 4.0)
            .round() as u8;
        self.world.set_redstone_daylight(self.day_time, sky_darken);
        let players = std::iter::once((self.player.pos, self.player.collision_shape()))
            .filter(|_| self.mode != GameMode::Spectator && self.vitals.health > 0.0)
            .chain(
                self.agents
                    .players
                    .values()
                    .filter(|b| b.active && b.agent.mode != GameMode::Spectator && b.agent.vitals.health > 0.0)
                    .map(|b| (b.agent.player.pos, b.agent.player.collision_shape())),
            );
        self.world.redstone_contacts(players, &self.mobs.entities);
        crate::simulation::tick_world_rules(
            &mut self.world,
            self.player.pos,
            self.gamerules.bool("doFireTick"),
            self.gamerules.int("randomTickSpeed") as u32,
        );
        self.play_notes();
        self.world.tick_automation_entities(&mut self.mobs.entities);
        self.update_mobs(dt);
        self.update_items();
        self.tick_particles();
        if self.gamerules.bool("doDaylightCycle") {
            self.day_time += dt / DAY_LENGTH;
            if self.day_time >= 1.0 {
                self.day_time = self.day_time.fract();
                self.day_count = self.day_count.saturating_add(1);
            }
        }
    }
}
