//! The corpus. Each build states its Java 26.3 behaviour and where that
//! comes from; `docs/verification/2026-11-23-v0.17-reference-contraptions.md`
//! lists them with their status. Timings count machine ticks: the script
//! for tick `m` runs before game tick `m`, the way a player's action lands
//! between two server ticks, and probes read the world after it.
use super::rig::*;
use glam::{DVec3, IVec3};
use voxelcraft::{
    entity::minecart::CartKind,
    inventory::Stack,
    item::Item,
    world::{
        block::{Block, Facing, RailShape},
        crafter,
        crops::{self, Fruit},
        rails::{self, RailKind},
        redstone_blocks::{self as r, Component},
        stone_blocks::SLIME_BLOCK,
    },
};

const fn v(x: i32, y: i32, z: i32) -> IVec3 {
    IVec3::new(x, y, z)
}

fn bools(run: &Run, probe: usize, on: impl Fn(i64) -> bool) -> Vec<bool> {
    run.column(probe).into_iter().map(on).collect()
}

fn is(block: Block) -> impl Fn(i64) -> bool {
    move |id| id == block.0 as i64
}

/// The value after machine tick `m`.
fn at(col: &[i64], m: u64) -> i64 {
    col[m as usize - 1]
}

// ---------------------------------------------------------------- clocks

/// A loop of four repeaters at delay 4 (8 game ticks each) with dust at
/// the corners, started by a stone button beside one corner.
///
/// Java (`DiodeBlock`, `RepeaterBlock`, `ButtonBlock`): the button holds
/// 15 for 20 game ticks; each repeater turns on 8 ticks after its input
/// rises and off 8 ticks after it falls, and a pulse longer than the delay
/// passes unchanged. One 20-tick pulse circulates for ever: each repeater
/// is on 20 of every 32 ticks, the next one 8 ticks behind, and the first
/// turns on 8 ticks after the press.
fn repeater_loop() -> Spec {
    Spec {
        name: "repeater_loop",
        blocks: grid(
            &[&["SSSS", "SSSS", "SSSS"], &["bw>w", ".^.v", ".w<w"]],
            &[
                ('S', Block::STONE),
                ('b', r::button(0, false, false)),
                ('w', r::wire(0)),
                ('>', r::repeater(Facing::East, 4, false)),
                ('v', r::repeater(Facing::South, 4, false)),
                ('<', r::repeater(Facing::West, 4, false)),
                ('^', r::repeater(Facing::North, 4, false)),
            ],
        ),
        script: |rig, m| {
            if m == 1 {
                rig.press(v(0, 1, 0));
            }
        },
        probes: vec![
            Probe::Block(v(2, 1, 0)),
            Probe::Block(v(3, 1, 1)),
            Probe::Block(v(2, 1, 2)),
            Probe::Block(v(1, 1, 1)),
            Probe::Power(v(1, 1, 0)),
            Probe::Pending,
        ],
        ticks: 200,
        unload_at: 61,
        restart_at: 97,
        ..Spec::default()
    }
}

fn verify_repeater_loop(run: &Run) -> Result<(), String> {
    let on = |i: usize, f: Facing| bools(run, i, is(r::repeater(f, 4, true)));
    let east = on(0, Facing::East);
    let (re, fe) = (rises(&east), falls(&east));
    ensure(re.len() >= 5 && fe.len() >= 5, || format!("the loop stopped: rises {re:?}, falls {fe:?}"))?;
    for k in 0..re.len() - 1 {
        ensure(re[k + 1] - re[k] == 32, || format!("period {} at {}, Java 32", re[k + 1] - re[k], re[k]))?;
    }
    // The circulating pulse keeps the width it started with (20 in Java;
    // the exact width is checked in `player_actions_take_effect_at_once`).
    let width = fe[0] - re[0];
    for (a, b) in re.iter().zip(&fe) {
        ensure(b - a == width, || format!("on for {} from {a}, the first pulse {width}: Java keeps the width", b - a))?;
    }
    for (i, f, lag) in [(1, Facing::South, 8), (2, Facing::West, 16), (3, Facing::North, 24)] {
        let rs = rises(&on(i, f));
        for (k, &t) in rs.iter().enumerate().take(re.len()) {
            ensure(t == re[k] + lag, || format!("{f:?} repeater on at {t}, Java {}", re[k] + lag))?;
        }
    }
    Ok(())
}

#[test]
fn repeater_loop_clock() {
    check(&repeater_loop(), verify_repeater_loop);
}

/// Java runs a player's action and the neighbour updates it causes at once
/// (`Level.updateNeighborsAt` calls `neighborChanged` synchronously), so
/// the button's dust reaches the repeater in the tick of the press and it
/// lights 8 ticks later (tick 8) and carries the button's full 20-tick
/// pulse (off at 28); the burnout torch, placed in set-up, first turns off
/// at tick 2.
#[test]
fn player_actions_take_effect_at_once() {
    let loop_run = run(&repeater_loop(), Cut::None);
    let east = rises(&bools(&loop_run, 0, is(r::repeater(Facing::East, 4, true))));
    let east_falls = falls(&bools(&loop_run, 0, is(r::repeater(Facing::East, 4, true))));
    assert_eq!(east.first(), Some(&8), "first repeater on; Java 8 ticks after the press");
    assert_eq!(east_falls.first(), Some(&28), "the 20-tick button pulse; Java off at 28");
    let torch_run = run(&torch_burnout(), Cut::None);
    let offs = falls(&bools(&torch_run, 0, is(r::torch(3, true))));
    assert_eq!(offs.first(), Some(&2), "first torch turn-off; Java 2 ticks after placement");
}

/// Two observers facing each other.
///
/// Java (`ObserverBlock.tick`, `startSignal`): placing B in front of A
/// schedules A 2 ticks later. A's on-tick sets its state (the shape update
/// schedules B) before it schedules its own off-tick, so B's tick runs
/// first 2 ticks later and each observer is on for 2 and off for 4 ticks:
/// A from tick 2, B from tick 4, period 6. Checked on the vanilla 26.3
/// server (2026-11-23, Track A: `/tick freeze`, `/tick step 1`): once
/// running, A was on at ticks 8-9 and 14, B at 6-7 and 12-13.
fn observer_clock() -> Spec {
    Spec {
        name: "observer_clock",
        blocks: vec![(v(0, 0, 0), r::observer(2, false)), (v(1, 0, 0), r::observer(3, false))],
        probes: vec![Probe::Block(v(0, 0, 0)), Probe::Block(v(1, 0, 0)), Probe::Pending],
        ticks: 120,
        unload_at: 41,
        restart_at: 63,
        ..Spec::default()
    }
}

fn verify_observer_clock(run: &Run) -> Result<(), String> {
    let a = bools(run, 0, is(r::observer(2, true)));
    let b = bools(run, 1, is(r::observer(3, true)));
    for m in 1..=run.lines.len() as u64 {
        let want_a = m >= 2 && (m - 2) % 6 < 2;
        let want_b = m >= 4 && (m - 4) % 6 < 2;
        let (got_a, got_b) = (a[m as usize - 1], b[m as usize - 1]);
        ensure(got_a == want_a && got_b == want_b, || {
            format!("tick {m}: A {got_a} B {got_b}, Java A {want_a} B {want_b}")
        })?;
    }
    Ok(())
}

#[test]
fn observer_clock_pulses() {
    check(&observer_clock(), verify_observer_clock);
}

#[test]
fn observer_clock_survives_unload_and_restart() {
    persists(&observer_clock(), no_invariants);
}

/// A subtract-mode comparator fed 15 from a redstone block, its output dust
/// looping back three steps into its side.
///
/// Java (`ComparatorBlock.calculateOutputSignal`, `checkTickOnNeighbor`,
/// `RedStoneWireBlock` decay): output 15 reaches the side as 13, so the
/// output becomes 15 - 13 = 2 two ticks later; 2 reaches the side as 0, so
/// it returns to 15. The front dust alternates 15 and 2, two ticks each.
fn comparator_clock() -> Spec {
    Spec {
        name: "comparator_clock",
        blocks: grid(
            &[&[".SS", ".SS"], &["Rcw", ".ww"]],
            &[
                ('S', Block::STONE),
                ('R', r::REDSTONE_BLOCK),
                ('c', r::comparator(Facing::East, true, false)),
                ('w', r::wire(0)),
            ],
        ),
        probes: vec![Probe::Power(v(2, 1, 0)), Probe::Power(v(1, 1, 1)), Probe::Block(v(1, 1, 0)), Probe::Pending],
        ticks: 120,
        unload_at: 43,
        restart_at: 66,
        ..Spec::default()
    }
}

fn verify_comparator_clock(run: &Run) -> Result<(), String> {
    let front = run.column(0);
    let steady = &front[10..];
    ensure(steady.iter().all(|&p| p == 15 || p == 2), || format!("front dust not 15/2: {steady:?}"))?;
    let high: Vec<bool> = front.iter().map(|&p| p == 15).collect();
    let e: Vec<u64> = edges(&high).into_iter().map(|e| e.0).filter(|&t| t > 10).collect();
    ensure(e.len() >= 40, || format!("the clock stopped: {e:?}"))?;
    ensure(e.windows(2).all(|w| w[1] - w[0] == 2), || format!("Java changes every 2 ticks: {e:?}"))
}

#[test]
fn comparator_subtraction_clock() {
    check(&comparator_clock(), verify_comparator_clock);
}

/// Two hoppers feeding each other with five items. A comparator reads the
/// second; through a delay-1 repeater its dust locks the first.
///
/// Java (`HopperBlockEntity.pushItemsTick` / `tryMoveItems` / `tryMoveInItem`,
/// cooldown 8, or 7 when the receiver already ticked this tick): hopper 1
/// pushes an item at t; the comparator turns on at t+2 and the repeater at
/// t+4, locking hopper 1 before its cooldown ends (t+8). Hopper 2 pushes the
/// item back at t+7 (cooldown 8 counted down from the tick it arrived); the
/// comparator turns off at t+9, the repeater at t+11, and hopper 1 pushes
/// again at once. Period 11: the repeater is on 7 ticks and off 4.
fn hopper_clock() -> Spec {
    Spec {
        name: "hopper_clock",
        blocks: grid(
            &[&["...SSS", ".S...S", ".SSSSS"], &[".hgcRw", ".w...w", ".wwwww"]],
            &[
                ('S', Block::STONE),
                ('h', r::hopper(2, false)),
                ('g', r::hopper(3, false)),
                ('c', r::comparator(Facing::East, false, false)),
                ('R', r::repeater(Facing::East, 1, false)),
                ('w', r::wire(0)),
            ],
        ),
        contents: vec![(v(1, 1, 0), vec![Stack::new(Block::COBBLESTONE, 5)])],
        probes: vec![
            Probe::Items(v(1, 1, 0)),
            Probe::Items(v(2, 1, 0)),
            Probe::Block(v(3, 1, 0)),
            Probe::Block(v(4, 1, 0)),
            Probe::Block(v(1, 1, 0)),
            Probe::Scheduled(v(1, 1, 0)),
            Probe::Scheduled(v(2, 1, 0)),
            Probe::Pending,
        ],
        closed: true,
        ticks: 160,
        unload_at: 47,
        restart_at: 81,
        ..Spec::default()
    }
}

fn verify_hopper_clock(run: &Run) -> Result<(), String> {
    let on = bools(run, 3, is(r::repeater(Facing::East, 1, true)));
    let (rs, fs) = (rises(&on), falls(&on));
    ensure(rs.len() >= 6, || format!("the clock stopped: {rs:?}"))?;
    for w in rs[1..].windows(2) {
        ensure(w[1] - w[0] == 11, || format!("period {} at {}, Java 11", w[1] - w[0], w[0]))?;
    }
    for &f in &fs {
        if let Some(&r0) = rs.iter().rev().find(|&&r0| r0 < f) {
            ensure(f - r0 == 7, || format!("repeater on {} ticks from {r0}, Java 7", f - r0))?;
        }
    }
    Ok(())
}

#[test]
#[ignore = "needs Track C2 (hopper transfer timing) and Track A: Java's receiving hopper still ticks in the tick \
            it receives (block-entity tickers after the sender), so it pushes 7 ticks later, and a hopper unlocked \
            by a scheduled tick pushes in the same tick; VoxelCraft takes 8 and 1, a period of 13 instead of 11"]
fn hopper_clock_period() {
    check(&hopper_clock(), verify_hopper_clock);
}

#[test]
fn hopper_clock_survives_unload_and_restart() {
    persists(&hopper_clock(), no_invariants);
}

/// A wall torch whose output powers the block above it, dust on its support
/// block picks that up and powers the support: the torch turns itself off.
///
/// Java (`RedstoneTorchBlock.tick`, `neighborChanged`, `isToggledTooFrequently`;
/// `LevelChunkTicks.schedule` keeps one tick per block and position): the
/// torch toggles every 2 ticks, off at 2, 6, 10 ... and the eighth turn-off
/// within 60 ticks (tick 30) burns it out. The turn-off's own neighbour
/// update has already scheduled a 2-tick check, so the 160-tick restart is
/// dropped as a duplicate; the check at 32 still counts eight recent
/// toggles and does nothing, and with no further update the torch stays
/// out for good.
fn torch_burnout() -> Spec {
    Spec {
        name: "torch_burnout",
        blocks: vec![
            (v(0, 0, 0), Block::STONE),
            (v(1, 0, 0), r::torch(3, true)),
            (v(0, 1, 0), r::wire(0)),
            (v(1, 1, 0), Block::STONE),
        ],
        probes: vec![Probe::Block(v(1, 0, 0)), Probe::Power(v(0, 1, 0)), Probe::Pending],
        ticks: 300,
        unload_at: 120,
        restart_at: 160,
        // Java forgets the toggle history on a restart, so a restart while
        // it toggles is its own case (`torch_burnout_history_is_not_saved`).
        motion_after: 10_000,
        ..Spec::default()
    }
}

fn verify_torch_burnout(run: &Run) -> Result<(), String> {
    let lit = bools(run, 0, is(r::torch(3, true)));
    let offs = falls(&lit);
    ensure(lit[0], || "the torch starts lit".into())?;
    ensure(offs.len() == 8, || {
        format!("{} turn-offs at {offs:?}; Java burns out on the 8th and stays out", offs.len())
    })?;
    ensure(offs.windows(2).all(|w| w[1] - w[0] == 4), || format!("Java toggles every 2 ticks: {offs:?}"))?;
    let last = *offs.last().unwrap() as usize;
    ensure(lit[last - 1..].iter().all(|&l| !l), || format!("relit after burning out at {last}"))
}

#[test]
fn torch_burnout_stays_out() {
    check(&torch_burnout(), verify_torch_burnout);
}

#[test]
fn torch_burnout_survives_unload_and_restart() {
    persists(&torch_burnout(), no_invariants);
}

/// Java keeps the toggle history in memory only (`RedstoneTorchBlock.RECENT_TOGGLES`,
/// a `WeakHashMap` per level that nothing saves): after a restart the
/// torch needs eight fresh turn-offs before it burns out.
#[test]
fn torch_burnout_history_is_not_saved() {
    let spec = torch_burnout();
    let run = run(&spec, Cut::Restart(12));
    let lit = bools(&run, 0, is(r::torch(3, true)));
    let after: Vec<u64> = falls(&lit).into_iter().filter(|&t| t >= 12).collect();
    assert_eq!(after.len(), 8, "turn-offs after the restart at 12: {after:?}; Java needs eight new ones");
}

// ----------------------------------------------------------------- doors

const DOOR: Block = Block::STONE_BRICKS;

/// Four sticky pistons, two per side, close a 2x2 doorway. Redstone blocks
/// placed behind them stand in for the door's input wiring.
///
/// Java (`PistonBaseBlock.checkIfExtend` queues a block event that runs in
/// the same tick; `PistonMovingBlockEntity` lands 2 ticks later; sticky
/// retraction pulls the block back): power placed before tick m fills the
/// doorway after tick m+2 and removing it reopens it after m+2. Four door
/// blocks, never more or fewer.
fn door_2x2() -> Spec {
    Spec {
        name: "door_2x2",
        blocks: grid(
            &[&[], &[".PD..dQ."], &[".PD..dQ."]],
            &[('P', r::piston(2, true, false)), ('Q', r::piston(3, true, false)), ('D', DOOR), ('d', DOOR)],
        ),
        script: |rig, m| {
            let b = match m {
                5 | 80 => r::REDSTONE_BLOCK,
                40 | 120 => Block::AIR,
                _ => return,
            };
            for p in [v(0, 1, 0), v(0, 2, 0), v(7, 1, 0), v(7, 2, 0)] {
                rig.set(p, b);
            }
        },
        probes: vec![
            Probe::Block(v(3, 1, 0)),
            Probe::Block(v(4, 1, 0)),
            Probe::Block(v(3, 2, 0)),
            Probe::Block(v(4, 2, 0)),
            Probe::Block(v(2, 1, 0)),
            Probe::Block(v(5, 1, 0)),
            Probe::Block(v(2, 2, 0)),
            Probe::Block(v(5, 2, 0)),
            Probe::Moving,
            Probe::Pending,
        ],
        counted: |b| b == DOOR,
        closed: true,
        ticks: 160,
        unload_at: 41,
        restart_at: 100,
        motion_after: 75,
        ..Spec::default()
    }
}

fn verify_door_2x2(run: &Run) -> Result<(), String> {
    let d = DOOR.0 as i64;
    for m in 1..=run.lines.len() as u64 {
        let line = &run.lines[m as usize - 1];
        let closed = (7..=39).contains(&m) || (82..=119).contains(&m);
        let open = (1..=4).contains(&m) || (42..=79).contains(&m) || m >= 122;
        if closed {
            ensure(line[..4].iter().all(|&b| b == d), || format!("tick {m}: doorway {:?} not closed", &line[..4]))?;
        }
        if open {
            ensure(line[..4].iter().all(|&b| b == 0) && line[4..8].iter().all(|&b| b == d), || {
                format!("tick {m}: doorway {:?} not open", &line[..8])
            })?;
        }
    }
    Ok(())
}

#[test]
fn piston_door_2x2() {
    check(&door_2x2(), verify_door_2x2);
}

/// A 3x3 doorway: three sticky pistons above and three below fill the top
/// and bottom rows, one on the right fills the middle-right cell, and a
/// double piston extender on the left pushes a gold block two cells to the
/// centre, its inner piston's head filling the middle-left cell. Redstone
/// blocks placed and removed by the script are the sequencer.
///
/// Java (`PistonBaseBlock.moveBlocks`, `PistonStructureResolver`; extended
/// pistons can't be moved; a sticky piston pulls the block in front of its
/// head): close = all outer pistons and the outer extender piston extend
/// (tick 5), then the inner piston extends from its new place (tick 9),
/// pushing the gold block to the centre; closed from tick 11. Open = inner
/// retracts pulling the gold block back one (60), outer retracts pulling
/// the inner piston back (64), inner extends into the empty cell (68) and
/// retracts pulling the gold block home (72); open from tick 74.
fn door_3x3() -> Spec {
    let mut blocks = Vec::new();
    for x in 4..=6 {
        blocks.push((v(x, 7, 0), r::piston(5, true, false)));
        blocks.push((v(x, 6, 0), DOOR));
        blocks.push((v(x, 1, 0), r::piston(4, true, false)));
        blocks.push((v(x, 2, 0), DOOR));
    }
    blocks.extend([
        (v(8, 4, 0), r::piston(3, true, false)),
        (v(7, 4, 0), DOOR),
        (v(1, 4, 0), r::piston(2, true, false)),
        (v(2, 4, 0), r::piston(2, true, false)),
        (v(3, 4, 0), Block::GOLD_BLOCK),
    ]);
    let mut probes: Vec<Probe> = Vec::new();
    for y in 3..=5 {
        for x in 4..=6 {
            probes.push(Probe::Block(v(x, y, 0)));
        }
    }
    probes.extend([Probe::Block(v(3, 4, 0)), Probe::Moving, Probe::Pending]);
    Spec {
        name: "door_3x3",
        blocks,
        script: |rig, m| {
            let outer = [v(4, 8, 0), v(5, 8, 0), v(6, 8, 0), v(4, 0, 0), v(5, 0, 0), v(6, 0, 0), v(9, 4, 0)];
            match m {
                5 => {
                    for p in outer {
                        rig.set(p, r::REDSTONE_BLOCK);
                    }
                    rig.set(v(0, 4, 0), r::REDSTONE_BLOCK);
                }
                9 => rig.set(v(3, 3, 0), r::REDSTONE_BLOCK),
                60 => {
                    for p in outer {
                        rig.set(p, Block::AIR);
                    }
                    rig.set(v(3, 3, 0), Block::AIR);
                }
                64 => rig.set(v(0, 4, 0), Block::AIR),
                68 => rig.set(v(2, 3, 0), r::REDSTONE_BLOCK),
                72 => rig.set(v(2, 3, 0), Block::AIR),
                _ => {}
            }
        },
        probes,
        counted: |b| b == DOOR || b == Block::GOLD_BLOCK,
        closed: true,
        ticks: 100,
        unload_at: 63,
        restart_at: 61,
        motion_after: 5,
        ..Spec::default()
    }
}

fn verify_door_3x3(run: &Run) -> Result<(), String> {
    let (d, g) = (DOOR.0 as i64, Block::GOLD_BLOCK.0 as i64);
    let head = r::piston_head(2, true).0 as i64;
    // Rows bottom to top, x 4..=6.
    let closed = [d, d, d, head, g, d, d, d, d];
    for m in 1..=run.lines.len() as u64 {
        let line = &run.lines[m as usize - 1];
        if (13..=59).contains(&m) {
            ensure(line[..9] == closed, || format!("tick {m}: doorway {:?}, Java {closed:?}", &line[..9]))?;
        }
        if (1..=4).contains(&m) || m >= 76 {
            ensure(line[..9].iter().all(|&b| b == 0) && line[9] == g, || {
                format!("tick {m}: doorway {:?} and pocket {} not open", &line[..9], line[9])
            })?;
        }
    }
    Ok(())
}

#[test]
fn piston_door_3x3() {
    check(&door_3x3(), verify_door_3x3);
}

// ------------------------------------------------------- flying machine

/// The two-piston slime flying machine heading east: back half = a normal
/// piston facing east, a slime block on it and an observer beside the slime
/// facing south; front half = a slime block in front of the piston, a
/// second slime beside it, a sticky piston facing west above the first and
/// an observer facing south above the second. Each observer's back powers
/// its half's piston (through the slime block for the back half).
///
/// Java (`ObserverBlock`; `PistonMovingBlockEntity.finalTick` lands a moved
/// block through `Block.updateFromNeighbourShapes`, so a moved observer
/// fires; slime sticks to every movable neighbour except the moving piston;
/// block events run before block entities): a change in front of the back
/// observer (tick 1) fires it at 2; the normal piston pushes the front half,
/// which lands at 4; its observer fires at 6, the sticky piston extends to
/// the back half's slime and at 8 retracts, pulling the back half, which
/// lands at 10 and fires the back observer again. One block east every 10
/// ticks: the back slime arrives at x = 1 at tick 10, x = 2 at 20, and so on.
fn flying_machine() -> Spec {
    Spec {
        name: "flying_machine",
        blocks: vec![
            (v(0, 0, 0), r::piston(2, false, false)),
            (v(0, 1, 0), SLIME_BLOCK),
            (v(0, 1, 1), r::observer(0, false)),
            (v(1, 0, 0), SLIME_BLOCK),
            (v(1, 0, 1), SLIME_BLOCK),
            (v(1, 1, 0), r::piston(3, true, false)),
            (v(1, 1, 1), r::observer(0, false)),
        ],
        script: |rig, m| {
            if m == 1 {
                rig.set(v(0, 1, 2), Block::STONE);
                rig.set(v(0, 1, 2), Block::AIR);
            }
        },
        probes: vec![Probe::Locate(SLIME_BLOCK, 1, 0), Probe::Locate(SLIME_BLOCK, 0, 0), Probe::Moving, Probe::Pending],
        reach: v(16, 0, 0),
        counted: |b| {
            b == SLIME_BLOCK || matches!(r::component(b), Some(Component::Observer { .. } | Component::Piston { .. }))
        },
        closed: true,
        ticks: 120,
        unload_at: 45,
        restart_at: 73,
        motion_after: 30,
        ..Spec::default()
    }
}

fn verify_flying_machine(run: &Run) -> Result<(), String> {
    let back = run.column(0);
    let mut arrivals = Vec::new();
    let mut last = 0;
    for (i, &x) in back.iter().enumerate() {
        if x >= 0 && x != last {
            arrivals.push((i as u64 + 1, x));
            last = x;
        }
    }
    ensure(arrivals.len() >= 10, || format!("the machine stalled: {arrivals:?}"))?;
    for (k, &(t, x)) in arrivals.iter().enumerate() {
        let k = k as i64 + 1;
        ensure(x == k && t as i64 == 10 * k, || {
            format!("back slime at x {x} on tick {t}, Java x {k} on tick {}", 10 * k)
        })?;
    }
    Ok(())
}

#[test]
#[ignore = "needs Track B: pistons must move slime assemblies (slime sticks to every movable neighbour, \
            PistonStructureResolver) and moved observers must fire on landing (PistonMovingBlockEntity.finalTick \
            -> updateFromNeighbourShapes); today the pistons move only the block in front"]
fn flying_machine_travels() {
    check(&flying_machine(), verify_flying_machine);
}

#[test]
fn flying_machine_survives_unload_and_restart() {
    persists(&flying_machine(), no_invariants);
}

// ------------------------------------------------------------- sorting

/// The hopper-comparator item filter: a hopper line carries iron, diamonds
/// and gold; the filter hopper under it (41 iron plus one each of four
/// fillers, facing its comparator so it never pushes) pulls only iron. Its
/// comparator reads 2; at 42 iron it reads 3, which survives two dust
/// steps to power a block, turning off the torch that locks the hopper
/// below, which then pulls one iron into the output chest.
///
/// Java (`AbstractContainerMenu.getRedstoneSignalFromContainer` = 1 +
/// floor(14 x fullness): (41 + 4)/64/5 -> 2, (42 + 4)/64/5 -> 3; hoppers
/// pull only what fits; locked hoppers neither push nor pull): every iron
/// ends in the output chest, every diamond and gold ingot at the end of the
/// line, and the filter keeps its 41 iron and fillers.
fn item_sorter() -> Spec {
    let iron = Stack::new(Item::IRON_INGOT, 5);
    let diamond = Stack::new(Item::DIAMOND, 5);
    let gold = Stack::new(Item::GOLD_INGOT, 5);
    Spec {
        name: "item_sorter",
        blocks: grid(
            &[
                &[".kSS", "...S", "...S", "...S", ".S.S", "...S", "..SS"],
                &[".Lww", ".S.w", ".S.w", ".S.w", ".w.w", ".S.w", ".tww"],
                &[".F", ".c", ".w", ".w"],
                &["hhhk"],
                &["k"],
            ],
            &[
                ('S', Block::STONE),
                ('k', Block::CHEST),
                ('L', r::hopper(4, false)),
                ('w', r::wire(0)),
                ('t', r::torch(1, true)),
                ('F', r::hopper(0, false)),
                ('c', r::comparator(Facing::South, false, false)),
                ('h', r::hopper(2, false)),
            ],
        ),
        contents: vec![
            (
                v(1, 2, 0),
                vec![
                    Stack::new(Item::IRON_INGOT, 41),
                    Stack::new(Item::STICK, 1),
                    Stack::new(Item::COAL, 1),
                    Stack::new(Item::REDSTONE, 1),
                    Stack::new(Item::EMERALD, 1),
                ],
            ),
            (v(0, 4, 0), vec![iron, diamond, gold, iron, diamond, gold]),
        ],
        probes: vec![
            Probe::Count(v(1, 0, 0), Item::IRON_INGOT),
            Probe::Items(v(1, 0, 0)),
            Probe::Count(v(3, 3, 0), Item::DIAMOND),
            Probe::Count(v(3, 3, 0), Item::GOLD_INGOT),
            Probe::Count(v(3, 3, 0), Item::IRON_INGOT),
            Probe::Items(v(1, 2, 0)),
            Probe::Items(v(1, 1, 0)),
            Probe::Block(v(1, 1, 6)),
            Probe::Block(v(1, 2, 0)),
            Probe::Items(v(0, 3, 0)),
            Probe::Items(v(1, 3, 0)),
            Probe::Items(v(2, 3, 0)),
            Probe::Count(v(1, 2, 0), Item::IRON_INGOT),
            Probe::Count(v(1, 3, 0), Item::IRON_INGOT),
            Probe::Count(v(1, 1, 0), Item::IRON_INGOT),
            Probe::Count(v(1, 2, 0), Item::STICK),
            Probe::Count(v(1, 2, 0), Item::EMERALD),
            Probe::Pending,
        ],
        closed: true,
        ticks: 420,
        unload_at: 101,
        restart_at: 163,
        motion_after: 30,
        ..Spec::default()
    }
}

fn verify_item_sorter(run: &Run) -> Result<(), String> {
    let last = run.lines.last().unwrap();
    let want = [10, 10, 10, 10, 0, 45, 0];
    ensure(last[..7] == want, || {
        format!(
            "end state (output iron, output items, diamonds, gold, stray iron, filter, lock hopper) {:?}, Java {want:?}",
            &last[..7]
        )
    })?;
    let stray = run.lines.iter().position(|l| l[0] != l[1] || l[4] != 0);
    ensure(stray.is_none(), || format!("an item went the wrong way at tick {}", stray.unwrap() + 1))
}

#[test]
#[ignore = "needs Track C2 (hopper transfer timing): Java's hopper that receives into an empty inventory \
            pushes 7 ticks later (HopperBlockEntity: cooldown 8 - k, and it still ticks this tick), so a carrier \
            hopper fed every 8 ticks is empty when each item arrives and holds it 7 ticks for the filter to pull; \
            VoxelCraft waits 8, the carrier keeps a buffered item, and each new item leaves the tick it arrives, \
            passing the filter (4 of 10 iron sorted)"]
fn item_sorter_filters() {
    check(&item_sorter(), verify_item_sorter);
}

#[test]
fn item_sorter_survives_unload_and_restart() {
    persists(&item_sorter(), no_invariants);
}

/// Hoppers feed a furnace raw iron from the top and coal from the side; a
/// hopper under it takes the ingots to a chest.
///
/// Java (`AbstractFurnaceBlockEntity.serverTick`: 200 ticks per smelt,
/// coal burns 1600; `HopperBlockEntity`; furnace faces: top input, side
/// fuel, bottom output): one coal smelts all four; each ingot finishes
/// exactly 200 ticks after the previous one, and raw iron plus ingots stay
/// four throughout.
fn auto_smelter() -> Spec {
    Spec {
        name: "auto_smelter",
        blocks: vec![
            (v(0, 0, 0), r::hopper(2, false)),
            (v(1, 0, 0), Block::CHEST),
            (v(0, 1, 0), Block::FURNACE),
            (v(1, 1, 0), r::hopper(3, false)),
            (v(0, 2, 0), r::hopper(4, false)),
            (v(1, 2, 0), Block::CHEST),
            (v(0, 3, 0), Block::CHEST),
        ],
        contents: vec![
            (v(0, 3, 0), vec![Stack::new(Item::RAW_IRON, 4)]),
            (v(1, 2, 0), vec![Stack::new(Item::COAL, 1)]),
        ],
        probes: vec![
            Probe::Count(v(0, 3, 0), Item::RAW_IRON),
            Probe::Count(v(0, 2, 0), Item::RAW_IRON),
            Probe::Count(v(0, 1, 0), Item::RAW_IRON),
            Probe::Count(v(0, 1, 0), Item::IRON_INGOT),
            Probe::Count(v(0, 0, 0), Item::IRON_INGOT),
            Probe::Count(v(1, 0, 0), Item::IRON_INGOT),
            Probe::Count(v(0, 1, 0), Item::COAL),
            Probe::Count(v(1, 2, 0), Item::COAL),
            Probe::Count(v(1, 1, 0), Item::COAL),
        ],
        ticks: 900,
        unload_at: 301,
        restart_at: 503,
        motion_after: 10_000,
        ..Spec::default()
    }
}

fn verify_auto_smelter(run: &Run) -> Result<(), String> {
    for (i, l) in run.lines.iter().enumerate() {
        ensure(l[..6].iter().sum::<i64>() == 4, || format!("tick {}: raw + ingots {:?} != 4", i + 1, &l[..6]))?;
    }
    let made: Vec<i64> = run.lines.iter().map(|l| l[3] + l[4] + l[5]).collect();
    let done: Vec<u64> = made.windows(2).enumerate().filter(|(_, w)| w[1] > w[0]).map(|(i, _)| i as u64 + 2).collect();
    ensure(done.len() == 4, || format!("ingots finished at {done:?}; Java makes four"))?;
    ensure(done.windows(2).all(|w| w[1] - w[0] == 200), || format!("ingots at {done:?}; Java 200 ticks apart"))?;
    let last = run.lines.last().unwrap();
    ensure(last[5] == 4 && last[6] + last[7] + last[8] == 0, || {
        format!("end state {last:?}: Java 4 ingots in the chest, coal burnt")
    })
}

#[test]
fn auto_smelter_smelts() {
    check(&auto_smelter(), verify_auto_smelter);
}

// ---------------------------------------------------------------- farms

/// Sugar cane on sand beside a water source; an observer watches the third
/// block and its dust powers the support beside a piston that breaks the
/// second, so the third pops too.
///
/// Java (`SugarCaneBlock.randomTick`, `PushReaction.POPPED` for sugar cane
/// in `Blocks`, `ObserverBlock`): cane grows at random, so this build is
/// checked by accounting, not by trace: every grown block is either
/// standing or an item (none lost or duplicated, across unload and
/// restart too), the bottom block is never harvested, and a third block
/// never stands for more than a few ticks.
fn sugar_cane_farm() -> Spec {
    Spec {
        name: "sugar_cane_farm",
        blocks: vec![
            (v(1, 0, 1), Block::STONE),
            (v(2, 0, 1), Block::STONE),
            (v(0, 1, 1), Block::STONE),
            (v(1, 1, 0), Block::STONE),
            (v(1, 1, 2), Block::STONE),
            (v(1, 1, 1), Block::WATER),
            (v(2, 1, 1), Block::SAND),
            (v(2, 2, 1), Block::SUGAR_CANE),
            (v(4, 3, 1), Block::STONE),
            (v(3, 3, 1), r::piston(3, false, false)),
            (v(4, 4, 1), r::wire(0)),
            (v(3, 4, 1), r::observer(3, false)),
        ],
        probes: vec![
            Probe::Block(v(2, 2, 1)),
            Probe::Block(v(2, 3, 1)),
            Probe::Block(v(2, 4, 1)),
            Probe::LooseItems,
            Probe::Moving,
        ],
        counted: |b| b == Block::SUGAR_CANE,
        deterministic: false,
        random_tick_speed: 1024,
        ticks: 1200,
        unload_at: 401,
        restart_at: 703,
        motion_after: 10_000,
        ..Spec::default()
    }
}

/// Ticks (0-based) at which a cell turned into `block`.
fn appeared(col: &[i64], block: Block) -> Vec<usize> {
    (1..col.len()).filter(|&t| col[t - 1] != block.0 as i64 && col[t] == block.0 as i64).collect()
}

/// Every grown block is standing or an item: the total changes by exactly
/// the blocks that appeared. Holds whatever the timing.
fn grown_accounting(run: &Run, cells: &[usize], block: Block) -> Result<usize, String> {
    let mut grown = vec![0i64; run.lines.len()];
    for &c in cells {
        for t in appeared(&run.column(c), block) {
            grown[t] += 1;
        }
    }
    for t in 1..run.lines.len() {
        ensure(run.totals[t] - run.totals[t - 1] == grown[t], || {
            format!("tick {}: total {} -> {} with {} grown", t + 1, run.totals[t - 1], run.totals[t], grown[t])
        })?;
    }
    Ok(grown.iter().sum::<i64>() as usize)
}

fn sugar_cane_accounting(run: &Run) -> Result<(), String> {
    ensure(run.column(0).iter().all(|&b| b == Block::SUGAR_CANE.0 as i64), || "the bottom cane went".into())?;
    grown_accounting(run, &[1, 2], Block::SUGAR_CANE).map(|_| ())
}

fn verify_sugar_cane_farm(run: &Run) -> Result<(), String> {
    sugar_cane_accounting(run)?;
    let third = run.column(2);
    let mut standing = 0;
    for (t, &b) in third.iter().enumerate() {
        standing = if b == Block::SUGAR_CANE.0 as i64 { standing + 1 } else { 0 };
        ensure(standing <= 6, || format!("tick {}: a third cane stood {standing} ticks unharvested", t + 1))?;
    }
    let harvests = appeared(&run.column(1), Block::AIR).len();
    ensure(harvests >= 4, || format!("only {harvests} harvests"))
}

#[test]
#[ignore = "needs Track B: a block a piston destroys must update its neighbours (PistonBaseBlock.moveBlocks \
            drops it and sets air with neighbour updates), so the cane above loses its support and pops; \
            VoxelCraft leaves it floating and the farm jams after one harvest"]
fn sugar_cane_farm_harvests() {
    check(&sugar_cane_farm(), verify_sugar_cane_farm);
}

#[test]
fn sugar_cane_farm_survives_unload_and_restart() {
    persists(&sugar_cane_farm(), sugar_cane_accounting);
}

/// Java's sugar cane needs 16 random ticks per block (`SugarCaneBlock`
/// AGE 0..15; the age-15 tick grows the block above and resets the age).
/// In the farm each harvest needs two growths, so at random tick speed 1024
/// (0.25 random ticks per block per tick) a block grows about every 64
/// ticks: about 62 in 4000 ticks.
#[test]
#[ignore = "needs a sugar cane fix outside the v0.17 tracks: Java grows a block after 16 random ticks (AGE 0..15), \
            VoxelCraft with chance 1/4 per random tick (world::growth tick_cane), four times as fast; and Track B's \
            piston fix, without which this farm jams after its first harvest"]
fn sugar_cane_grows_at_java_rate() {
    let spec = Spec { ticks: 4000, ..sugar_cane_farm() };
    let run = run(&spec, Cut::None);
    let grown = appeared(&run.column(1), Block::SUGAR_CANE).len() + appeared(&run.column(2), Block::SUGAR_CANE).len();
    assert!((45..=85).contains(&grown), "{grown} cane grew in 4000 ticks; Java about 62");
}

/// A pumpkin stem (age 7) on farmland under open sky, with glass on two
/// sides and an observer on the third, so its pumpkin can only grow onto
/// the dirt east of it. The observer watches the stem attach and its dust
/// powers a piston facing the pumpkin.
///
/// Java (`StemBlock.randomTick`; `Blocks.PUMPKIN` is `PushReaction.POPPED`,
/// so the piston breaks it where it stands and drops it, leaving the stem;
/// `AttachedStemBlock` reverts to a plain stem): pumpkins grow at random,
/// so the check is accounting: every pumpkin grown is an item or still
/// standing, the stem survives, and no pumpkin stands more than a few ticks.
fn pumpkin_farm() -> Spec {
    Spec {
        name: "pumpkin_farm",
        blocks: vec![
            (v(1, 1, 2), Block::FARMLAND),
            (v(2, 1, 2), Block::DIRT),
            (v(1, 2, 2), crops::stem_block(Fruit::Pumpkin, 7)),
            (v(0, 2, 2), Block::GLASS),
            (v(1, 2, 3), Block::GLASS),
            (v(3, 2, 2), r::piston(3, false, false)),
            (v(1, 1, 0), Block::STONE),
            (v(2, 1, 0), Block::STONE),
            (v(3, 1, 0), Block::STONE),
            (v(3, 1, 1), Block::STONE),
            (v(1, 2, 0), r::wire(0)),
            (v(2, 2, 0), r::wire(0)),
            (v(3, 2, 0), r::wire(0)),
            (v(3, 2, 1), r::wire(0)),
            (v(1, 2, 1), r::observer(0, false)),
        ],
        probes: vec![Probe::Block(v(1, 2, 2)), Probe::Block(v(2, 2, 2)), Probe::LooseItems, Probe::Moving],
        counted: |b| b == Block::PUMPKIN,
        deterministic: false,
        random_tick_speed: 1024,
        ticks: 2000,
        unload_at: 701,
        restart_at: 1303,
        motion_after: 10_000,
        ..Spec::default()
    }
}

fn pumpkin_accounting(run: &Run) -> Result<(), String> {
    grown_accounting(run, &[1], Block::PUMPKIN).map(|_| ())
}

fn verify_pumpkin_farm(run: &Run) -> Result<(), String> {
    let (stem, spot) = (run.column(0), run.column(1));
    if let Some(t) = stem.iter().position(|&b| crops::stem(Block(b as u16)).is_none()) {
        return Err(format!("tick {}: the stem is gone ({})", t + 1, stem[t]));
    }
    let grown = grown_accounting(run, &[1], Block::PUMPKIN)?;
    let mut standing = 0;
    for (t, &b) in spot.iter().enumerate() {
        standing = if b == Block::PUMPKIN.0 as i64 { standing + 1 } else { 0 };
        ensure(standing <= 6, || format!("tick {}: a pumpkin stood {standing} ticks", t + 1))?;
    }
    ensure(grown >= 2, || format!("only {grown} pumpkins grew"))
}

#[test]
#[ignore = "needs Track B: Java 26.3 registers pumpkins (and melons and carved pumpkins) with PushReaction.POPPED, \
            so the piston breaks the pumpkin in place; VoxelCraft moves pumpkins (automation::piston_reaction). \
            Also undiagnosed: no pumpkin grew in 2000 ticks at speed 1024 here (Java about ten); check the stem's \
            growth light in the headless world first"]
fn pumpkin_farm_harvests() {
    check(&pumpkin_farm(), verify_pumpkin_farm);
}

#[test]
fn pumpkin_farm_survives_unload_and_restart() {
    persists(&pumpkin_farm(), pumpkin_accounting);
}

// ------------------------------------------------------------- transport

/// A hopper minecart with ten iron ingots rolls east from a powered rail
/// along a straight track over a detector rail and stops against a wall on
/// a rail laid on a hopper, which unloads it into a chest.
///
/// Java (`OldMinecartBehavior`, `PoweredRailBlock`, `DetectorRailBlock`
/// powers while a cart is inside it and rechecks every 20 ticks,
/// `HopperBlockEntity.suckInItems` from a container entity above, one item
/// per 8-tick cooldown): the detector rail powers while the cart passes and
/// is off again at the end; the cart rests over the unloader; the chest
/// gains one ingot every 8 ticks until it holds all ten; cart, hopper and
/// chest always hold ten together.
fn minecart_line() -> Spec {
    let rail = Block::rail(RailShape::EastWest);
    let mut blocks =
        vec![(v(10, 0, 0), Block::CHEST), (v(10, 1, 0), r::hopper(4, false)), (v(1, 1, 0), r::REDSTONE_BLOCK)];
    for x in (0..=9).chain([11]) {
        if x != 1 {
            blocks.push((v(x, 1, 0), Block::STONE));
        }
    }
    blocks.extend([
        (v(0, 2, 0), Block::STONE),
        (v(1, 2, 0), rails::special(RailKind::Powered, RailShape::EastWest, true)),
        (v(8, 2, 0), rails::special(RailKind::Detector, RailShape::EastWest, false)),
        (v(11, 2, 0), Block::STONE),
    ]);
    for x in (2..=7).chain([9, 10]) {
        blocks.push((v(x, 2, 0), rail));
    }
    Spec {
        name: "minecart_line",
        blocks,
        setup: |rig| {
            let id = rig.entities.place_cart(&rig.world, CartKind::Hopper, abs(v(1, 2, 0))).expect("cart on the rail");
            let cart = rig.entities.cart_mut(id).unwrap();
            cart.vel = DVec3::new(0.3, 0.0, 0.0);
            cart.slots[0] = Some(Stack::new(Item::IRON_INGOT, 10));
        },
        probes: vec![
            Probe::CartX(0),
            Probe::CartItems(0),
            Probe::Items(v(10, 1, 0)),
            Probe::Items(v(10, 0, 0)),
            Probe::Block(v(8, 2, 0)),
            Probe::Pending,
        ],
        closed: true,
        ticks: 260,
        unload_at: 9,
        restart_at: 17,
        motion_after: 12,
        ..Spec::default()
    }
}

fn verify_minecart_line(run: &Run) -> Result<(), String> {
    let detector_on = bools(run, 4, is(rails::special(RailKind::Detector, RailShape::EastWest, true)));
    ensure(detector_on.iter().any(|&o| o), || "the detector rail never powered".into())?;
    ensure(!*detector_on.last().unwrap(), || "the detector rail is still powered at the end".into())?;
    let last = run.lines.last().unwrap();
    let x = (ORIGIN.x + 10) as i64 * 1000;
    ensure((x..x + 1000).contains(&last[0]), || {
        format!("cart rests at x {}, Java over the unloader at {x}..{}", last[0], x + 1000)
    })?;
    ensure(last[1] == 0 && last[3] == 10, || {
        format!("cart {} chest {}: Java empties the cart into the chest", last[1], last[3])
    })?;
    let chest = run.column(3);
    let gains: Vec<u64> =
        chest.windows(2).enumerate().filter(|(_, w)| w[1] > w[0]).map(|(i, _)| i as u64 + 2).collect();
    ensure(gains.windows(2).all(|w| w[1] - w[0] == 8), || format!("chest gains at {gains:?}; Java one per 8 ticks"))
}

#[test]
#[ignore = "needs Track D1: Java rails stand on hoppers (BaseRailBlock.canSurvive -> canSupportRigidBlock; the \
            hopper's top rim is rigid), VoxelCraft pops that rail into an item; and a hopper under a cart pushes \
            and pulls in the same 8-tick cycle (tryMoveItems), while VoxelCraft drains the cart every 16 ticks"]
fn minecart_storage_line() {
    check(&minecart_line(), verify_minecart_line);
}

#[test]
fn minecart_storage_line_survives_unload_and_restart() {
    persists(&minecart_line(), no_invariants);
}

// -------------------------------------------------------------- crafting

/// The crafter facing east (`FrontAndTop.EAST_UP`), untriggered and idle,
/// so it puts its results into the chest east of it. A new crafter has all
/// nine slots enabled, and they are chest-backed. (`Block::from_name`
/// gives the item's state, `north_up`, which would drop the iron blocks
/// north of the crafter as items.)
fn crafter_facing_east() -> Block {
    let orientation = crafter::orientation_of(IVec3::X, IVec3::Y).expect("east_up is a FrontAndTop value");
    crafter::encode(crafter::State { orientation, triggered: false, crafting: false })
}

/// A chest feeds 27 iron ingots through a hopper into a crafter set to
/// craft iron blocks (all nine slots). A comparator reads the crafter; nine
/// dust steps pass only a full reading of 9 to a block beside the crafter.
///
/// Java (`CrafterBlockEntity.canPlaceItem` spreads hopper input one per
/// empty slot; `getRedstoneSignal` = filled or disabled slots; `CrafterBlock`
/// crafts 4 ticks after a rising edge and puts the result into the
/// container it faces): every ninth ingot completes the grid, the dust
/// powers the crafter, and it crafts one iron block 6 ticks after the
/// ninth ingot arrived, before the hopper's next ingot (8 ticks). Three
/// iron blocks, 72 ticks apart; ingots in iron blocks plus loose ingots
/// stay 27. Checked on the pinned 26.3 server (2026-11-23,
/// `tools/java_autocrafter_trace.py`): all four probes match tick for tick
/// over the 320 ticks, with iron blocks at 79, 151 and 223.
fn crafter_autocrafter(crafter: Block) -> Spec {
    let mut blocks = vec![
        (v(2, 1, 0), crafter),
        (v(3, 1, 0), Block::CHEST),
        (v(2, 2, 0), r::hopper(4, false)),
        (v(2, 3, 0), Block::CHEST),
        (v(1, 1, 0), r::comparator(Facing::West, false, false)),
        (v(2, 1, 1), Block::STONE),
    ];
    let dust =
        [v(0, 1, 0), v(0, 1, 1), v(0, 1, 2), v(0, 1, 3), v(0, 1, 4), v(1, 1, 4), v(2, 1, 4), v(2, 1, 3), v(2, 1, 2)];
    for p in dust {
        blocks.push((p - IVec3::Y, Block::STONE));
    }
    blocks.push((v(1, 0, 0), Block::STONE));
    for p in dust {
        blocks.push((p, r::wire(0)));
    }
    Spec {
        name: "crafter_autocrafter",
        blocks,
        contents: vec![(v(2, 3, 0), vec![Stack::new(Item::IRON_INGOT, 27)])],
        probes: vec![
            Probe::Count(v(3, 1, 0), Item::from_block(Block::IRON_BLOCK)),
            Probe::Items(v(2, 1, 0)),
            Probe::Items(v(2, 3, 0)),
            Probe::Items(v(2, 2, 0)),
            Probe::Pending,
        ],
        ticks: 320,
        unload_at: 89,
        restart_at: 151,
        motion_after: 60,
        ..Spec::default()
    }
}

fn verify_crafter(run: &Run) -> Result<(), String> {
    for (i, l) in run.lines.iter().enumerate() {
        ensure(l[0] * 9 + l[1] + l[2] + l[3] == 27, || format!("tick {}: ingots {l:?} don't add up to 27", i + 1))?;
    }
    let blocks = run.column(0);
    let made: Vec<u64> =
        blocks.windows(2).enumerate().filter(|(_, w)| w[1] > w[0]).map(|(i, _)| i as u64 + 2).collect();
    // The pinned vanilla server, stepped tick by tick
    // (`tools/java_autocrafter_trace.py`), made them at 79, 151 and 223.
    ensure(made == [79, 151, 223], || format!("iron blocks at {made:?}; Java 79, 151 and 223"))
}

#[test]
fn crafter_autocrafts() {
    check(&crafter_autocrafter(crafter_facing_east()), verify_crafter);
}

// ---------------------------------------------------------------- memory

/// A BUD switch: a piston quasi-powered by a redstone block diagonally
/// above it. The script places a block behind the piston (10), removes the
/// redstone block (40) and removes the block behind (70).
///
/// Java (`PistonBaseBlock.getNeighborSignal` also checks the neighbours of
/// the block above; `Level.updateNeighborsAt` notifies only the six direct
/// neighbours, so neither the redstone block's placement nor its removal
/// reaches the piston): the piston stays retracted until the block behind
/// it appears (extends at 10), stays extended when its power goes (40),
/// and retracts when the block behind it changes again (70).
fn bud_switch() -> Spec {
    Spec {
        name: "bud_switch",
        blocks: vec![(v(1, 1, 0), r::piston(2, false, false)), (v(1, 2, 1), r::REDSTONE_BLOCK)],
        script: |rig, m| match m {
            10 => rig.set(v(0, 1, 0), Block::STONE),
            40 => rig.set(v(1, 2, 1), Block::AIR),
            70 => rig.set(v(0, 1, 0), Block::AIR),
            _ => {}
        },
        probes: vec![Probe::Block(v(1, 1, 0)), Probe::Block(v(2, 1, 0)), Probe::Moving, Probe::Pending],
        ticks: 100,
        unload_at: 25,
        restart_at: 55,
        motion_after: 5,
        ..Spec::default()
    }
}

fn verify_bud_switch(run: &Run) -> Result<(), String> {
    let piston = run.column(0);
    let (retracted, extended) = (r::piston(2, false, false).0 as i64, r::piston(2, false, true).0 as i64);
    for m in 1..=run.lines.len() as u64 {
        let want = match m {
            1..=9 | 72.. => retracted,
            12..=69 => extended,
            _ => continue,
        };
        ensure(at(&piston, m) == want, || {
            format!(
                "tick {m}: piston {}, Java {}",
                at(&piston, m),
                if want == extended { "extended" } else { "retracted" }
            )
        })?;
    }
    Ok(())
}

#[test]
fn bud_switch_waits_for_an_update() {
    check(&bud_switch(), verify_bud_switch);
}

#[test]
fn bud_switch_survives_unload_and_restart() {
    persists(&bud_switch(), no_invariants);
}

/// A T flip-flop: two hoppers facing each other hold one diamond, locked by
/// dust on top of them fed by a torch. An input pulse (an observer seeing
/// the script toggle a block) turns the torch off for 2 ticks; the hopper
/// holding the diamond pushes it across, and the receiver's 8-tick cooldown
/// outlasts the unlock. A comparator reading the second hopper lights a lamp.
///
/// Java (`ObserverBlock` 2-tick pulse, `RedstoneTorchBlock` 2-tick delay,
/// `HopperBlock.checkPoweredState`, `HopperBlockEntity` cooldown 8 on the
/// receiver; dust may sit on hoppers, `RedStoneWireBlock.canSurviveOn`):
/// every pulse moves the diamond to the other hopper: after pulses 1 and 3
/// it is in the second hopper and the lamp is lit, after 2 and 4 it is back
/// and the lamp is dark.
fn t_flip_flop() -> Spec {
    Spec {
        name: "t_flip_flop",
        blocks: vec![
            (v(0, 1, 0), r::hopper(2, false)),
            (v(1, 1, 0), r::hopper(3, false)),
            (v(1, 0, 1), Block::STONE),
            (v(1, 1, 1), r::comparator(Facing::South, false, false)),
            (v(1, 1, 2), r::LAMP),
            (v(3, 2, 0), Block::STONE),
            (v(4, 2, 0), r::observer(2, false)),
            (v(0, 2, 0), r::wire(0)),
            (v(1, 2, 0), r::wire(0)),
            (v(2, 2, 0), r::torch(4, true)),
        ],
        contents: vec![(v(0, 1, 0), vec![Stack::new(Item::DIAMOND, 1)])],
        script: |rig, m| match m {
            10 | 70 => rig.set(v(5, 2, 0), Block::STONE),
            40 | 100 => rig.set(v(5, 2, 0), Block::AIR),
            _ => {}
        },
        probes: vec![
            Probe::Count(v(0, 1, 0), Item::DIAMOND),
            Probe::Count(v(1, 1, 0), Item::DIAMOND),
            Probe::Block(v(1, 1, 2)),
            Probe::Block(v(2, 2, 0)),
            Probe::Pending,
        ],
        closed: true,
        ticks: 130,
        unload_at: 25,
        restart_at: 55,
        motion_after: 8,
        ..Spec::default()
    }
}

fn verify_t_flip_flop(run: &Run) -> Result<(), String> {
    let lit = Block(r::LAMP.0 + 1).0 as i64;
    let (first, second, lamp) = (run.column(0), run.column(1), run.column(2));
    for (m, in_second) in [(9, false), (25, true), (55, false), (85, true), (115, false)] {
        let ok = at(&second, m) == in_second as i64
            && at(&first, m) == !in_second as i64
            && (at(&lamp, m) == lit) == in_second;
        ensure(ok, || {
            format!(
                "tick {m}: diamond {}/{} lamp {}; Java has it in hopper {} with the lamp {}",
                at(&first, m),
                at(&second, m),
                at(&lamp, m),
                if in_second { 2 } else { 1 },
                if in_second { "lit" } else { "dark" }
            )
        })?;
    }
    Ok(())
}

#[test]
#[ignore = "the build doesn't toggle in Java either: the observer's on-tick schedules its off-tick before \
            updateNeighborsInFront reaches the torch, so the off-tick runs first and a redstone torch never sees a \
            2-tick observer pulse (checked on the vanilla 26.3 server, 2026-11-23, Track A: observer -> stone -> \
            wall torch, the torch stayed lit through every pulse); it needs a longer pulse (Track F)"]
fn t_flip_flop_toggles() {
    check(&t_flip_flop(), verify_t_flip_flop);
}

#[test]
fn t_flip_flop_survives_unload_and_restart() {
    persists(&t_flip_flop(), no_invariants);
}

/// Soak: the clocks and the cane farm run ten times as long with the cuts
/// spread out, checking persistence only (the Java checks are above).
#[test]
#[ignore = "soak run: tens of seconds, not part of CI's quick pass; run with --ignored"]
fn soak_long_runs() {
    for (spec, invariants) in [
        (Spec { ticks: 2000, unload_at: 777, restart_at: 1501, ..repeater_loop() }, no_invariants as fn(&Run) -> _),
        (Spec { ticks: 1200, unload_at: 433, restart_at: 977, ..observer_clock() }, no_invariants),
        (Spec { ticks: 1200, unload_at: 433, restart_at: 977, ..comparator_clock() }, no_invariants),
        (Spec { ticks: 1600, unload_at: 611, restart_at: 1203, ..hopper_clock() }, no_invariants),
        (Spec { ticks: 4000, unload_at: 1501, restart_at: 2903, ..sugar_cane_farm() }, sugar_cane_accounting),
    ] {
        persists(&spec, invariants);
    }
}
