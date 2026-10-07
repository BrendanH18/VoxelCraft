//! Game-side glue for dropped items: turning world drops and inventory
//! spill into item entities, picking them up, throwing items (Q, clicking
//! off the inventory panel), dropping everything on death, and drawing them.

use glam::DVec3;

use crate::audio::Audio;
use crate::audio::sounds::Sound;
use crate::inventory::Stack;
use crate::render::BlockModel;
use crate::world::block::Block;

use super::{Container, Game, survival};
use crate::entity::PlayerId;

/// Dropped items farther away than this aren't drawn.
const DRAW_DIST: f64 = 64.0;

impl Game {
    /// Spawns what the world dropped and what didn't fit in the inventory,
    /// then picks up items the player is touching. Closes a furnace or chest
    /// screen whose block is gone.
    pub(super) fn update_items(&mut self) {
        for (cell, stack) in std::mem::take(&mut self.world.drops) {
            self.mobs.entities.drop_from_block(stack, cell);
        }
        for cell in std::mem::take(&mut self.world.brews_done) {
            // Java's brewing stand bubbling as a brew finishes.
            let at = cell.as_dvec3() + DVec3::splat(0.5);
            self.audio.play(crate::audio::sounds::Sound::Swim, Some(at), 0.5, (1.6, 1.9));
        }
        for (cell, xp) in std::mem::take(&mut self.world.xp_drops) {
            self.mobs.entities.spawn_xp(cell.as_dvec3() + DVec3::splat(0.5), xp);
        }
        for stack in self.inventory.take_spill() {
            self.mobs.entities.throw(stack, self.player.eye(), self.player.forward().as_dvec3());
        }
        if !self.vitals.is_dead() {
            self.pick_up_items();
        }
        let gone = match self.container {
            Container::Furnace(pos) => self.world.furnace(pos).is_none(),
            Container::Chest(pos) => self.world.chest(pos).is_none(),
            Container::Brewing(pos) => self.world.brewing_stand(pos).is_none(),
            Container::Enchanting(pos) => {
                self.world.get_block(pos) != Some(crate::world::block::Block::ENCHANTING_TABLE)
            }
            // An anvil can break in use, or fall away.
            Container::Anvil(pos) => !self.world.get_block(pos).is_some_and(|b| b.is_anvil()),
            Container::Smithing(pos) => self.world.get_block(pos) != Some(Block::SMITHING_TABLE),
            _ => false,
        };
        if gone && self.inventory_open {
            self.toggle_inventory();
        }
    }

    fn pick_up_items(&mut self) {
        let player = self.player.pos;
        let mut picked = false;
        self.mobs.entities.items.retain_mut(|item| {
            if item.pickup_delay > 0.0 || !item.touches_player(player) {
                return true;
            }
            let left = self.inventory.add_stack(item.stack);
            picked |= left < item.stack.count;
            item.stack.count = left;
            left > 0
        });
        let arrows = self.mobs.entities.collect_arrows(player);
        if arrows > 0 {
            picked = true;
            let left = self.inventory.add(crate::item::Item::ARROW, arrows);
            if left > 0 {
                self.mobs.entities.scatter(Stack::new(crate::item::Item::ARROW, left), player);
            }
        }
        if picked {
            self.audio.play(Sound::Pop, None, 0.35, (0.8, 1.8));
        }
        if let Some(chime) = crate::entity::orb::absorb(
            &mut self.mobs.entities.orbs,
            player,
            &mut self.vitals.xp,
            &mut self.inventory,
            self.actions.selected,
        ) {
            xp_sounds(&mut self.audio, None, chime);
        }
    }

    /// Right-click with an ender pearl throws it (Java's one-second
    /// cooldown applies). Returns whether one was thrown.
    pub(super) fn throw_pearl(&mut self) -> bool {
        if self.held_item() != Some(crate::item::Item::ENDER_PEARL) || self.vitals.pearl_cooldown > 0.0 {
            return false;
        }
        self.vitals.pearl_cooldown = crate::entity::pearl::COOLDOWN;
        let p = &self.player;
        // Java adds the thrower's motion, vertical only while airborne.
        let carry = if p.on_ground { p.vel.with_y(0.0) } else { p.vel };
        self.mobs.entities.throw_pearl(self.actor, p.eye(), p.forward().as_dvec3(), carry);
        // Java's throw is the bow sound, pitched well down.
        self.audio.play(Sound::Bow, Some(p.eye()), 0.5, (0.42, 0.62));
        if self.mode.is_survival() {
            self.inventory.take_one(self.actions.selected);
        }
        true
    }

    /// Right-click with an eye of ender in the overworld releases it toward
    /// the nearest stronghold (not while aiming at a portal frame).
    pub(super) fn throw_eye(&mut self) -> bool {
        if self.held_item() != Some(crate::item::Item::EYE_OF_ENDER)
            || self.world.generator.dimension != crate::world::terrain::Dimension::Overworld
            || self
                .target()
                .and_then(|(pos, _)| self.world.get_block(pos))
                .is_some_and(|b| b.base() == Block::END_PORTAL_FRAME)
        {
            return false;
        }
        let from = self.player.pos + DVec3::Y * (crate::player::SHAPE.height * 0.5);
        let Some(target) = self.world.generator.strongholds.nearest(from.floor().as_ivec3()) else { return false };
        self.mobs.entities.release_eye(from, target.as_dvec3());
        eye_thrown_sound(&mut self.audio, from);
        if self.mode.is_survival() {
            self.inventory.take_one(self.actions.selected);
        }
        true
    }

    /// Right-click on an empty End portal frame with an eye of ender puts it
    /// in, opening the portal if that completes the ring.
    pub(super) fn insert_eye(&mut self, pos: glam::IVec3) -> bool {
        if self.held_item() != Some(crate::item::Item::EYE_OF_ENDER) {
            return false;
        }
        let Some(opened) = self.world.insert_eye(pos) else { return false };
        frame_filled_sounds(&mut self.audio, pos, opened);
        if self.mode.is_survival() {
            self.inventory.take_one(self.actions.selected);
        }
        true
    }

    /// A pearl landed at `pos`: its thrower (if alive, in this world)
    /// teleports there and takes 5 damage, like a fall.
    pub(super) fn pearl_landed(&mut self, owner: PlayerId, pos: DVec3) {
        if owner == PlayerId::HOST {
            if self.vitals.is_dead() || self.sleeping.is_some() {
                return;
            }
            self.audio.play(Sound::Teleport, Some(self.player.pos + DVec3::Y), 0.8, (0.9, 1.1));
            self.player.pos = pos;
            self.player.vel = DVec3::ZERO;
            self.vitals.reset_fall();
            self.previous_eye = self.player.eye();
            self.damage_player(crate::entity::pearl::DAMAGE, survival::CAUSE_FALL);
        } else if let Some(bot) = self.agents.by_id_mut(owner)
            && bot.active
        {
            let from = bot.agent.player.pos;
            if bot.agent.pearl_teleport(pos, &mut self.mobs.entities) {
                self.audio.play(Sound::Teleport, Some(from + DVec3::Y), 0.8, (0.9, 1.1));
            }
        }
        self.audio.play(Sound::Teleport, Some(pos + DVec3::Y), 0.8, (0.9, 1.1));
    }

    /// Q: drops one of the selected item, or the whole stack with Ctrl.
    /// With the inventory open, drops from the slot under the mouse.
    pub(super) fn drop_selected(&mut self, whole_stack: bool) {
        let slot = if self.inventory_open {
            match self.slot_under_cursor() {
                Some(super::hud::SlotRef::Inventory(i)) => i,
                _ => return,
            }
        } else {
            self.actions.selected
        };
        let Some(stack) = self.inventory.slots[slot] else { return };
        let count = if whole_stack { stack.count } else { 1 };
        self.inventory.slots[slot] = (stack.count > count).then_some(Stack { count: stack.count - count, ..stack });
        if slot == self.actions.selected {
            self.actions.reset();
        }
        self.throw(Stack { count, ..stack });
    }

    /// Clicking off the inventory panel throws the held stack (or one of
    /// it with the right button). Returns whether the click was used.
    pub(super) fn throw_cursor(&mut self, one: bool) -> bool {
        let Some(stack) = self.inventory.cursor else { return false };
        if !self.cursor_off_panel() {
            return false;
        }
        let count = if one { 1 } else { stack.count };
        self.inventory.cursor = (stack.count > count).then_some(Stack { count: stack.count - count, ..stack });
        self.throw(Stack { count, ..stack });
        true
    }

    fn throw(&mut self, stack: Stack) {
        self.mobs.entities.throw(stack, self.player.eye(), self.player.forward().as_dvec3());
    }

    /// A dying survival player drops their whole inventory, including what
    /// was on the crafting grid and the cursor, and seven points of
    /// experience per level (at most 100); the rest is lost.
    pub(super) fn drop_everything(&mut self) {
        let mut stacks = self.inventory.take_all();
        stacks.extend(self.craft.take_all());
        stacks.extend(self.take_work());
        stacks.retain(|s| !s.active_enchants().has(crate::enchant::Enchantment::VanishingCurse));
        for stack in stacks {
            self.mobs.entities.scatter(stack, self.player.pos);
        }
        let xp = self.vitals.xp.die();
        self.mobs.entities.spawn_xp(self.player.pos, xp);
    }

    /// Spinning, bobbing models for nearby dropped items: a small cube for
    /// blocks, a flat icon for everything else, with extra copies for
    /// bigger stacks.
    pub(super) fn item_models(&self, alpha: f64) -> Vec<BlockModel> {
        let mut out = Vec::new();
        for item in &self.mobs.entities.items {
            if item.pos.distance_squared(self.player.pos) > DRAW_DIST * DRAW_DIST {
                continue;
            }
            let block = item.stack.item.block().filter(|b| !b.flat_icon());
            let icon = match block {
                Some(_) => None,
                None => item.stack.item.block().map(|b| b.info().tex[0].into()).or(item.stack.item.icon_layer()),
            };
            let size = if block.is_some() { 0.25 } else { 0.5 };
            let spin = item.age + item.phase;
            let bob = 0.1 + 0.07 * (item.age * 2.0 + item.phase).sin() as f64;
            let sky_light = crate::entity::sky_light(&self.world, item.pos + DVec3::Y * 0.25);
            let block_light = self.torch_light(item.pos + DVec3::Y * 0.25);
            let copies = match item.stack.count {
                1 => 1,
                2..=16 => 2,
                17..=32 => 3,
                _ => 4,
            };
            for i in 0..copies {
                // Copies sit a little apart, the same way every frame.
                let k = i as f64;
                let offset = DVec3::new((k * 2.3).sin(), 0.0, (k * 1.7).cos()) * 0.06 * k.min(1.0);
                let min = item.previous_pos.lerp(item.pos, alpha)
                    + offset
                    + DVec3::new(-size / 2.0, bob + k * 0.03, -size / 2.0);
                out.push(BlockModel {
                    min,
                    size: size as f32,
                    block: block.unwrap_or(Block::AIR),
                    sky_light,
                    block_light,
                    yaw: spin,
                    icon,
                });
            }
        }
        out
    }
}

/// The orb pickup ding (Java's random pitch around 0.9) and, every five
/// levels, the level-up fanfare. `at` is `None` for the local player.
pub(super) fn xp_sounds(audio: &mut Audio, at: Option<DVec3>, chime: Option<f32>) {
    audio.play(Sound::Orb, at, 0.3, (0.55, 1.25));
    if let Some(volume) = chime {
        audio.play(Sound::LevelUp, at, volume, (1.0, 1.0));
    }
}

/// Java's eye launch whoosh, pitched down like a thrown pearl.
pub(super) fn eye_thrown_sound(audio: &mut Audio, from: DVec3) {
    audio.play(Sound::Bow, Some(from), 0.5, (0.42, 0.62));
}

/// An eye settling into the frame at `pos`, and the portal opening.
pub(super) fn frame_filled_sounds(audio: &mut Audio, pos: glam::IVec3, opened: bool) {
    audio.play(Sound::FrameFill, Some(pos.as_dvec3() + DVec3::splat(0.5)), 1.0, (0.9, 1.1));
    if opened {
        // Java plays this to everyone in the world, wherever they are.
        audio.play(Sound::PortalSpawn, None, 1.0, (1.0, 1.0));
    }
}
