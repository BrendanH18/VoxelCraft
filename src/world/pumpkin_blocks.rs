//! Append-only carved pumpkins and jack o'lanterns, 953..=960. Each has four facings; the
//! face texture sits on the facing side, the rest is the pumpkin's ribbed skin.
use super::block::{Block, Facing, RenderKind, tex};

pub const CARVED: Block = Block(953);
pub const JACK_O_LANTERN: Block = Block(957);

pub fn oriented(id: u16) -> Option<(Block, Facing)> {
    (953..=960).contains(&id).then(|| (Block(953 + (id - 953) / 4 * 4), Facing::ALL[((id - 953) % 4) as usize]))
}

/// A carved pumpkin or jack o'lantern in any facing: what a golem builds from.
pub const fn is_head(b: Block) -> bool {
    matches!(b.0, 953..=960)
}

pub fn palette_ids() -> impl Iterator<Item = u16> {
    [CARVED.0, JACK_O_LANTERN.0].into_iter()
}

pub const fn registry(id: u16) -> Option<(&'static str, RenderKind, [u16; 6])> {
    if id < 953 || id > 960 {
        return None;
    }
    let jack = id >= 957;
    let mut t = [
        tex::PUMPKIN_SIDE,
        tex::PUMPKIN_SIDE,
        tex::PUMPKIN_TOP,
        tex::PUMPKIN_TOP,
        tex::PUMPKIN_SIDE,
        tex::PUMPKIN_SIDE,
    ];
    t[Facing::ALL[((id - 953) % 4) as usize].face()] = if jack { tex::JACK_O_LANTERN } else { tex::CARVED_PUMPKIN };
    Some((if jack { "jack o'lantern" } else { "carved pumpkin" }, RenderKind::Opaque, t))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::item::Item;

    #[test]
    fn both_heads_register_turn_drop_and_glow() {
        for (base, glows) in [(CARVED, false), (JACK_O_LANTERN, true)] {
            for f in Facing::ALL {
                let b = base.with_facing(f);
                assert_eq!(b.oriented(), Some((base, f)));
                assert_eq!(b.base(), base);
                assert!(is_head(b));
                assert_eq!(b.drop(), Some(Item::from_block(base)));
                assert_eq!(b.emission(), if glows { 15 } else { 0 });
                assert!(b.info().tex.iter().all(|&t| u32::from(t) < tex::COUNT));
                assert_eq!(b.info().tex[f.face()], if glows { tex::JACK_O_LANTERN } else { tex::CARVED_PUMPKIN });
                assert_eq!(b.best_tool(), Some(crate::item::ToolKind::Axe));
            }
        }
        assert_eq!(Block::from_name("carved pumpkin"), Some(CARVED));
        assert_eq!(Block::from_name("jack o'lantern"), Some(JACK_O_LANTERN));
        assert!(!is_head(Block::PUMPKIN));
    }
}
