//! Append-only redstone state registry. Connections and repeater locks are
//! derived from neighbours, avoiding redundant saved state and texture layers.
use super::block::{Block, Facing, RenderKind, tex};

pub const WIRE: Block = Block(1100);
pub const LEVER: Block = Block(1116);
pub const STONE_BUTTON: Block = Block(1128);
pub const WOOD_BUTTON: Block = Block(1140);
pub const TORCH: Block = Block(1152);
pub const REPEATER: Block = Block(1162);
pub const COMPARATOR: Block = Block(1194);
pub const LAMP: Block = Block(1210);
pub const REDSTONE_BLOCK: Block = Block(1212);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Component {
    Wire(u8),
    Lever { mount: u8, on: bool },
    Button { mount: u8, on: bool, wood: bool },
    Torch { mount: u8, lit: bool },
    Repeater { facing: Facing, delay: u8, on: bool },
    Comparator { facing: Facing, subtract: bool, on: bool },
    Lamp(bool),
    Source,
}

pub const fn wire(power: u8) -> Block {
    Block(1100 + power as u16)
}
pub const fn lever(mount: u8, on: bool) -> Block {
    Block(1116 + mount as u16 * 2 + on as u16)
}
pub const fn button(mount: u8, on: bool, wood: bool) -> Block {
    Block(1128 + wood as u16 * 12 + mount as u16 * 2 + on as u16)
}
pub const fn torch(mount: u8, lit: bool) -> Block {
    Block(1152 + mount as u16 * 2 + !lit as u16)
}
pub const fn repeater(facing: Facing, delay: u8, on: bool) -> Block {
    Block(1162 + on as u16 * 16 + (delay as u16 - 1) * 4 + facing as u16)
}
pub const fn comparator(facing: Facing, subtract: bool, on: bool) -> Block {
    Block(1194 + on as u16 * 8 + subtract as u16 * 4 + facing as u16)
}
pub const fn component(b: Block) -> Option<Component> {
    let id = b.0;
    Some(match id {
        1100..=1115 => Component::Wire((id - 1100) as u8),
        1116..=1127 => Component::Lever { mount: ((id - 1116) / 2) as u8, on: id % 2 == 1 },
        1128..=1151 => Component::Button { mount: ((id - 1128) % 12 / 2) as u8, on: id % 2 == 1, wood: id >= 1140 },
        1152..=1161 => Component::Torch { mount: ((id - 1152) / 2) as u8, lit: id.is_multiple_of(2) },
        1162..=1193 => Component::Repeater {
            facing: Facing::ALL[((id - 1162) % 4) as usize],
            delay: (((id - 1162) % 16 / 4) + 1) as u8,
            on: id >= 1178,
        },
        1194..=1209 => Component::Comparator {
            facing: Facing::ALL[((id - 1194) % 4) as usize],
            subtract: (id - 1194) % 8 >= 4,
            on: id >= 1202,
        },
        1210..=1211 => Component::Lamp(id == 1211),
        1212 => Component::Source,
        _ => return None,
    })
}

pub const fn base(b: Block) -> Option<Block> {
    Some(match component(b) {
        Some(Component::Wire(_)) => WIRE,
        Some(Component::Lever { .. }) => LEVER,
        Some(Component::Button { wood: true, .. }) => WOOD_BUTTON,
        Some(Component::Button { .. }) => STONE_BUTTON,
        Some(Component::Torch { .. }) => TORCH,
        Some(Component::Repeater { .. }) => REPEATER,
        Some(Component::Comparator { .. }) => COMPARATOR,
        Some(Component::Lamp(_)) => LAMP,
        Some(Component::Source) => REDSTONE_BLOCK,
        None => return None,
    })
}

/// Mount 0 is floor, 1..4 face south/north/east/west, 5 is ceiling.
pub fn support(mount: u8) -> glam::IVec3 {
    match mount {
        0 => glam::IVec3::NEG_Y,
        5 => glam::IVec3::Y,
        _ => -Facing::ALL[(mount - 1) as usize].offset(),
    }
}

pub const fn registry(id: u16) -> Option<(&'static str, RenderKind, [u16; 6])> {
    use RenderKind::*;
    let (name, kind, layer) = match component(Block(id)) {
        Some(Component::Wire(p)) => ("redstone wire", Shaped, 1100 + p as u16),
        Some(Component::Lever { .. }) => ("lever", Shaped, tex::COBBLESTONE),
        Some(Component::Button { wood: true, .. }) => ("oak button", Shaped, tex::PLANKS),
        Some(Component::Button { .. }) => ("stone button", Shaped, tex::STONE),
        Some(Component::Torch { lit, .. }) => ("redstone torch", Shaped, if lit { 1116 } else { 1117 }),
        Some(Component::Repeater { on, .. }) => ("repeater", Shaped, if on { 1119 } else { 1118 }),
        Some(Component::Comparator { on, .. }) => ("comparator", Shaped, if on { 1119 } else { 1118 }),
        Some(Component::Lamp(on)) => ("redstone lamp", Opaque, if on { 1121 } else { 1120 }),
        Some(Component::Source) => ("block of redstone", Opaque, 1122),
        None => return None,
    };
    Some((name, kind, [layer; 6]))
}

pub fn palette_ids() -> impl Iterator<Item = u16> {
    [1100, 1116, 1128, 1140, 1152, 1162, 1194, 1210, 1212].into_iter()
}

/// Java wire sides: 0 none, 1 side, 2 up. Isolated default wire is a cross.
pub fn connections(neighbour: impl Fn(glam::IVec3) -> Block) -> [u8; 4] {
    use glam::IVec3;
    let clear = !neighbour(IVec3::Y).is_opaque();
    let mut out = [0; 4];
    for f in Facing::ALL {
        let d = f.offset();
        let n = neighbour(d);
        let connects = match component(n) {
            Some(Component::Repeater { facing, .. } | Component::Comparator { facing, .. }) => {
                f.along_x() == facing.along_x()
            }
            Some(Component::Lamp(_)) => false,
            Some(_) => true,
            _ => false,
        };
        out[f as usize] =
            if clear && n.is_solid() && matches!(component(neighbour(d + IVec3::Y)), Some(Component::Wire(_))) {
                2
            } else if connects
                || (!n.is_opaque() && matches!(component(neighbour(d - IVec3::Y)), Some(Component::Wire(_))))
            {
                1
            } else {
                0
            };
    }
    let ns = out[0] != 0 || out[1] != 0;
    let ew = out[2] != 0 || out[3] != 0;
    if !ew {
        out[0] = out[0].max(1);
        out[1] = out[1].max(1);
    }
    if !ns {
        out[2] = out[2].max(1);
        out[3] = out[3].max(1);
    }
    out
}

/// Orient mounted components toward the clicked face. Diodes face away from
/// the placer (the existing Facing::toward helper faces toward the player).
pub fn placed(b: Block, normal: glam::IVec3, toward: Facing) -> Block {
    let mount = if normal == glam::IVec3::Y {
        0
    } else if normal == glam::IVec3::NEG_Y {
        5
    } else {
        Facing::from_offset(normal).map_or(0, |f| f as u8 + 1)
    };
    match component(b) {
        Some(Component::Lever { .. }) => lever(mount, false),
        Some(Component::Button { wood, .. }) => button(mount, false, wood),
        Some(Component::Torch { .. }) => torch(if mount == 5 { 0 } else { mount }, true),
        Some(Component::Repeater { .. }) => repeater(toward.opposite(), 1, false),
        Some(Component::Comparator { .. }) => comparator(toward.opposite(), false, false),
        _ => b,
    }
}
