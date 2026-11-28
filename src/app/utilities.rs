//! Client presentation of the v0.12 utility block uses (pots, cauldrons,
//! campfires, respawn anchors), and the host's follow-up for anchors.

use glam::{DVec3, IVec3};

use super::Game;
use crate::audio::{
    Audio, SoundSource,
    sounds::{Material, Sound},
};
use voxelcraft::entity::decor;
use voxelcraft::world::utilities::interact::Effect;

/// The sound of a utility block use, in Java's `BLOCKS` source.
pub(super) fn feedback(audio: &mut Audio, cell: IVec3, effect: Effect) {
    let at = Some(cell.as_dvec3() + DVec3::splat(0.5));
    let (sound, gain, pitch) = match effect {
        // `dyedItemIteration` plays no sound.
        Effect::Consumed | Effect::AnchorExploded | Effect::Washed => return,
        Effect::PotPlanted | Effect::PotEmptied => (Sound::Place(Material::Grass), 0.8, (0.9, 1.1)),
        Effect::PotFilled => (Sound::Place(Material::Stone), 0.8, (1.2, 1.4)),
        Effect::Cauldron { fill, lava, snow, bottle } => {
            let sound = if bottle {
                Sound::Swim
            } else if lava {
                Sound::Place(Material::Stone)
            } else if snow {
                Sound::Place(Material::Snow)
            } else if fill {
                Sound::Splash
            } else {
                Sound::Swim
            };
            (sound, 0.7, if bottle { (1.3, 1.5) } else { (0.85, 1.0) })
        }
        Effect::FoodPlaced => (Sound::Place(Material::Wood), 0.6, (0.9, 1.1)),
        Effect::Lit => (Sound::Place(Material::Stone), 0.7, (1.6, 1.9)),
        Effect::Doused => (Sound::Fuse, 0.5, (2.0, 2.4)),
        Effect::AnchorCharged => (Sound::Place(Material::Stone), 1.0, (0.55, 0.7)),
        Effect::AnchorSpawn => (Sound::Teleport, 0.6, (0.6, 0.7)),
        Effect::EnderChest => (Sound::Place(Material::Stone), 0.5, (0.9, 1.0)),
    };
    audio.play_in(SoundSource::Blocks, sound, at, gain, pitch);
}

/// Sounds of placing, using and breaking decorations (Java plays these
/// in `BLOCKS` for hanging entities and `NEUTRAL` for armor stands).
pub(super) fn decor_feedback(audio: &mut Audio, at: DVec3, effect: decor::Effect) {
    use decor::Effect as E;
    let (sound, gain, pitch) = match effect {
        E::Refused => return,
        E::Placed(_) => (Sound::Place(Material::Wood), 0.8, (0.9, 1.0)),
        E::ItemAdded => (Sound::Place(Material::Wood), 0.6, (1.3, 1.5)),
        E::ItemRotated => (Sound::Click, 0.7, (1.1, 1.3)),
        E::ItemRemoved => (Sound::Pop, 0.7, (0.9, 1.1)),
        E::Broken(_) => (Sound::Break(Material::Wood), 0.9, (0.8, 1.0)),
        E::Wobbled => (Sound::Hit, 0.7, (1.1, 1.3)),
        E::Equipped => (Sound::Place(Material::Stone), 0.6, (1.2, 1.4)),
    };
    audio.play_in(SoundSource::Blocks, sound, Some(at), gain, pitch);
}

impl Game {
    /// Items in item frames, as free-standing block models: a small block,
    /// or the item's icon flat in the frame. Only frames `view` draws from
    /// `camera` show their item (Java draws it with the frame).
    pub(super) fn decor_models<'a>(
        &'a self,
        camera: DVec3,
        view: &'a voxelcraft::entity::draw_distance::DrawDistance,
    ) -> impl Iterator<Item = crate::render::BlockModel> + 'a {
        // A filled map whose data exists is drawn as its image instead
        // (`entity::decor`), as Java's renderer does.
        let maps = &self.world.maps;
        let shows_map = move |s: &voxelcraft::inventory::Stack| {
            voxelcraft::world::maps::has_map_id(s) && s.map_id.is_some_and(|id| maps.maps.contains_key(&id))
        };
        decor::framed_items(&self.mobs.entities.decor, camera, view).filter(move |f| !shows_map(&f.2)).map(
            |(centre, orient, stack, sky, torch)| {
                let block = crate::item::model_block(&stack);
                // v1.0 gap track 6b: a 3D item mesh, at Java's frame scale
                // (0.5) and `fixed` transform; the frame's turn faces model
                // +Z out of the wall where Java's faces -Z, so it turns once
                // more.
                if voxelcraft::content::item_models::has_mesh(stack.item) {
                    return crate::render::BlockModel {
                        patterns: None,
                        lid: None,
                        min: centre,
                        size: 0.5,
                        block: voxelcraft::world::block::Block::AIR,
                        sky_light: sky,
                        block_light: torch,
                        yaw: 0.0,
                        icon: None,
                        orient: Some(orient * glam::Quat::from_rotation_y(std::f32::consts::PI)),
                        tint: [255; 4],
                        glint: stack.has_foil(),
                        translucent: false,
                        mesh: Some((stack, voxelcraft::content::item_models::Context::Fixed)),
                        transform: None,
                    };
                }
                let size = if block.is_some() { 0.375 } else { 0.5 };
                let icon = match block {
                    Some(_) => None,
                    None => stack
                        .item
                        .dial_layer(self.dial_of(&self.session.player).for_stack(&stack))
                        .or_else(|| stack.item.flat_layer()),
                };
                // v1.0 gap track 6c: fences, walls and anvils turn a quarter
                // in a frame (their `fixed` transforms' 90 degrees about y), so
                // a fence, which runs north to south as Java's
                // `fence_inventory` does, lies across the frame.
                let fixed = voxelcraft::content::item_display::of(&stack).fixed.rotation;
                let orient = if block.is_some() && fixed[0] == 0.0 && fixed[2] == 0.0 && fixed[1] % 180.0 == 90.0 {
                    orient * glam::Quat::from_rotation_y(fixed[1].to_radians())
                } else {
                    orient
                };
                // A block sits on the frame's backing rather than in the wall.
                let out =
                    if block.is_some() { (orient * glam::Vec3::Z).as_dvec3() * (size / 2.0) } else { DVec3::ZERO };
                crate::render::BlockModel {
                    patterns: None,
                    lid: None,
                    min: centre + out - DVec3::splat(size / 2.0),
                    size: size as f32,
                    block: block.unwrap_or(voxelcraft::world::block::Block::AIR),
                    sky_light: sky,
                    block_light: torch,
                    yaw: 0.0,
                    icon,
                    orient: Some(orient),
                    tint: voxelcraft::entity::wolf_armor::item_tint_bytes(stack),
                    glint: stack.has_foil(),
                    translucent: crate::item::translucent(&stack),
                    mesh: None,
                    transform: None,
                }
            },
        )
    }

    /// `--decor`: places a decoration as its item would, then fills a
    /// frame or dresses a stand with `held`.
    pub(super) fn place_scripted_decor(
        &mut self,
        pos: IVec3,
        face: IVec3,
        item: voxelcraft::item::Item,
        held: Option<voxelcraft::item::Item>,
    ) {
        let yaw = voxelcraft::knockback::java_yaw_degrees(self.session.player.yaw);
        let entities = &mut self.mobs.entities;
        if !matches!(entities.place_decor(&self.world, item, pos, face, yaw, &[]), Some(Some(_))) {
            log::warn!("--decor {pos}: {} doesn't fit there", item.name());
            return;
        }
        let (Some(d), Some(held)) = (entities.decor.last_mut(), held) else { return };
        let stack = voxelcraft::inventory::Stack::new(held, 1);
        match decor::stand_slot(held) {
            Some(piece) if d.kind == decor::Kind::Stand => d.armor[piece as usize] = Some(stack),
            _ if matches!(d.kind, decor::Kind::Frame { .. }) => d.item = Some(stack),
            _ => log::warn!("--decor {pos}: a {} can't hold {}", item.name(), held.name()),
        }
    }

    /// Respawn anchors any player used outside the Nether explode like a bed
    /// (`RespawnAnchorBlock.explode`, power 5).
    pub(super) fn explode_anchors(&mut self) {
        for cell in self.world.take_anchor_blasts() {
            self.explode(
                cell.as_dvec3() + DVec3::splat(0.5),
                5.0,
                voxelcraft::simulation::damage::Cause::BAD_RESPAWN_POINT,
                false,
            );
        }
    }

    /// The host's part of a utility use.
    pub(super) fn present_utility(&mut self, cell: IVec3, effect: Effect) {
        match effect {
            Effect::EnderChest => self.open_ender_chest(cell),
            Effect::AnchorSpawn => self.show_popup("Respawn point set"),
            _ => {}
        }
    }

    /// The ender chest at `pos` shows this player's ender inventory, lent
    /// to the world's chest table while the screen is open.
    fn open_ender_chest(&mut self, pos: IVec3) {
        if self.inventory_open {
            return;
        }
        self.world.lend_chest(pos, self.session.ender);
        self.open_chest(pos);
    }

    /// Closing a chest screen: an ender chest's contents go back to the player.
    pub(super) fn close_ender_chest(&mut self, pos: IVec3) {
        if let Some(contents) = self.world.take_lent_chest(pos) {
            self.session.ender = contents;
        }
    }

    /// The ender inventory as it stands, including an open ender chest's.
    pub(super) fn ender_for_save(&self) -> voxelcraft::world::chest::Chest {
        match self.body.container {
            super::Container::Chest(pos) if self.inventory_open && self.world.lent_chest() == Some(pos) => {
                self.world.chest(pos).copied().unwrap_or(self.session.ender)
            }
            _ => self.session.ender,
        }
    }
}
