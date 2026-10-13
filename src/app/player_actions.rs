//! Client interaction adapters and presentation around shared engine operations.
use super::*;
impl Game {
    pub(super) fn target(&self) -> Option<(glam::IVec3, glam::IVec3)> {
        self.session.block_target(&self.world)
    }

    /// Creative still breaks on the input edge; execution uses the shared rules.
    pub(super) fn break_block(&mut self) {
        if !self.attacking() {
            self.mine_held(crate::simulation::TICK_SECONDS);
        }
    }

    /// Sends the dragon egg at `pos` somewhere nearby, with a teleport sound.
    pub(super) fn teleport_egg(&mut self, pos: glam::IVec3) {
        if let Some(to) = self.world.teleport_egg(pos) {
            self.audio.play(crate::audio::sounds::Sound::Teleport, Some(to.as_dvec3()), 0.6, (0.9, 1.1));
        }
    }

    /// Fixed-tick mining; the client only presents the shared outcome.
    pub(super) fn continue_breaking(&mut self, dt: f64) {
        if self.attacking() {
            self.session.cancel_mining();
            self.actions.breaking = None;
            return;
        }
        self.mine_held(dt);
    }

    fn mine_held(&mut self, dt: f64) {
        use voxelcraft::session::mining::Outcome;
        let result =
            self.session.tick_mining(self.actions.selected, &mut self.world, &mut self.mobs.entities, &self.gamerules);
        match result {
            Ok(Outcome::Hit { cell, block, .. }) => self.audio.block_hit(block, cell, dt),
            Ok(Outcome::Broken { cell, block, tool_broken, .. }) => {
                self.audio.block_break(block, cell);
                if self.session.mode.is_survival() {
                    self.action_cooldown = BREAK_DELAY;
                }
                if let Some(item) = tool_broken {
                    self.show_popup(&format!("{} broke", capitalize(item.name())));
                }
            }
            Ok(Outcome::TeleportedEgg { destination, .. }) => {
                self.action_cooldown = BREAK_DELAY;
                if let Some(to) = destination {
                    self.audio.play(crate::audio::sounds::Sound::Teleport, Some(to.as_dvec3()), 0.6, (0.9, 1.1));
                }
            }
            Err(_) => self.session.cancel_mining(),
        }
        self.actions.breaking = self.session.breaking(&self.world);
    }

    /// Held use advances the shared fixed-tick consumption clock. Dried kelp
    /// takes 0.8 seconds; other currently registered food/drinks take 1.6.
    pub(super) fn eat(&mut self, acting: bool, dt: f64) {
        let held = self.held_item();
        let milk = held == Some(Item::MILK_BUCKET);
        let potion = held.and_then(|i| i.as_potion());
        let food = self.held_item().and_then(|i| i.food());
        let hungry = self.session.mode.is_survival() && self.session.vitals.hunger.can_eat();
        // Right-clicking a composter fills it instead, as in Java.
        let composting = !self.sneak_building()
            && self
                .target()
                .and_then(|(pos, _)| self.world.get_block(pos))
                .is_some_and(|b| crate::world::composter::level(b).is_some());
        let using =
            acting && self.right_held && (milk || potion.is_some() || (food.is_some() && hungry && !composting));
        if !using {
            self.session.cancel_consumption();
            self.actions.eat_timer = 0.0;
            return;
        }
        let before = self.session.consumption_elapsed();
        let result = self.session.tick_consumption(self.actions.selected, using, &mut self.mobs.entities);
        let Ok(step) = result else {
            self.session.cancel_consumption();
            self.actions.eat_timer = 0.0;
            return;
        };
        // This adapter field is now presentation only; progress belongs to the session.
        self.actions.eat_timer = f64::from(step.progress) * EAT_TIME;
        if using && (before / 0.25).floor() != ((before + dt) / 0.25).floor() {
            let sound = crate::audio::sounds::Sound::Step(crate::audio::sounds::Material::Snow);
            self.audio.play(sound, Some(self.session.player.eye()), 0.7, (1.4, 1.7));
        }
        if let Some(consumed) = step.completed {
            self.damage_player(consumed.magic_damage, survival::CAUSE_MAGIC);
        }
    }

    /// Right-click with armor in hand puts it on (swapping out the worn
    /// piece), unless aimed at a container. Returns whether it did.
    pub(super) fn equip_held(&mut self) -> bool {
        if !voxelcraft::survival_items::equip_armor(
            &self.session.player,
            &mut self.session.inventory,
            self.actions.selected,
            self.session.mode,
            &self.world,
        ) {
            return false;
        }
        let sound = crate::audio::sounds::Sound::Place(crate::audio::sounds::Material::Wood);
        self.audio.play(sound, Some(self.session.player.eye()), 0.6, (1.4, 1.6));
        true
    }

    /// Torch light at a point, 0..1, for lighting things drawn outside the
    /// chunk meshes.
    pub(super) fn torch_light(&self, p: glam::DVec3) -> f32 {
        self.world.block_light(p.floor().as_ivec3()) as f32 / 15.0
    }

    /// The item in the selected hotbar slot.
    pub(super) fn held_item(&self) -> Option<Item> {
        self.session.inventory.get(self.actions.selected).map(|s| s.item)
    }

    /// Wears the held tool for a block broken or a mob hit (survival).
    pub(super) fn wear_held(&mut self, hitting_mob: bool) {
        let Some(held) = self.held_item() else { return };
        if self.session.mode.is_survival()
            && self.session.inventory.wear(self.actions.selected, crate::mining::wear(held, hitting_mob))
        {
            self.tool_broke(held);
        }
    }

    /// Popup and sound for a held tool that just broke.
    pub(super) fn tool_broke(&mut self, held: Item) {
        self.show_popup(&format!("{} broke", capitalize(held.name())));
        self.audio.play(
            crate::audio::sounds::Sound::Break(crate::audio::sounds::Material::Wood),
            Some(self.session.player.eye()),
            0.8,
            (1.3, 1.5),
        );
    }
}
