//! Client interaction adapters and presentation around shared engine operations.
use super::*;
impl Game {
    pub(super) fn target(&self) -> Option<(glam::IVec3, glam::IVec3)> {
        self.world.raycast(self.player.eye(), self.player.forward().as_dvec3(), REACH)
    }

    /// Instant break (creative).
    pub(super) fn break_block(&mut self) {
        if !self.mode.can_build() || self.attacking() {
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
        if !self.mode.can_build() || self.attacking() {
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
            held: self.inventory.get(self.actions.selected),
            helmet: self.inventory.armor[0].map_or(Default::default(), |s| s.enchants),
            eyes_in_water: self.player.head_in_water(&self.world),
            on_ground: self.player.on_ground || self.player.flying,
        };
        self.world.touch_redstone_ore(pos);
        let progress = self.actions.mine(
            pos,
            self.world.get_block(pos).unwrap_or(block),
            crate::mining::dig_time(block, digger) / self.vitals.effects.mining_factor(),
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
        self.vitals.hunger.exhaust(survival::EXHAUST_MINE);
        if block.hardness() > 0.0 {
            self.wear_held(false);
        }
    }

    /// Eating: holding right-click with food in survival, when not full,
    /// finishes a bite after [`EAT_TIME`] seconds. Potions drink the same
    /// way, in any mode.
    pub(super) fn eat(&mut self, acting: bool, dt: f64) {
        let held = self.held_item();
        let milk = held == Some(Item::MILK_BUCKET);
        let potion = held.and_then(|i| i.as_potion());
        let food = self.held_item().and_then(|i| i.food());
        let hungry = self.mode.is_survival() && self.vitals.hunger.can_eat();
        // Right-clicking a composter fills it instead, as in Java.
        let composting = !self.sneak_building()
            && self
                .target()
                .and_then(|(pos, _)| self.world.get_block(pos))
                .is_some_and(|b| crate::world::composter::level(b).is_some());
        let using =
            acting && self.right_held && (milk || potion.is_some() || (food.is_some() && hungry && !composting));
        if !using {
            self.actions.eat_timer = 0.0;
            return;
        }
        let before = self.actions.eat_timer;
        let finished = self.actions.eat(dt);
        // Chewing sounds four times a second.
        if (before / 0.25).floor() != ((before + dt) / 0.25).floor() {
            let sound = crate::audio::sounds::Sound::Step(crate::audio::sounds::Material::Snow);
            self.audio.play(sound, Some(self.player.eye()), 0.7, (1.4, 1.7));
        }
        if !finished {
            return;
        }
        if milk {
            self.vitals.effects.clear();
            voxelcraft::survival_items::exchange(
                &mut self.inventory,
                self.actions.selected,
                Item::BUCKET,
                self.mode.is_creative(),
                &mut self.mobs.entities,
                &self.player,
            );
        } else if let Some(potion) = potion {
            // Creative keeps the potion; survival is left with the bottle.
            if self.mode.is_survival() {
                self.inventory.slots[self.actions.selected] = Some(crate::inventory::Stack::new(Item::GLASS_BOTTLE, 1));
            }
            let damage = potion.drink(&mut self.vitals);
            self.damage_player(damage, survival::CAUSE_MAGIC);
        } else if let Some((hunger, saturation)) = food {
            let effect = self.held_item().and_then(|item| item.food_effect_roll(self.mobs.entities.roll()));
            if let Some(remainder) = held.and_then(Item::remainder) {
                voxelcraft::survival_items::exchange(
                    &mut self.inventory,
                    self.actions.selected,
                    remainder,
                    false,
                    &mut self.mobs.entities,
                    &self.player,
                );
            } else {
                self.inventory.take_one(self.actions.selected);
            }
            self.vitals.hunger.eat(hunger, saturation);
            if let Some((effect, amp, ticks)) = effect {
                self.vitals.apply_effect(effect, amp, ticks);
            }
        }
    }

    /// Right-click with armor in hand puts it on (swapping out the worn
    /// piece), unless aimed at a container. Returns whether it did.
    pub(super) fn equip_held(&mut self) -> bool {
        if !voxelcraft::survival_items::equip_armor(
            &self.player,
            &mut self.inventory,
            self.actions.selected,
            self.mode,
            &self.world,
        ) {
            return false;
        }
        let sound = crate::audio::sounds::Sound::Place(crate::audio::sounds::Material::Wood);
        self.audio.play(sound, Some(self.player.eye()), 0.6, (1.4, 1.6));
        true
    }

    /// Torch light at a point, 0..1, for lighting things drawn outside the
    /// chunk meshes.
    pub(super) fn torch_light(&self, p: glam::DVec3) -> f32 {
        self.world.block_light(p.floor().as_ivec3()) as f32 / 15.0
    }

    /// The item in the selected hotbar slot.
    pub(super) fn held_item(&self) -> Option<Item> {
        self.inventory.get(self.actions.selected).map(|s| s.item)
    }

    /// Wears the held tool for a block broken or a mob hit (survival).
    pub(super) fn wear_held(&mut self, hitting_mob: bool) {
        let Some(held) = self.held_item() else { return };
        if self.mode.is_survival() && self.inventory.wear(self.actions.selected, crate::mining::wear(held, hitting_mob))
        {
            self.show_popup(&format!("{} broke", capitalize(held.name())));
            self.audio.play(
                crate::audio::sounds::Sound::Break(crate::audio::sounds::Material::Wood),
                Some(self.player.eye()),
                0.8,
                (1.3, 1.5),
            );
        }
    }
}
