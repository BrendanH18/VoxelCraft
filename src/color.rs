//! Java dye IDs and textureDiffuseColor, shared by items, blocks and sheep.
use crate::item::Item;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum DyeColor {
    #[default]
    White = 0,
    Orange = 1,
    Magenta = 2,
    LightBlue = 3,
    Yellow = 4,
    Lime = 5,
    Pink = 6,
    Gray = 7,
    LightGray = 8,
    Cyan = 9,
    Purple = 10,
    Blue = 11,
    Brown = 12,
    Green = 13,
    Red = 14,
    Black = 15,
}

impl DyeColor {
    pub const ALL: [Self; 16] = [
        Self::White,
        Self::Orange,
        Self::Magenta,
        Self::LightBlue,
        Self::Yellow,
        Self::Lime,
        Self::Pink,
        Self::Gray,
        Self::LightGray,
        Self::Cyan,
        Self::Purple,
        Self::Blue,
        Self::Brown,
        Self::Green,
        Self::Red,
        Self::Black,
    ];
    pub const fn rgb(self) -> [u8; 3] {
        const COLORS: [u32; 16] = [
            0xF9FFFE, 0xF9801D, 0xC74EBD, 0x3AB3DA, 0xFED83D, 0x80C71F, 0xF38BAA, 0x474F52, 0x9D9D97, 0x169C9C,
            0x8932B8, 0x3C44AA, 0x835432, 0x5E7C16, 0xB02E26, 0x1D1D21,
        ];
        let c = COLORS[self as usize];
        [(c >> 16) as u8, (c >> 8) as u8, c as u8]
    }
    pub const fn dye(self) -> Item {
        Item(576 + self as u16)
    }
    pub const fn dye_name(self) -> &'static str {
        const NAMES: [&str; 16] = [
            "white dye",
            "orange dye",
            "magenta dye",
            "light blue dye",
            "yellow dye",
            "lime dye",
            "pink dye",
            "gray dye",
            "light gray dye",
            "cyan dye",
            "purple dye",
            "blue dye",
            "brown dye",
            "green dye",
            "red dye",
            "black dye",
        ];
        NAMES[self as usize]
    }
    /// Java Sheep.createSheepColor: white is 230, others diffuse * 0.75.
    pub fn sheep_rgb(self) -> [u8; 3] {
        if self == Self::White { [230; 3] } else { self.rgb().map(|v| (v as f32 * 0.75) as u8) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn java_diffuse_colors_and_item_roundtrips() {
        assert_eq!(DyeColor::White.rgb(), [249, 255, 254]);
        assert_eq!(DyeColor::Red.rgb(), [176, 46, 38]);
        assert_eq!(DyeColor::Black.rgb(), [29, 29, 33]);
        for c in DyeColor::ALL {
            let i = c.dye();
            assert_eq!(i.dye_color(), Some(c));
            assert_eq!(Item::from_name(c.dye_name()), Some(i));
            assert!(Item::creative_palette().any(|p| p == i));
            assert!(i.icon_layer().is_some());
        }
    }
}
