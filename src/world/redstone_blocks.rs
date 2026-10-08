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
pub const STONE_PLATE: Block = Block(1213);
pub const WOOD_PLATE: Block = Block(1215);
pub const LIGHT_PLATE: Block = Block(1217);
pub const HEAVY_PLATE: Block = Block(1233);
pub const IRON_DOOR: Block = Block(1249);
pub const IRON_TRAPDOOR: Block = Block(1265);
pub const DAYLIGHT: Block = Block(1281);
pub const TARGET: Block = Block(1313);
pub const WOOD_TRAPDOOR: Block = Block(1437);
pub const PISTON: Block = Block(1329);
pub const STICKY_PISTON: Block = Block(1341);
pub const OBSERVER: Block = Block(1365);
pub const DISPENSER: Block = Block(1377);
pub const DROPPER: Block = Block(1383);
pub const HOPPER: Block = Block(1389);
pub const MOVING: Block = Block(1470);
pub const HAY: Block = Block(1469);

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
    Plate { kind: u8, power: u8 },
    IronDoor { facing: Facing, open: bool, upper: bool },
    Trapdoor { facing: Facing, open: bool, top: bool, iron: bool },
    Daylight { power: u8, inverted: bool },
    Target(u8),
    GlowingOre(bool),
    Hay,
    Piston { facing: u8, sticky: bool, extended: bool },
    PistonHead { facing: u8, sticky: bool },
    Observer { facing: u8, on: bool },
    Moving,
    Dispenser { facing: u8, dropper: bool },
    Hopper { facing: u8, disabled: bool },
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
pub const fn plate(kind: u8, power: u8) -> Block {
    match kind {
        0 => Block(1213 + (power > 0) as u16),
        1 => Block(1215 + (power > 0) as u16),
        2 => Block(1217 + power as u16),
        _ => Block(1233 + power as u16),
    }
}
pub const fn iron_door(facing: Facing, open: bool, upper: bool) -> Block {
    Block(1249 + facing as u16 + open as u16 * 4 + upper as u16 * 8)
}
pub const fn trapdoor(facing: Facing, open: bool, top: bool, iron: bool) -> Block {
    Block(if iron { 1265 } else { 1437 } + facing as u16 + open as u16 * 4 + top as u16 * 8)
}
pub const fn daylight(power: u8, inverted: bool) -> Block {
    Block(1281 + power as u16 + inverted as u16 * 16)
}
pub const fn target(power: u8) -> Block {
    Block(1313 + power as u16)
}
pub const fn dot(power: u8) -> Block {
    Block(1453 + power as u16)
}

/// Six directions share the existing horizontal Facing order, then up/down.
pub fn direction(facing: u8) -> glam::IVec3 {
    match facing {
        4 => glam::IVec3::Y,
        5 => glam::IVec3::NEG_Y,
        _ => Facing::ALL[facing as usize].offset(),
    }
}
pub const fn piston(facing: u8, sticky: bool, extended: bool) -> Block {
    Block(1329 + sticky as u16 * 12 + extended as u16 * 6 + facing as u16)
}
pub const fn piston_head(facing: u8, sticky: bool) -> Block {
    Block(1353 + sticky as u16 * 6 + facing as u16)
}
pub const fn observer(facing: u8, on: bool) -> Block {
    Block(1365 + on as u16 * 6 + facing as u16)
}

pub const fn dispenser(facing: u8, dropper: bool) -> Block {
    Block(1377 + dropper as u16 * 6 + facing as u16)
}
/// Hopper facing 0..3 is horizontal, 4 is downward.
pub const fn hopper(facing: u8, disabled: bool) -> Block {
    Block(1389 + disabled as u16 * 5 + (facing as u16 + 1) % 5)
}
pub fn hopper_direction(facing: u8) -> glam::IVec3 {
    if facing == 4 { glam::IVec3::NEG_Y } else { direction(facing) }
}

pub const fn component(b: Block) -> Option<Component> {
    let id = b.0;
    Some(match id {
        1100..=1115 => Component::Wire((id - 1100) as u8),
        1453..=1468 => Component::Wire((id - 1453) as u8),
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
        1213..=1216 => {
            Component::Plate { kind: ((id - 1213) / 2) as u8, power: if id.is_multiple_of(2) { 15 } else { 0 } }
        }
        1217..=1248 => Component::Plate { kind: ((id - 1217) / 16 + 2) as u8, power: ((id - 1217) % 16) as u8 },
        1249..=1264 => Component::IronDoor {
            facing: Facing::ALL[((id - 1249) % 4) as usize],
            open: (id - 1249) % 8 >= 4,
            upper: id >= 1257,
        },
        1265..=1280 | 1437..=1452 => {
            let i = if id < 1437 { id - 1265 } else { id - 1437 };
            Component::Trapdoor {
                facing: Facing::ALL[(i % 4) as usize],
                open: i % 8 >= 4,
                top: i >= 8,
                iron: id < 1437,
            }
        }
        1281..=1312 => Component::Daylight { power: ((id - 1281) % 16) as u8, inverted: id >= 1297 },
        1313..=1328 => Component::Target((id - 1313) as u8),
        1329..=1352 => {
            Component::Piston { facing: ((id - 1329) % 6) as u8, sticky: id >= 1341, extended: (id - 1329) % 12 >= 6 }
        }
        1353..=1364 => Component::PistonHead { facing: ((id - 1353) % 6) as u8, sticky: id >= 1359 },
        1365..=1376 => Component::Observer { facing: ((id - 1365) % 6) as u8, on: id >= 1371 },
        1470 => Component::Moving,
        1377..=1388 => Component::Dispenser { facing: ((id - 1377) % 6) as u8, dropper: id >= 1383 },
        1389..=1398 => Component::Hopper { facing: (((id - 1389) % 5 + 4) % 5) as u8, disabled: id >= 1394 },
        1435..=1436 => Component::GlowingOre(id == 1436),
        1469 => Component::Hay,
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
        Some(Component::Plate { kind, .. }) => plate(kind, 0),
        Some(Component::IronDoor { .. }) => IRON_DOOR,
        Some(Component::Trapdoor { iron, .. }) => {
            if iron {
                IRON_TRAPDOOR
            } else {
                WOOD_TRAPDOOR
            }
        }
        Some(Component::Daylight { .. }) => DAYLIGHT,
        Some(Component::Target(_)) => TARGET,
        Some(Component::Hay) => HAY,
        Some(Component::Piston { sticky, .. }) => {
            if sticky {
                STICKY_PISTON
            } else {
                PISTON
            }
        }
        Some(Component::PistonHead { sticky, .. }) => piston_head(0, sticky),
        Some(Component::Observer { .. }) => OBSERVER,
        Some(Component::Moving) => MOVING,
        Some(Component::Dispenser { dropper, .. }) => {
            if dropper {
                DROPPER
            } else {
                DISPENSER
            }
        }
        Some(Component::Hopper { .. }) => HOPPER,
        Some(Component::GlowingOre(deep)) => {
            if deep {
                Block::DEEPSLATE_REDSTONE_ORE
            } else {
                Block::REDSTONE_ORE
            }
        }
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
        Some(Component::Lever { .. }) => ("lever", Shaped, 1129),
        Some(Component::Button { wood: true, .. }) => ("oak button", Shaped, tex::PLANKS),
        Some(Component::Button { .. }) => ("stone button", Shaped, tex::STONE),
        Some(Component::Torch { lit, .. }) => ("redstone torch", Shaped, if lit { 1116 } else { 1117 }),
        Some(Component::Repeater { on, .. }) => ("repeater", Shaped, if on { 1119 } else { 1118 }),
        Some(Component::Comparator { on, .. }) => ("comparator", Shaped, if on { 1119 } else { 1118 }),
        Some(Component::Lamp(on)) => ("redstone lamp", Opaque, if on { 1121 } else { 1120 }),
        Some(Component::Source) => ("block of redstone", Opaque, 1122),
        Some(Component::Plate { kind, .. }) => (
            [
                "stone pressure plate",
                "oak pressure plate",
                "light weighted pressure plate",
                "heavy weighted pressure plate",
            ][kind as usize],
            Shaped,
            [tex::STONE, tex::PLANKS, tex::GOLD_BLOCK, tex::IRON_BLOCK][kind as usize],
        ),
        Some(Component::IronDoor { .. }) => ("iron door", Shaped, 1123),
        Some(Component::Trapdoor { iron, .. }) => {
            (if iron { "iron trapdoor" } else { "oak trapdoor" }, Shaped, if iron { 1123 } else { 1124 })
        }
        Some(Component::Daylight { inverted, .. }) => ("daylight detector", Shaped, if inverted { 1126 } else { 1125 }),
        Some(Component::Target(_)) => ("target", Opaque, 1127),
        Some(Component::Hay) => ("hay bale", Opaque, 1128),
        Some(Component::GlowingOre(deep)) => (
            if deep { "deepslate redstone ore" } else { "redstone ore" },
            Opaque,
            if deep { tex::DEEPSLATE_REDSTONE_ORE } else { tex::REDSTONE_ORE },
        ),
        Some(Component::Piston { sticky, extended, .. }) => {
            (if sticky { "sticky piston" } else { "piston" }, if extended { Shaped } else { Opaque }, 1130)
        }
        Some(Component::PistonHead { .. }) => ("piston head", Shaped, 1130),
        Some(Component::Observer { .. }) => ("observer", Opaque, tex::STONE),
        Some(Component::Moving) => ("moving piston", Invisible, 1130),
        Some(Component::Dispenser { dropper, .. }) => {
            (if dropper { "dropper" } else { "dispenser" }, Opaque, tex::COBBLESTONE)
        }
        Some(Component::Hopper { .. }) => ("hopper", Shaped, 1138),
        None => return None,
    };
    let mut textures = [layer; 6];
    match component(Block(id)) {
        Some(Component::Dispenser { facing, dropper }) => {
            textures[texture_face(facing)] = if dropper { 1137 } else { 1136 }
        }
        Some(Component::Piston { facing, sticky, .. } | Component::PistonHead { facing, sticky }) => {
            textures[texture_face(facing)] = if sticky { 1132 } else { 1131 };
        }
        Some(Component::Observer { facing, on }) => {
            textures[texture_face(facing)] = 1133;
            textures[texture_face(opposite(facing))] = if on { 1135 } else { 1134 };
        }
        _ => {}
    }
    Some((name, kind, textures))
}

const fn texture_face(f: u8) -> usize {
    [4, 5, 0, 1, 2, 3][f as usize]
}
pub const fn opposite(f: u8) -> u8 {
    [1, 0, 3, 2, 5, 4][f as usize]
}

pub fn palette_ids() -> impl Iterator<Item = u16> {
    [1100, 1116, 1128, 1140, 1152, 1162, 1194, 1210, 1212, 1213, 1215, 1217, 1233, 1249, 1265, 1281, 1313, 1437]
        .into_iter()
        .chain([1329, 1341, 1365, 1377, 1383, 1389, 1469])
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
            Some(Component::Observer { facing, .. }) => direction(opposite(facing)) == -d,
            Some(
                Component::Piston { .. }
                | Component::PistonHead { .. }
                | Component::Moving
                | Component::Dispenser { .. }
                | Component::Hopper { .. }
                | Component::Lamp(_)
                | Component::IronDoor { .. }
                | Component::Trapdoor { .. }
                | Component::GlowingOre(_)
                | Component::Hay,
            ) => false,
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
    if out == [0; 4] && (1453..=1468).contains(&neighbour(IVec3::ZERO).0) {
        return out;
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
        Some(Component::Piston { sticky, .. }) => piston(toward as u8, sticky, false),
        Some(Component::Dispenser { dropper, .. }) => dispenser(toward as u8, dropper),
        Some(Component::Hopper { .. }) => {
            hopper(if normal.y != 0 { 4 } else { opposite(Facing::from_offset(normal).unwrap_or(toward) as u8) }, false)
        }
        Some(Component::Observer { .. }) => observer(toward.opposite() as u8, false),
        Some(Component::IronDoor { .. }) => iron_door(toward, false, false),
        Some(Component::Trapdoor { iron, .. }) => {
            trapdoor(Facing::from_offset(normal).unwrap_or(toward), false, normal == glam::IVec3::NEG_Y, iron)
        }
        _ => b,
    }
}

/// Directional devices use the nearest view direction, including vertical
/// placement. Mounted controls still use the clicked supporting face.
pub fn placed_with_look(b: Block, normal: glam::IVec3, look: glam::Vec3) -> Block {
    let placed = placed(b, normal, Facing::toward(look));
    if look.y.abs() <= look.x.abs().max(look.z.abs()) {
        return placed;
    }
    let toward = if look.y < 0.0 { 4 } else { 5 };
    match component(b) {
        Some(Component::Piston { sticky, .. }) => piston(toward, sticky, false),
        Some(Component::Observer { .. }) => observer(opposite(toward), false),
        Some(Component::Dispenser { dropper, .. }) => dispenser(toward, dropper),
        _ => placed,
    }
}
