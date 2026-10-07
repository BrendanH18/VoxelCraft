//! Append-only colour states (560..=666). Legacy white wool, red beds and
//! badlands terracotta keep their original IDs.
use super::block::{Block, RenderKind, tex};
use crate::color::DyeColor;

impl Block {
    pub const GLASS_PANE: Self = Self(640);
    pub const fn wool(c: DyeColor) -> Self {
        if matches!(c, DyeColor::White) { Self::WOOL } else { Self(560 + c as u16) }
    }
    pub const fn carpet(c: DyeColor) -> Self {
        Self(576 + c as u16)
    }
    pub const fn colored_bed(c: DyeColor, head: bool) -> Self {
        if matches!(c, DyeColor::Red) {
            if head { Self::BED_HEAD } else { Self::BED_FOOT }
        } else {
            Self(592 + c as u16 * 2 + head as u16)
        }
    }
    pub const fn stained_glass(c: DyeColor) -> Self {
        Self(624 + c as u16)
    }
    pub const fn stained_pane(c: DyeColor) -> Self {
        Self(641 + c as u16)
    }
    /// Reuses badlands IDs 85..=90 for the six colours already in the world.
    pub const fn stained_terracotta(c: DyeColor) -> Self {
        const IDS: [u16; 16] = [89, 85, 657, 658, 86, 659, 660, 661, 90, 662, 663, 664, 88, 665, 87, 666];
        Self(IDS[c as usize])
    }
    pub fn wool_color(self) -> Option<DyeColor> {
        if self == Self::WOOL {
            Some(DyeColor::White)
        } else {
            self.0.checked_sub(560).filter(|&i| (1..16).contains(&i)).map(|i| DyeColor::ALL[i as usize])
        }
    }
    pub fn carpet_color(self) -> Option<DyeColor> {
        self.0.checked_sub(576).filter(|&i| i < 16).map(|i| DyeColor::ALL[i as usize])
    }
    pub fn bed_color(self) -> Option<DyeColor> {
        if matches!(self, Self::BED_FOOT | Self::BED_HEAD) {
            Some(DyeColor::Red)
        } else {
            self.0.checked_sub(592).filter(|&i| i < 32 && i / 2 != 14).map(|i| DyeColor::ALL[i as usize / 2])
        }
    }
    pub fn is_bed_head(self) -> bool {
        self == Self::BED_HEAD || self.bed_color().is_some() && self.0 % 2 == 1
    }
    pub fn stained_glass_color(self) -> Option<DyeColor> {
        self.0.checked_sub(624).filter(|&i| i < 16).map(|i| DyeColor::ALL[i as usize])
    }
    pub fn stained_pane_color(self) -> Option<DyeColor> {
        self.0.checked_sub(641).filter(|&i| i < 16).map(|i| DyeColor::ALL[i as usize])
    }
    pub fn is_glass_pane(self) -> bool {
        (640..=656).contains(&self.0)
    }
    pub fn stained_terracotta_color(self) -> Option<DyeColor> {
        DyeColor::ALL.into_iter().find(|&c| Self::stained_terracotta(c) == self)
    }
}

pub(super) fn palette_ids() -> impl Iterator<Item = u16> {
    // Wool (white stays Block::WOOL) and carpets. Beds are items, not block halves.
    // 624..=666: stained glass, panes, remaining terracotta.
    (561..592).chain(624..667)
}

pub(super) const fn definition(id: u16) -> Option<(&'static str, RenderKind, [u16; 6])> {
    const WOOL: [&str; 16] = [
        "white wool",
        "orange wool",
        "magenta wool",
        "light blue wool",
        "yellow wool",
        "lime wool",
        "pink wool",
        "gray wool",
        "light gray wool",
        "cyan wool",
        "purple wool",
        "blue wool",
        "brown wool",
        "green wool",
        "red wool",
        "black wool",
    ];
    const CARPET: [&str; 16] = [
        "white carpet",
        "orange carpet",
        "magenta carpet",
        "light blue carpet",
        "yellow carpet",
        "lime carpet",
        "pink carpet",
        "gray carpet",
        "light gray carpet",
        "cyan carpet",
        "purple carpet",
        "blue carpet",
        "brown carpet",
        "green carpet",
        "red carpet",
        "black carpet",
    ];
    const BEDS: [[&str; 2]; 16] = [
        ["white bed foot", "white bed head"],
        ["orange bed foot", "orange bed head"],
        ["magenta bed foot", "magenta bed head"],
        ["light blue bed foot", "light blue bed head"],
        ["yellow bed foot", "yellow bed head"],
        ["lime bed foot", "lime bed head"],
        ["pink bed foot", "pink bed head"],
        ["gray bed foot", "gray bed head"],
        ["light gray bed foot", "light gray bed head"],
        ["cyan bed foot", "cyan bed head"],
        ["purple bed foot", "purple bed head"],
        ["blue bed foot", "blue bed head"],
        ["brown bed foot", "brown bed head"],
        ["green bed foot", "green bed head"],
        ["red bed foot", "red bed head"],
        ["black bed foot", "black bed head"],
    ];
    const GLASS: [&str; 16] = [
        "white stained glass",
        "orange stained glass",
        "magenta stained glass",
        "light blue stained glass",
        "yellow stained glass",
        "lime stained glass",
        "pink stained glass",
        "gray stained glass",
        "light gray stained glass",
        "cyan stained glass",
        "purple stained glass",
        "blue stained glass",
        "brown stained glass",
        "green stained glass",
        "red stained glass",
        "black stained glass",
    ];
    const PANE: [&str; 16] = [
        "white stained glass pane",
        "orange stained glass pane",
        "magenta stained glass pane",
        "light blue stained glass pane",
        "yellow stained glass pane",
        "lime stained glass pane",
        "pink stained glass pane",
        "gray stained glass pane",
        "light gray stained glass pane",
        "cyan stained glass pane",
        "purple stained glass pane",
        "blue stained glass pane",
        "brown stained glass pane",
        "green stained glass pane",
        "red stained glass pane",
        "black stained glass pane",
    ];
    const EXTRA_TERRACOTTA: [&str; 10] = [
        "magenta terracotta",
        "light blue terracotta",
        "lime terracotta",
        "pink terracotta",
        "gray terracotta",
        "cyan terracotta",
        "purple terracotta",
        "blue terracotta",
        "green terracotta",
        "black terracotta",
    ];
    match id {
        561..=575 => Some((WOOL[(id - 560) as usize], RenderKind::Opaque, [tex::COLORED_WOOL + id - 560; 6])),
        576..=591 => Some((CARPET[(id - 576) as usize], RenderKind::Cutout, [tex::COLORED_WOOL + id - 576; 6])),
        592..=619 | 622..=623 => {
            let color = (id - 592) / 2;
            let head = (id - 592) % 2;
            let side = tex::COLORED_BED + color * 4 + 2 + head;
            let top = tex::COLORED_BED + color * 4 + head;
            Some((BEDS[color as usize][head as usize], RenderKind::Cutout, [side, side, top, tex::PLANKS, side, side]))
        }
        624..=639 => Some((GLASS[(id - 624) as usize], RenderKind::Translucent, [tex::STAINED_GLASS + id - 624; 6])),
        640 => Some(("glass pane", RenderKind::Shaped, [tex::GLASS; 6])),
        641..=656 => Some((PANE[(id - 641) as usize], RenderKind::Shaped, [tex::STAINED_GLASS + id - 641; 6])),
        657..=666 => {
            Some((EXTRA_TERRACOTTA[(id - 657) as usize], RenderKind::Opaque, [tex::STAINED_TERRACOTTA + id - 657; 6]))
        }
        _ => None,
    }
}

impl super::World {
    /// The adjacent matching half; legacy beds have no saved facing state.
    pub fn bed_partner(&self, pos: glam::IVec3, half: Block) -> Option<glam::IVec3> {
        let other = Block::colored_bed(half.bed_color()?, !half.is_bed_head());
        super::block::Facing::ALL.into_iter().map(|f| pos + f.offset()).find(|&p| self.get_block(p) == Some(other))
    }
    pub fn place_colored_bed(&mut self, at: glam::IVec3, direction: glam::IVec3, color: DyeColor) -> bool {
        if super::block::Facing::from_offset(direction).is_none() {
            return false;
        }
        let head = at + direction;
        let fits = |p| {
            self.get_block(p).is_some_and(|b| b.is_replaceable())
                && self.get_block(p - glam::IVec3::Y).is_some_and(|b| b.is_opaque())
        };
        if !fits(at) || !fits(head) {
            return false;
        }
        self.set_block(at, Block::colored_bed(color, false));
        self.set_block(head, Block::colored_bed(color, true));
        true
    }
    pub fn break_bed_partner(&mut self, pos: glam::IVec3, half: Block, drops: bool) {
        let Some(other) = self.bed_partner(pos, half) else { return };
        self.set_block(other, Block::AIR);
        if half.is_bed_head() && drops {
            self.spill_block(other, Block::colored_bed(half.bed_color().unwrap(), false));
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{item::Item, world::block::Facing};
    #[test]
    fn colored_wool_carpets_and_beds_keep_legacy_ids() {
        assert_eq!(Block::wool(DyeColor::White), Block::WOOL);
        assert_eq!(DyeColor::Red.bed(), Item::BED);
        for c in DyeColor::ALL {
            let wool = Block::wool(c);
            assert_eq!(wool.wool_color(), Some(c));
            assert_eq!(wool.hardness(), 0.8);
            assert_eq!(wool.fire_odds(), (30, 60));
            assert_eq!(Block::from_name(&format!("{}_wool", c.dye_name().trim_end_matches(" dye"))), Some(wool));
            let carpet = Block::carpet(c);
            assert_eq!(carpet.height(), 1.0 / 16.0);
            assert!(!carpet.can_stay_on(Block::AIR) && !carpet.can_stay_on(Block::WATER));
            assert!(carpet.can_stay_on(Block::STONE));
            let foot = Block::colored_bed(c, false);
            let head = Block::colored_bed(c, true);
            assert_eq!(foot.bed_color(), Some(c));
            assert!(head.is_bed_head() && !foot.is_bed_head());
            assert_eq!(foot.drop(), Some(c.bed()));
            assert_eq!(head.drop(), None);
            assert_eq!(foot.with_facing(Facing::East), foot);
            assert_eq!(Item::from_name(c.bed_name()), Some(c.bed()));
            assert_eq!(c.bed().max_stack(), 1);
            assert!(Item::creative_palette().any(|p| p == Item::from_block(wool)));
            assert!(Item::creative_palette().any(|p| p == Item::from_block(carpet)));
            assert!(Item::creative_palette().any(|p| p == c.bed()));
        }
        assert_eq!(Item::from_name("shears"), Some(Item::SHEARS));
        assert!(Item::creative_palette().any(|p| p == Item::SHEARS));
    }

    #[test]
    fn stained_glass_panes_and_terracotta_keep_badlands_ids() {
        use crate::world::block::RenderKind;
        assert_eq!(Block::stained_terracotta(DyeColor::Orange), Block::terracotta(1));
        assert_eq!(Block::stained_terracotta(DyeColor::White), Block(89));
        assert_eq!(Block::GLASS_PANE.shaped(), Some(crate::world::block::Shaped::Pane));
        assert!(Block::GLASS_PANE.is_solid() && Block::GLASS_PANE.hardness() == 0.3);
        assert_eq!(Item::from_name("glass pane"), Some(Item::from_block(Block::GLASS_PANE)));
        for c in DyeColor::ALL {
            let glass = Block::stained_glass(c);
            assert_eq!(glass.kind(), RenderKind::Translucent);
            assert!(glass.is_solid() && glass.info().self_cull);
            assert_eq!(glass.drop(), None);
            assert_eq!(glass.hardness(), 0.3);
            assert_eq!(Block::from_name(&format!("{} stained glass", c.adjective())), Some(glass));
            let pane = Block::stained_pane(c);
            assert!(pane.is_glass_pane() && pane.hardness() == 0.3);
            assert_eq!(pane.drop(), None);
            let terracotta = Block::stained_terracotta(c);
            assert_eq!(terracotta.stained_terracotta_color(), Some(c));
            assert_eq!(terracotta.hardness(), 1.25);
            assert_eq!(terracotta.harvest_level(), Some(0));
            assert_eq!(Block::from_name(&format!("{} terracotta", c.adjective())), Some(terracotta));
            for b in [glass, pane, terracotta, Block::GLASS_PANE] {
                assert!(Item::creative_palette().any(|p| p == Item::from_block(b)), "{}", b.name());
            }
        }
    }
}
