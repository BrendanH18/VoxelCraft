//! Client interaction adapters and presentation around shared engine operations.
use super::*;
impl Game {
    pub(super) fn target(&self) -> Option<(glam::IVec3, glam::IVec3)> {
        self.session.block_target(&self.world)
    }

    /// Torch light at a point, 0..1, for lighting things drawn outside the
    /// chunk meshes.
    pub(super) fn torch_light(&self, p: glam::DVec3) -> f32 {
        self.world.block_light(p.floor().as_ivec3()) as f32 / 15.0
    }

    /// The item in the selected hotbar slot.
    pub(super) fn held_item(&self) -> Option<Item> {
        self.session.inventory.get(self.body.actions.selected).map(|s| s.item)
    }

    /// Popup and sound for a held tool that just broke.
    pub(super) fn tool_broke(&mut self, held: Item) {
        self.show_popup(&format!("{} broke", capitalize(held.name())));
        self.audio.play_in(
            crate::audio::SoundSource::Players,
            crate::audio::sounds::Sound::Break(crate::audio::sounds::Material::Wood),
            Some(self.session.player.eye()),
            0.8,
            (1.3, 1.5),
        );
    }
}
