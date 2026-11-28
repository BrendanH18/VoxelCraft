//! Game-side glue for dropped items: turning world drops and inventory
//! spill into item entities, picking them up, throwing items (Q, clicking
//! off the inventory panel), dropping everything on death, and drawing them.

use glam::DVec3;

use crate::audio::sounds::Sound;
use crate::audio::{Audio, SoundSource};
use crate::render::BlockModel;
use crate::world::block::Block;

use super::{Container, Game};
use crate::entity::PlayerId;
use crate::inventory::Stack;

/// Dropped items farther away than this aren't drawn.
const DRAW_DIST: f64 = 64.0;

impl Game {
    /// The items and experience the world dropped become entities.
    pub(super) fn spawn_world_drops(&mut self) {
        for (cell, stack) in std::mem::take(&mut self.world.drops) {
            self.mobs.entities.drop_from_block(stack, cell);
        }
        for (cell, xp) in std::mem::take(&mut self.world.xp_drops) {
            self.mobs.entities.spawn_xp(cell.as_dvec3() + DVec3::splat(0.5), xp);
        }
    }

    /// Spawns what the world dropped and what didn't fit in the inventory,
    /// then picks up items the player is touching. Closes a furnace or chest
    /// screen whose block is gone.
    pub(super) fn update_items(&mut self) {
        self.spawn_world_drops();
        for cell in std::mem::take(&mut self.world.brews_done) {
            // Java's brewing stand bubbling as a brew finishes.
            let at = cell.as_dvec3() + DVec3::splat(0.5);
            // The brewing stand's bubbling is `BLOCKS` in Java.
            self.audio.play_in(SoundSource::Blocks, crate::audio::sounds::Sound::Swim, Some(at), 0.5, (1.6, 1.9));
        }
        for stack in self.session.inventory.take_spill() {
            self.mobs.entities.throw(stack, self.session.player.eye(), self.session.player.forward().as_dvec3());
        }
        if !self.session.vitals.is_dead() {
            self.pick_up_items();
        }
        let session = &self.session;
        let far = |pos: glam::IVec3| !session.can_keep_using_block(pos);
        let gone = match self.body.container {
            Container::Furnace(pos) => self.world.furnace(pos).is_none() || far(pos),
            // A lent ender inventory whose ender chest was mined closes too.
            Container::Chest(pos) => {
                self.world.chest(pos).is_none()
                    || far(pos)
                    || (self.world.lent_chest() == Some(pos)
                        && !self
                            .world
                            .get_block(pos)
                            .is_some_and(|b| voxelcraft::world::utilities::ender_chest_of(b).is_some()))
            }
            Container::Minecart(id) => {
                !self.mobs.entities.vehicle_in_reach(id, session.player.eye(), session.entity_screen_range())
            }
            Container::Brewing(pos) => self.world.brewing_stand(pos).is_none() || far(pos),
            Container::Enchanting(pos) => {
                self.world.get_block(pos) != Some(crate::world::block::Block::ENCHANTING_TABLE) || far(pos)
            }
            // An anvil can break in use, or fall away.
            Container::Anvil(pos) => !self.world.get_block(pos).is_some_and(|b| b.is_anvil()) || far(pos),
            Container::Grindstone(pos) => {
                self.world.get_block(pos).is_none_or(|b| b.base() != Block::GRINDSTONE) || far(pos)
            }
            Container::Trading(id) => {
                !self.mobs.entities.merchant_in_reach(id, session.player.eye(), session.entity_screen_range())
            }
            Container::Smithing(pos) => self.world.get_block(pos) != Some(Block::SMITHING_TABLE) || far(pos),
            Container::Stonecutter(pos)
            | Container::Loom(pos)
            | Container::Cartography(pos)
            | Container::Beacon(pos) => self.workstation_gone(pos, far),
            _ => false,
        };
        if gone && self.inventory_open {
            self.toggle_inventory();
        }
    }

    fn pick_up_items(&mut self) {
        let player = self.session.player.pos;
        let mut picked = self.mobs.entities.pick_up_items(player, &mut self.session.inventory);
        let (id, shape) = (self.session.id, self.session.player.collision_shape());
        let creative = self.session.mode.is_creative();
        let inventory = &mut self.session.inventory;
        picked |=
            self.mobs.entities.collect_tridents(id, player, shape, creative, |stack| inventory.add_stack(stack) == 0);
        for stack in self.mobs.entities.collect_arrow_stacks(player) {
            picked = true;
            let left = self.session.inventory.add_stack(stack);
            if left > 0 {
                let mut stack = stack;
                stack.count = left;
                self.mobs.entities.scatter(stack, player);
            }
        }
        if picked {
            self.audio.play(Sound::Pop, None, 0.35, (0.8, 1.8));
        }
        if let Some(chime) = crate::entity::orb::absorb(
            &mut self.mobs.entities.orbs,
            player,
            &mut self.session.vitals.xp,
            &mut self.session.inventory,
            self.body.actions.selected,
        ) {
            xp_sounds(&mut self.audio, None, chime);
        }
    }

    /// A pearl landed at `pos`: its thrower (if alive, in this world)
    /// teleports there and takes 5 damage, like a fall.
    pub(super) fn pearl_landed(&mut self, owner: PlayerId, pos: DVec3) {
        if owner == PlayerId::HOST {
            if self.session.vitals.is_dead() || self.session.sleeping.is_some() {
                return;
            }
            // `ThrownEnderpearl` plays the landing sound as `PLAYERS`.
            self.audio.play_in(
                SoundSource::Players,
                Sound::Teleport,
                Some(self.session.player.pos + DVec3::Y),
                0.8,
                (0.9, 1.1),
            );
            self.session.player.pos = pos;
            self.session.player.vel = DVec3::ZERO;
            self.session.vitals.reset_fall();
            self.previous_eye = self.session.player.eye();
            self.damage_player(crate::entity::pearl::DAMAGE, voxelcraft::simulation::damage::Cause::ENDER_PEARL);
        } else if let Some(bot) = self.agents.by_id_mut(owner)
            && bot.active
        {
            let from = bot.agent.session.player.pos;
            if bot.agent.pearl_teleport(pos, &mut self.mobs.entities) {
                self.audio.play_in(SoundSource::Players, Sound::Teleport, Some(from + DVec3::Y), 0.8, (0.9, 1.1));
            }
        }
        self.audio.play_in(SoundSource::Players, Sound::Teleport, Some(pos + DVec3::Y), 0.8, (0.9, 1.1));
    }

    /// Q: drops one of the selected item, or the whole stack with Ctrl.
    /// With the inventory open, drops from the slot under the mouse.
    pub(super) fn drop_selected(&mut self, whole_stack: bool) {
        if self.lan.client.is_some() {
            self.lan_command(if whole_stack { "drop_stack" } else { "drop" });
            return;
        }
        let slot = if self.inventory_open {
            match self.slot_under_cursor() {
                Some(super::hud::SlotRef::Inventory(i)) => i,
                // v0.18 Track 4: `THROW` on a creative cell drops one, or a
                // full stack with the modifier, without touching the cell.
                Some(super::hud::SlotRef::Palette(s)) => {
                    let stack = Stack { count: if whole_stack { s.max() } else { 1 }, ..s };
                    let (eye, dir) = (self.session.player.eye(), self.session.player.forward().as_dvec3());
                    self.mobs.entities.throw(stack, eye, dir);
                    return;
                }
                // v0.17 Track D2: over a storage slot the drop key throws
                // from it, with nothing held (`THROW`).
                Some(s) => {
                    if let Some(slot) = super::screen::storage_slot(s) {
                        self.with_screen(|screen| {
                            if let Some(mut storage) = screen.storage() {
                                storage.throw(slot, whole_stack);
                            }
                        });
                    }
                    return;
                }
                None => return,
            }
        } else {
            self.body.actions.selected
        };
        if voxelcraft::session::storage::drop_slot(&mut self.session, &mut self.mobs.entities, slot, whole_stack)
            .is_some()
            && slot == self.body.actions.selected
        {
            self.body.actions.reset();
            self.session.cancel_consumption();
        }
    }

    /// v0.17 Track D2: a number key over a slot of an open storage screen
    /// swaps that slot with hotbar slot `hotbar` (`SWAP`). Returns whether
    /// a slot was under the mouse.
    pub(super) fn swap_under_cursor(&mut self, hotbar: usize) -> bool {
        // v0.18 Track 4: `SWAP` on a creative cell puts a full stack of it
        // in that hotbar slot; an empty cell does nothing.
        match self.slot_under_cursor() {
            Some(super::hud::SlotRef::Palette(s)) => {
                self.session.inventory.slots[hotbar] = Some(Stack { count: s.max(), ..s });
                return true;
            }
            Some(super::hud::SlotRef::PaletteEmpty | super::hud::SlotRef::Destroy) => return true,
            _ => {}
        }
        let Some(slot) = self.slot_under_cursor().and_then(super::screen::storage_slot) else { return false };
        self.with_screen(|screen| match screen.storage() {
            Some(mut storage) => storage.swap(slot, hotbar),
            None => {
                if let voxelcraft::session::storage::StorageSlot::Inventory(i) = slot {
                    voxelcraft::session::storage::swap_in_inventory(&mut screen.session.inventory, i, hotbar);
                }
            }
        });
        true
    }

    /// Clicking off the inventory panel throws the held stack (or one of
    /// it with the right button). Returns whether the click was used.
    pub(super) fn throw_cursor(&mut self, one: bool) -> bool {
        if !self.cursor_off_panel() {
            return false;
        }
        self.toss_cursor(one)
    }

    /// Throws the held stack (or one item of it), as a click outside the screen does.
    pub(super) fn toss_cursor(&mut self, one: bool) -> bool {
        voxelcraft::session::storage::toss_cursor(&mut self.session, &mut self.mobs.entities, one)
    }

    /// Spinning, bobbing models for nearby dropped items: a small cube for
    /// blocks, a flat icon for everything else, with extra copies for
    /// bigger stacks.
    pub(super) fn item_models(&self, alpha: f64) -> Vec<BlockModel> {
        let mut out = Vec::new();
        for item in &self.mobs.entities.items {
            if item.pos.distance_squared(self.session.player.pos) > DRAW_DIST * DRAW_DIST {
                continue;
            }
            let pos = item.previous_pos.lerp(item.pos, alpha);
            // `ItemEntityRenderer`: `sin(ageInTicks / 10 + bobOffs) * 0.1 + 0.1`.
            let bob = 0.1 + 0.1 * (item.age * 2.0 + item.phase).sin() as f64;
            self.dropped_models(item.stack, pos, item.age + item.phase, bob, &mut out);
        }
        // v1.0 gap track 6: `--item-review`'s staged item, at rest.
        if let Some((stack, pos, spin)) = self.item_review_drop() {
            self.dropped_models(stack, pos, spin, 0.0, &mut out);
        }
        out
    }

    /// The models of a dropped `stack` lying at `pos`, turned `spin` and
    /// raised `bob` (with copies for bigger stacks). As Java's
    /// `ItemEntityRenderer` places them, the model's lowest point (its
    /// bounding box's) hovers 1/16 above the item's feet before the bob.
    fn dropped_models(&self, stack: Stack, pos: DVec3, spin: f32, bob: f64, out: &mut Vec<BlockModel>) {
        // v1.0 gap track 6b: beds, banners, the shield and two plants
        // drawn with their 3D model's ground transform.
        let mesh = voxelcraft::content::item_models::mesh(&stack)
            .map(|m| m.lowest(voxelcraft::content::item_models::Context::Ground) as f64);
        let block = crate::item::model_block(&stack);
        let icon = match block {
            Some(_) => None,
            None => stack
                .item
                .dial_layer(self.dial_of(&self.session.player).for_stack(&stack))
                .or_else(|| stack.item.flat_layer()),
        };
        let size = block.map_or(0.5, |b| 0.25 * crate::world::shape::item_scale(b).ground as f64);
        // `ITEM_MIN_HOVER_HEIGHT` less the model's own lowest point (a
        // flat icon's quad starts at its bottom edge).
        let lowest = block.map_or(0.0, |b| {
            crate::world::shape::item_boxes(b).as_slice().iter().map(|bx| bx.min[1]).min().unwrap_or(0) as f64 / 16.0
        });
        let lift = HOVER - lowest * size;
        let sky_light = crate::entity::sky_light(&self.world, pos + DVec3::Y * 0.25);
        let block_light = self.torch_light(pos + DVec3::Y * 0.25);
        let copies = match stack.count {
            1 => 1,
            2..=16 => 2,
            17..=32 => 3,
            _ => 4,
        };
        for i in 0..copies {
            // Copies sit a little apart, the same way every frame.
            let k = i as f64;
            let offset = DVec3::new((k * 2.3).sin(), 0.0, (k * 1.7).cos()) * 0.06 * k.min(1.0);
            let mut min = pos + offset + DVec3::new(-size / 2.0, lift + bob + k * 0.03, -size / 2.0);
            if let Some(lowest) = mesh {
                // The mesh's origin, its lowest point hovering as a block's does.
                min = pos + offset + DVec3::new(0.0, HOVER - lowest + bob + k * 0.03, 0.0);
            }
            out.push(BlockModel {
                patterns: None,
                lid: None,
                min,
                size: if mesh.is_some() { 1.0 } else { size as f32 },
                block: block.unwrap_or(Block::AIR),
                sky_light,
                block_light,
                yaw: spin,
                icon,
                orient: None,
                tint: voxelcraft::entity::wolf_armor::item_tint_bytes(stack),
                glint: stack.has_foil(),
                translucent: crate::item::translucent(&stack),
                mesh: mesh.map(|_| (stack, voxelcraft::content::item_models::Context::Ground)),
            });
        }
    }
}

/// `ItemEntityRenderer.ITEM_MIN_HOVER_HEIGHT`: how far a dropped item's
/// model floats above its feet at the bottom of its bob.
const HOVER: f64 = 1.0 / 16.0;

/// The orb pickup ding (Java's random pitch around 0.9) and, every five
/// levels, the level-up fanfare. `at` is `None` for the local player.
pub(super) fn xp_sounds(audio: &mut Audio, at: Option<DVec3>, chime: Option<f32>) {
    audio.play(Sound::Orb, at, 0.3, (0.55, 1.25));
    if let Some(volume) = chime {
        audio.play(Sound::LevelUp, at, volume, (1.0, 1.0));
    }
}

/// A bow or crossbow being loaded or loosed (shared by every input path).
pub(super) fn shot_sound(audio: &mut Audio, ranged: voxelcraft::session::ranged::Ranged) {
    use voxelcraft::session::ranged::Ranged;
    match ranged {
        Ranged::Drawing => {}
        Ranged::CrossbowLoaded { from } => audio.play(Sound::Bow, Some(from), 0.5, (0.55, 0.65)),
        Ranged::CrossbowFired { from } => audio.play(Sound::Bow, Some(from), 0.9, (1.25, 1.35)),
        Ranged::ArrowShot { from, power } => {
            let pitch = (1.0 + 0.2 * (1.0 - power), 1.1 + 0.2 * (1.0 - power));
            audio.play(Sound::Bow, Some(from), 0.8, pitch);
        }
        Ranged::Charging { from, cue: Some(cue) } | Ranged::WeaponCue { from, cue: Some(cue) } => {
            weapon_sound(audio, cue, from);
        }
        Ranged::Charging { cue: None, .. } | Ranged::WeaponCue { cue: None, .. } => {}
    }
}

/// v1.0 gap track 6: one of the bundle sounds at Java's volume and pitch.
pub(super) fn bundle_sound(audio: &mut Audio, cue: voxelcraft::content::bundle::Cue, at: Option<DVec3>) {
    let (volume, pitch) = cue.volume_pitch();
    audio.play(Sound::Bundle(cue), at, volume, pitch);
}

/// A v0.16 weapon sound at `at`, at Java's volume 1 and pitch 1.
pub(super) fn weapon_sound(audio: &mut Audio, cue: voxelcraft::weapons::Cue, at: DVec3) {
    audio.play(Sound::Weapon(cue), Some(at), 1.0, (0.95, 1.05));
}

/// Java's eye launch whoosh, pitched down like a thrown pearl.
pub(super) fn eye_thrown_sound(audio: &mut Audio, from: DVec3) {
    audio.play_in(SoundSource::Neutral, Sound::Bow, Some(from), 0.5, (0.42, 0.62));
}

/// An eye settling into the frame at `pos`, and the portal opening.
pub(super) fn frame_filled_sounds(audio: &mut Audio, pos: glam::IVec3, opened: bool) {
    audio.play(Sound::FrameFill, Some(pos.as_dvec3() + DVec3::splat(0.5)), 1.0, (0.9, 1.1));
    if opened {
        // Java plays this to everyone in the world, wherever they are.
        audio.play(Sound::PortalSpawn, None, 1.0, (1.0, 1.0));
    }
}
