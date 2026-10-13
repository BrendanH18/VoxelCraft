//! Client interaction adapters and presentation around shared engine operations.
use super::*;
impl Game {
    pub(super) fn target(&self) -> Option<(glam::IVec3, glam::IVec3)> {
        self.world.raycast(self.session.player.eye(), self.session.player.forward().as_dvec3(), REACH)
    }

    /// Instant break (creative).
    pub(super) fn break_block(&mut self) {
        if !self.session.mode.can_build() || self.attacking() {
            return;
        }
        if let Some((pos, _)) = self.target()
            && let Some(block) = self.world.get_block(pos)
            && block != Block::BEDROCK
        {
            if self.held_item() == Some(Item::SHEARS) {
                self.world.disarm_tripwire(pos);
            }
            self.world.set_block(pos, Block::AIR);
            self.audio.block_break(block, pos);
            if crate::entity::nether::guarded_by_piglins(block) {
                self.piglins_notice(false);
            }
            if block.is_bed() {
                self.break_bed_partner(pos, block);
            }
            if block.is_door() {
                self.break_door_partner(pos, block);
            }
        }
    }

    /// Sends the dragon egg at `pos` somewhere nearby, with a teleport sound.
    pub(super) fn teleport_egg(&mut self, pos: glam::IVec3) {
        if let Some(to) = self.world.teleport_egg(pos) {
            self.audio.play(crate::audio::sounds::Sound::Teleport, Some(to.as_dvec3()), 0.6, (0.9, 1.1));
        }
    }

    /// Timed break with drops (survival). Called every frame while the
    /// button is held.
    pub(super) fn continue_breaking(&mut self, dt: f64) {
        if !self.session.mode.can_build() || self.attacking() {
            self.actions.breaking = None;
            return;
        }
        let Some((pos, face)) = self.target() else {
            self.actions.breaking = None;
            return;
        };
        let Some(block) = self.world.get_block(pos) else { return };
        // The dragon egg won't be mined: it jumps away from the hit.
        if block == Block::DRAGON_EGG {
            self.actions.breaking = None;
            self.action_cooldown = BREAK_DELAY;
            self.teleport_egg(pos);
            return;
        }
        let held = self.held_item();
        let digger = crate::mining::Digger {
            held: self.session.inventory.get(self.actions.selected),
            helmet: self.session.inventory.armor[0].map_or(Default::default(), |s| s.enchants),
            eyes_in_water: self.session.player.head_in_water(&self.world),
            on_ground: self.session.player.on_ground || self.session.player.flying,
        };
        self.world.touch_redstone_ore(pos);
        let progress = self.actions.mine(
            pos,
            self.world.get_block(pos).unwrap_or(block),
            crate::mining::dig_time(block, digger) / self.session.vitals.effects.mining_factor(),
            dt,
        );
        if progress < 1.0 {
            self.audio.block_hit(block, pos, dt);
            self.world.particles.push(crate::particles::Request::Hit { cell: pos, block, face });
            return;
        }
        self.actions.breaking = None;
        self.action_cooldown = BREAK_DELAY;
        // Broken ice melts into water, unless it was floating over nothing
        // or silk touch keeps it whole.
        let tool = digger.held.map_or(Default::default(), |s| s.active_enchants());
        let melts = block == Block::ICE
            && !tool.has(crate::enchant::Enchantment::SilkTouch)
            && self.dimension.has_sky()
            && self.world.get_block(pos - glam::IVec3::Y).is_some_and(|b| b != Block::AIR);
        if digger.held.is_some_and(|s| s.item == Item::SHEARS) {
            self.world.disarm_tripwire(pos);
        }
        self.world.set_block(pos, if melts { Block::WATER } else { Block::AIR });
        if melts {
            self.world.particles.push(crate::particles::Request::Break { cell: pos, block });
        }
        self.audio.block_break(block, pos);
        if crate::entity::nether::guarded_by_piglins(block) {
            self.piglins_notice(false);
        }
        // Stone, ores and the like only drop with a good enough pickaxe.
        if self.gamerules.bool("doTileDrops") && crate::mining::can_harvest(block, held) {
            self.world.spill_with_item(pos, block, digger.held);
            self.mobs.entities.drop_mined_xp(block, pos, tool);
        }
        if block.is_bed() {
            self.break_bed_partner(pos, block);
        }
        if block.is_door() {
            self.break_door_partner(pos, block);
        }
        self.session.vitals.hunger.exhaust(survival::EXHAUST_MINE);
        if block.hardness() > 0.0 {
            self.wear_held(false);
        }
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
            self.show_popup(&format!("{} broke", capitalize(held.name())));
            self.audio.play(
                crate::audio::sounds::Sound::Break(crate::audio::sounds::Material::Wood),
                Some(self.session.player.eye()),
                0.8,
                (1.3, 1.5),
            );
        }
    }
}
