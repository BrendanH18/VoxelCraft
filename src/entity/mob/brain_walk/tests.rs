//! v1.0 gap Track 2e: the brains' idle movement `RunOne`s and
//! `MoveToTargetSink`.
use super::super::head::Seen;
use super::*;
use crate::entity::{Ctx, PlayerId, Rng, Target};
use crate::physics::test_util::Grid;

fn ctx(players: &[DVec3]) -> Ctx {
    Ctx {
        spawn_light: crate::simulation::lighting::SpawnLight::DAY,
        players: players.iter().enumerate().map(|(i, &p)| Target::new(PlayerId(i as u32 + 1), p, true)).collect(),
        daylight: 1.0,
        spawning: false,
        raining: false,
        dimension: crate::world::terrain::Dimension::Overworld,
    }
}

fn mob(kind: MobKind) -> Mob {
    let mut m = Mob::new(kind, DVec3::new(0.5, 64.0, 0.5), 0.0);
    m.on_ground = true;
    m
}

fn seen(uid: u32, kind: MobKind, at: DVec3) -> Seen {
    Seen {
        uid,
        kind,
        eye: at + DVec3::Y,
        pos: at,
        breedable: false,
        baby: false,
        chasing: None,
        farmer: false,
        wants_food: false,
    }
}

/// The idle `RunOne` sends a villager to a villager it sees within 8
/// (`InteractWith`, speed 0.5, 2 blocks short) and looks at it; other picks
/// stroll, follow the look target or do nothing, never the old wandering.
#[test]
fn idle_villagers_walk_to_the_villagers_they_see() {
    let w = Grid::flat(64);
    let c = ctx(&[]);
    let (mut met, mut strolled, mut waited) = (0, 0, 0);
    for seed in 0..60 {
        let mut rng = Rng::new(seed);
        let mut v = mob(MobKind::Villager);
        v.walking.in_village = true;
        v.head.seen = vec![seen(78, MobKind::Villager, DVec3::new(5.5, 64.0, 0.5))];
        v.head_goals(&w, &c, &mut rng, false);
        match v.walking.walk {
            Some(Walk { to: WalkTo::Mob(78, _), speed, close }) => {
                assert_eq!((speed, close), (0.5, 2));
                assert!(matches!(v.head.target, Some(Tracked::Mob(78, _))));
                // The core sink starts on the next tick.
                assert!(v.head.fresh_target);
                met += 1;
            }
            Some(Walk { to: WalkTo::Point(p), .. }) => {
                assert!((p - v.pos).with_y(0.0).abs().max_element() <= 10.5, "{p}");
                strolled += 1;
            }
            Some(other) => panic!("{other:?}"),
            None => waited += 1,
        }
        assert_ne!(v.ai, Ai::Wander);
    }
    assert!(met > 0 && strolled > 0 && waited > 0, "met {met}, strolled {strolled}, waited {waited}");
}

/// `MoveToTargetSink`: a reached walk target is erased (`distManhattan` to
/// the target block within `closeEnoughDist`), and a set one is walked.
#[test]
fn the_walk_sink_walks_then_erases_its_target() {
    let w = Grid::flat(64);
    let c = ctx(&[]);
    let mut rng = Rng::new(3);
    let mut v = mob(MobKind::Villager);
    v.set_walk(Some(Walk { to: WalkTo::Point(DVec3::new(1.5, 64.0, 0.5)), speed: 0.5, close: 1 }));
    assert!(v.walk_sink(&w, &c, &mut rng, (150, 250)).is_none());
    assert!(v.walking.walk.is_none(), "one block off is close enough");
    v.set_walk(Some(Walk { to: WalkTo::Point(DVec3::new(8.5, 64.0, 0.5)), speed: 0.5, close: 0 }));
    let (dir, speed) = v.walk_sink(&w, &c, &mut rng, (150, 250)).expect("it walks");
    assert!(dir.is_some_and(|d| d.x > 0.5), "{dir:?}");
    assert!((speed - v.goal_speed(0.5)).abs() < 1e-9);
}

/// A piglin's idle movement: strolls within 10 blocks, walks to piglins it
/// sees, or waits; a celebrating piglin strolls only within 2.
#[test]
fn piglins_stroll_and_celebrate_close_by() {
    let w = Grid::flat(64);
    let c = ctx(&[]);
    let mut kinds = (0, 0);
    for seed in 0..80 {
        let mut rng = Rng::new(seed);
        let mut p = mob(MobKind::Piglin);
        p.head.seen = vec![seen(90, MobKind::Piglin, DVec3::new(4.5, 64.0, 0.5))];
        for _ in 0..3 {
            p.head_goals(&w, &c, &mut rng, false);
        }
        match p.walking.walk {
            // `InteractWith` (2 short) or `SetWalkTargetFromLookTarget` (3).
            Some(Walk { to: WalkTo::Mob(90, _), speed, close }) => {
                assert!(speed == 0.6 && (2..=3).contains(&close), "{speed} {close}");
                kinds.0 += 1;
            }
            Some(Walk { to: WalkTo::Point(at), speed, .. }) => {
                assert_eq!(speed, 0.6);
                assert!((at - p.pos).with_y(0.0).abs().max_element() <= 10.5);
                kinds.1 += 1;
            }
            _ => {}
        }
        let mut q = mob(MobKind::Piglin);
        q.nether.as_mut().unwrap().celebrate_left = 10.0;
        for _ in 0..40 {
            q.head_goals(&w, &c, &mut rng, false);
            if let Some(Walk { to: WalkTo::Point(at), .. }) = q.walking.walk {
                assert!((at - q.pos).with_y(0.0).abs().max_element() <= 2.5, "{at}");
                q.walking.walk = None;
            }
        }
    }
    assert!(kinds.0 > 0 && kinds.1 > 0, "{kinds:?}");
}

/// The sniffer's idle `RunOne` starts its scent and its sniff (the sniff
/// waits out `SNIFF_COOLDOWN`; the scent doesn't) and holds while they run.
#[test]
fn the_sniffers_run_one_starts_scents_and_sniffs() {
    use crate::entity::sniffer::{SCENTING, SNIFFING};
    let w = Grid::flat(64);
    let c = ctx(&[]);
    let mut rng = Rng::new(9);
    let (mut scents, mut sniffs) = (0, 0);
    for cooldown in [0, 9000] {
        let mut s = mob(MobKind::Sniffer);
        s.sniffer.as_mut().unwrap().cooldown = cooldown;
        for _ in 0..400 {
            s.head_goals(&w, &c, &mut rng, false);
            s.walking.walk = None;
            let st = s.sniffer.as_mut().unwrap();
            match st.pose {
                SCENTING => {
                    scents += 1;
                    st.stop_early();
                }
                SNIFFING => {
                    assert_eq!(cooldown, 0, "no sniff during the cooldown");
                    sniffs += 1;
                    st.stop_early();
                }
                _ => {}
            }
        }
    }
    assert!(scents > 0 && sniffs > 0, "scents {scents}, sniffs {sniffs}");
}

/// `JumpOnBed`: a baby villager at play near a bed walks to it and jumps on
/// it three to six times, five ticks apart.
#[test]
fn baby_villagers_jump_on_beds() {
    let mut w = Grid::flat(64);
    let bed = IVec3::new(0, 64, 0);
    w.set(bed, Block::colored_bed(crate::color::DyeColor::Blue, true));
    let c = ctx(&[]);
    let mut jumped = false;
    for seed in 0..60 {
        let mut rng = Rng::new(seed);
        let mut v = mob(MobKind::Villager);
        v.age = -24000;
        v.baby = true;
        v.villager.as_mut().unwrap().activity = crate::entity::villager::Activity::Play;
        v.walking.nearest_bed = Some(bed);
        let mut ticks = 0;
        while !matches!(v.walking.hold, Some(Hold::JumpOnBed(_))) && ticks < 400 {
            v.head_goals(&w, &c, &mut rng, false);
            v.walking.walk = None;
            ticks += 1;
        }
        let mut jumps = 0;
        while matches!(v.walking.hold, Some(Hold::JumpOnBed(_))) {
            v.head_goals(&w, &c, &mut rng, false);
            if std::mem::take(&mut v.jump) {
                jumps += 1;
            }
        }
        if jumps > 0 {
            assert!((3..=6).contains(&jumps), "{jumps}");
            jumped = true;
        }
    }
    assert!(jumped);
}

fn worker(job: IVec3, profession: crate::entity::villager::Profession) -> Mob {
    let mut v = mob(MobKind::Villager);
    let s = v.villager.as_mut().unwrap();
    s.activity = crate::entity::villager::Activity::Work;
    s.job = Some(job);
    s.set_profession(profession);
    v
}

/// v1.0 gap Track 2f: the work package. Farther than 9 blocks
/// (`distManhattan`) from its job site, `SetWalkTargetFromBlockMemory` walks
/// a villager back (0.5, 9 short); within 10 its `RunOne` strolls to the
/// job site (`StrollToPoi`, 0.4, 1 short) and within 4 around it
/// (`StrollAroundPoi`, a land spot within 8 and 6), never the old wandering.
#[test]
fn working_villagers_stroll_to_and_around_their_job_site() {
    use crate::entity::villager::Profession;
    let w = Grid::flat(64);
    let c = ctx(&[]);
    let job = IVec3::new(0, 64, 3);
    let mut v = worker(job, Profession::Librarian);
    v.pos = DVec3::new(15.5, 64.0, 3.5);
    let mut rng = Rng::new(3);
    v.head_goals(&w, &c, &mut rng, false);
    assert_eq!(v.walking.walk, Some(Walk { to: WalkTo::Point(DVec3::new(0.5, 64.0, 3.5)), speed: 0.5, close: 9 }));
    let (mut to_job, mut around) = (0, 0);
    for seed in 0..80 {
        let mut rng = Rng::new(seed);
        let mut v = worker(job, Profession::Librarian);
        v.pos = DVec3::new(if seed % 2 == 0 { 2.5 } else { 6.5 }, 64.0, 3.5);
        for _ in 0..5 {
            v.head_goals(&w, &c, &mut rng, false);
        }
        match v.walking.walk {
            Some(Walk { to: WalkTo::Point(p), speed, close }) if p == DVec3::new(0.5, 64.0, 3.5) => {
                assert_eq!((speed, close), (0.4, 1));
                to_job += 1;
            }
            Some(Walk { to: WalkTo::Point(p), speed, close }) => {
                assert_eq!((speed, close), (0.4, 1), "{p}");
                assert!(seed % 2 == 0, "StrollAroundPoi only within 4 of the job site");
                around += 1;
            }
            Some(other) => panic!("{other:?}"),
            None => {}
        }
        assert_ne!(v.ai, Ai::Wander);
    }
    assert!(to_job > 0 && around > 0, "to the job site {to_job}, around it {around}");
}

/// `WorkAtPoi`: within 1.73 of its job site's centre, every 300 ticks on a
/// coin flip, a villager works it for 60 ticks: it looks at it and the
/// village pass restocks.
#[test]
fn villagers_work_their_job_site_for_sixty_ticks() {
    use crate::entity::villager::Profession;
    let w = Grid::flat(64);
    let c = ctx(&[]);
    let job = IVec3::new(1, 64, 0);
    let mut rng = Rng::new(11);
    let mut v = worker(job, Profession::Librarian);
    let mut ticks = 0;
    while !matches!(v.walking.hold, Some(Hold::Work(_))) && ticks < 400 {
        v.head_goals(&w, &c, &mut rng, false);
        v.walking.walk = None;
        ticks += 1;
    }
    assert!(matches!(v.walking.hold, Some(Hold::Work(60))), "{:?}", v.walking.hold);
    assert!(std::mem::take(&mut v.walking.work_due));
    assert_eq!(v.head.target, Some(Tracked::Point(job.as_dvec3() + 0.5)));
    let mut held = 0;
    while matches!(v.walking.hold, Some(Hold::Work(_))) && held < 100 {
        v.head_goals(&w, &c, &mut rng, false);
        v.walking.walk = None;
        held += 1;
    }
    assert!((59..=62).contains(&held), "{held}");
    // Moving off the job site stops it.
    v.walking.hold = Some(Hold::Work(30));
    v.pos.x = 5.5;
    v.head_goals(&w, &c, &mut rng, false);
    assert!(!matches!(v.walking.hold, Some(Hold::Work(_))));
}

/// `HarvestFarmland`: a farmer at work with farmland near it walks to a
/// ripe crop or empty farmland within a block of it (0.5, 1 short) and
/// breaks the ripe crop once within 1 of its centre; `mobGriefing` off,
/// it doesn't start.
#[test]
fn farmers_harvest_the_ripe_crops_around_them() {
    use crate::entity::villager::Profession;
    let mut w = Grid::flat(64);
    let job = IVec3::new(4, 64, 4);
    w.set(job, Block::COMPOSTER);
    let ripe = IVec3::new(1, 64, 0);
    w.set(ripe - IVec3::Y, Block::FARMLAND);
    w.set(ripe, Block::wheat(7));
    let c = ctx(&[]);
    let mut harvested = false;
    for seed in 0..40 {
        let mut rng = Rng::new(seed);
        let mut v = worker(job, Profession::Farmer);
        v.pos = DVec3::new(1.5, 64.0, 0.5);
        for _ in 0..200 {
            v.head_goals(&w, &c, &mut rng, false);
            for act in std::mem::take(&mut v.walking.farm_acts) {
                assert_eq!(act, FarmAct::Harvest(ripe));
                harvested = true;
            }
            if let (Some(Hold::Harvest(_)), Some(walk)) = (v.walking.hold, v.walking.walk) {
                assert_eq!((walk.speed, walk.close), (0.5, 1));
            }
            // A walk the sink would finish.
            v.walking.walk = None;
        }
    }
    assert!(harvested);
    let mut rng = Rng::new(1);
    let mut v = worker(job, Profession::Farmer);
    v.pos = DVec3::new(1.5, 64.0, 0.5);
    v.griefing = false;
    for _ in 0..200 {
        v.head_goals(&w, &c, &mut rng, false);
        assert!(!matches!(v.walking.hold, Some(Hold::Harvest(_))));
        v.walking.walk = None;
    }
}

/// Track 2g: `HarvestFarmland` plants the first `VILLAGER_PLANTABLE_SEEDS`
/// stack on empty farmland (here a potato after the bone meal, which isn't
/// a seed), one at a time; without seeds it plants nothing.
#[test]
fn farmers_plant_seeds_from_their_inventory() {
    use crate::entity::villager::Profession;
    let mut w = Grid::flat(64);
    let job = IVec3::new(4, 64, 4);
    w.set(job, Block::COMPOSTER);
    let empty = IVec3::new(1, 64, 0);
    w.set(empty - IVec3::Y, Block::FARMLAND);
    let c = ctx(&[]);
    let mut planted = 0;
    for seed in 0..40 {
        let mut rng = Rng::new(seed);
        let mut v = worker(job, Profession::Farmer);
        v.pos = DVec3::new(1.5, 64.0, 0.5);
        let inv = v.villager.as_mut().unwrap();
        inv.accept_food(Stack::new(Item::BONE_MEAL, 3));
        inv.accept_food(Stack::new(Item::POTATO, 2));
        for _ in 0..200 {
            v.head_goals(&w, &c, &mut rng, false);
            for act in std::mem::take(&mut v.walking.farm_acts) {
                if let FarmAct::Plant(cell, block) = act {
                    assert_eq!((cell, block), (empty, Block::crop(crate::world::block::Crop::Potato, 0)));
                    planted += 1;
                }
            }
            v.walking.walk = None;
        }
        // It never plants more than it carries.
        assert!(v.villager.as_ref().unwrap().count_item(Item::POTATO) <= 2);
    }
    assert!(planted > 0);
}

/// Track 2g: `WorkAtComposter`: a farmer starting `WorkAtPoi` at its
/// composter bakes up to three bread from three wheat each and composts
/// the wheat and beetroot seeds beyond ten of each, at most 20, stopping at
/// level 7; a full composter is emptied first, its bone meal dropped.
#[test]
fn farmers_bake_bread_and_compost_their_spare_seeds() {
    use crate::entity::villager::Profession;
    let mut w = Grid::flat(64);
    let job = IVec3::new(0, 64, 0);
    let c = ctx(&[]);
    for (start, emptied) in [(crate::world::composter::state(0), false), (crate::world::composter::state(8), true)] {
        w.set(job, start);
        let mut done = false;
        for seed in 0..20 {
            let mut rng = Rng::new(seed);
            let mut v = worker(job, Profession::Farmer);
            v.pos = DVec3::new(0.5, 64.0, 1.5);
            let inv = v.villager.as_mut().unwrap();
            inv.accept_food(Stack::new(Item::WHEAT, 10));
            inv.accept_food(Stack::new(Item::WHEAT_SEEDS, 40));
            for _ in 0..400 {
                v.head_goals(&w, &c, &mut rng, false);
                let acts = std::mem::take(&mut v.walking.farm_acts);
                if let Some(FarmAct::Compost { cell, from, to }) =
                    acts.iter().find(|a| matches!(a, FarmAct::Compost { .. })).copied()
                {
                    assert_eq!((cell, from), (job, if emptied { 8 } else { 0 }));
                    assert!((1..=7).contains(&to));
                    // Track 2h: `WorkAtPoi.start` played the farmer's work sound.
                    assert!(acts.iter().any(|a| matches!(a, FarmAct::WorkSound(Profession::Farmer))));
                    let bone_meal =
                        acts.iter().any(|a| matches!(a, FarmAct::Item(s, _, _) if s.item == Item::BONE_MEAL));
                    assert_eq!(bone_meal, emptied);
                    let v = v.villager.as_ref().unwrap();
                    assert_eq!((v.count_item(Item::BREAD), v.count_item(Item::WHEAT)), (3, 1));
                    // Seeds go in one at a time until level 7 or 20 are used;
                    // ten or more always stay.
                    let used = 40 - v.count_item(Item::WHEAT_SEEDS);
                    assert!((1..=20).contains(&used) && (to == 7 || used == 20), "{used} {to}");
                    done = true;
                    break;
                }
                v.walking.walk = None;
            }
            if done {
                break;
            }
        }
        assert!(done);
    }
}

/// Track 2g: `UseBonemeal`: a villager with bone meal walks to a growing
/// crop round it (0.5, 1 short) and uses one on it, 40 ticks apart; with
/// none, it never starts.
#[test]
fn farmers_use_bone_meal_on_growing_crops() {
    use crate::entity::villager::Profession;
    let mut w = Grid::flat(64);
    let job = IVec3::new(4, 64, 4);
    w.set(job, Block::COMPOSTER);
    let crop = IVec3::new(1, 64, 0);
    w.set(crop - IVec3::Y, Block::FARMLAND);
    w.set(crop, Block::wheat(2));
    let c = ctx(&[]);
    let mut used = 0;
    for seed in 0..40 {
        let mut rng = Rng::new(seed);
        let mut v = worker(job, Profession::Farmer);
        v.pos = DVec3::new(1.5, 64.0, 0.5);
        v.villager.as_mut().unwrap().accept_food(Stack::new(Item::BONE_MEAL, 4));
        let mut last = None;
        let mut here = 0;
        for t in 0..400 {
            v.head_goals(&w, &c, &mut rng, false);
            for act in std::mem::take(&mut v.walking.farm_acts) {
                if let FarmAct::Bonemeal(cell) = act {
                    assert_eq!(cell, crop);
                    assert!(v.using_bone_meal());
                    if let Some(l) = last {
                        assert!(t - l >= 40, "{t} {l}");
                    }
                    last = Some(t);
                    used += 1;
                    here += 1;
                }
            }
            v.walking.walk = None;
        }
        assert_eq!(v.villager.as_ref().unwrap().count_item(Item::BONE_MEAL) + here, 4);
    }
    assert!(used > 0);
    let mut rng = Rng::new(3);
    let mut v = worker(job, Profession::Farmer);
    v.pos = DVec3::new(1.5, 64.0, 0.5);
    for _ in 0..400 {
        v.head_goals(&w, &c, &mut rng, false);
        assert!(!v.using_bone_meal());
        v.walking.walk = None;
    }
}

/// Track 2h: `WorkAtPoi.start` plays the profession's work sound
/// (`playWorkSound`) each time it starts; a farmer with nothing to compost
/// still ends `compostItems` with the composter's fill effect (level event
/// 1500 with no layer added).
#[test]
fn villagers_play_their_work_sound_and_farmers_always_fill() {
    use crate::entity::villager::Profession;
    let mut w = Grid::flat(64);
    let job = IVec3::new(0, 64, 0);
    let c = ctx(&[]);
    for job_kind in [Profession::Librarian, Profession::Farmer] {
        w.set(job, if job_kind == Profession::Farmer { crate::world::composter::state(3) } else { Block::STONE });
        let mut sounds = 0;
        for seed in 0..20 {
            let mut rng = Rng::new(seed);
            let mut v = worker(job, job_kind);
            v.pos = DVec3::new(0.5, 64.0, 1.5);
            for _ in 0..400 {
                v.head_goals(&w, &c, &mut rng, false);
                let acts = std::mem::take(&mut v.walking.farm_acts);
                if acts.iter().any(|a| matches!(a, FarmAct::WorkSound(j) if *j == job_kind)) {
                    sounds += 1;
                    let fill = acts.iter().find(|a| matches!(a, FarmAct::Compost { .. })).copied();
                    if job_kind == Profession::Farmer {
                        assert!(matches!(fill, Some(FarmAct::Compost { from: 3, to: 3, .. })), "{fill:?}");
                    } else {
                        assert!(fill.is_none());
                    }
                }
                v.walking.walk = None;
            }
        }
        assert!(sounds > 0, "{job_kind:?}");
    }
    assert_eq!(Profession::Nitwit.work_sound(), None);
    assert_eq!(Profession::Cleric.work_sound(), Some("entity.villager.work_cleric"));
}

/// Track 2h: torchflower crops are `CropBlock`s that never reach their
/// maximum age (2 is the torchflower): `UseBonemeal` picks them and
/// `HarvestFarmland` never breaks them.
#[test]
fn farmers_bone_meal_torchflower_crops_but_never_harvest_them() {
    use crate::entity::villager::Profession;
    use crate::world::sniffer_plants::TORCH_CROP;
    let mut w = Grid::flat(64);
    let job = IVec3::new(4, 64, 4);
    w.set(job, Block::COMPOSTER);
    let crop = IVec3::new(1, 64, 0);
    w.set(crop - IVec3::Y, Block::FARMLAND);
    for age in 0..2 {
        let b = Block(TORCH_CROP.0 + age);
        w.set(crop, b);
        assert!(growing(b) && !harvestable(&w, crop));
    }
    assert!(!growing(crate::world::sniffer_plants::TORCHFLOWER));
    let c = ctx(&[]);
    let mut used = 0;
    for seed in 0..40 {
        let mut rng = Rng::new(seed);
        let mut v = worker(job, Profession::Farmer);
        v.pos = DVec3::new(1.5, 64.0, 0.5);
        v.villager.as_mut().unwrap().accept_food(Stack::new(Item::BONE_MEAL, 4));
        for _ in 0..400 {
            v.head_goals(&w, &c, &mut rng, false);
            for act in std::mem::take(&mut v.walking.farm_acts) {
                assert!(!matches!(act, FarmAct::Harvest(_)), "{act:?}");
                if act == FarmAct::Bonemeal(crop) {
                    used += 1;
                }
            }
            v.walking.walk = None;
        }
    }
    assert!(used > 0);
}

/// Track 2g: `TradeWithVillager`'s throws while the partner is within
/// `sqrt(5)`: a farmer throws half its excess food and wheat beyond half a
/// stack; a villager throws a farmer partner the seeds beyond 24 it
/// requests, 0.3 a tick from 0.3 below its eyes toward the partner.
#[test]
fn trading_villagers_throw_food_wheat_and_seeds() {
    use crate::entity::villager::Profession;
    let at = DVec3::new(1.5, 64.0, 0.5);
    let mut v = worker(IVec3::new(9, 64, 9), Profession::Farmer);
    v.head.seen = vec![Seen { wants_food: false, ..seen(78, MobKind::Villager, at) }];
    v.walking.interaction = Some(78);
    let inv = v.villager.as_mut().unwrap();
    inv.accept_food(Stack::new(Item::CARROT, 40));
    inv.accept_food(Stack::new(Item::WHEAT, 40));
    v.trade_gate();
    let thrown: Vec<(Item, u8)> = v
        .walking
        .farm_acts
        .iter()
        .map(|a| match a {
            FarmAct::Item(s, from, vel) => {
                assert!((from.y - (64.0 + v.eye_height() - 0.3)).abs() < 1e-9);
                assert!((vel.length() - 6.0).abs() < 1e-9 && vel.x > 0.0);
                (s.item, s.count)
            }
            other => panic!("{other:?}"),
        })
        .collect();
    assert_eq!(thrown, [(Item::CARROT, 20), (Item::WHEAT, 20)]);
    // A non-farmer keeps its food from a partner that has enough, and
    // throws a farmer the seeds it requests.
    let mut v = worker(IVec3::new(9, 64, 9), Profession::Mason);
    v.head.seen = vec![Seen { farmer: true, wants_food: false, ..seen(78, MobKind::Villager, at) }];
    v.walking.interaction = Some(78);
    let inv = v.villager.as_mut().unwrap();
    inv.accept_food(Stack::new(Item::CARROT, 40));
    inv.accept_food(Stack::new(Item::WHEAT_SEEDS, 30));
    v.trade_gate();
    let thrown: Vec<(Item, u8)> = v
        .walking
        .farm_acts
        .iter()
        .filter_map(|a| if let FarmAct::Item(s, ..) = a { Some((s.item, s.count)) } else { None })
        .collect();
    assert_eq!(thrown, [(Item::WHEAT_SEEDS, 6)]);
}

fn baby_seen(uid: u32, at: DVec3, chasing: Option<u32>) -> Seen {
    Seen { baby: true, chasing, ..seen(uid, MobKind::Villager, at) }
}

/// `PlayTagWithOtherKids`: with babies in sight, one tick in ten a baby
/// chases the least-chased one (or the closest), 0.6 and 1 short, as its
/// `INTERACTION_TARGET`; chased itself, it runs off to a spot in the
/// village. The play `RunOne` stays off while babies are in sight.
#[test]
fn playing_babies_chase_each_other_and_run_off() {
    let w = Grid::flat(64);
    let c = ctx(&[]);
    let baby = |seen: Vec<Seen>| {
        let mut v = mob(MobKind::Villager);
        v.age = -24000;
        v.baby = true;
        v.uid = 5;
        v.villager.as_mut().unwrap().activity = crate::entity::villager::Activity::Play;
        v.head.seen = seen;
        v
    };
    let mut rng = Rng::new(7);
    // 81 is chased by 82; 83 by nobody: the chase picks 81.
    let mut v = baby(vec![
        baby_seen(83, DVec3::new(2.5, 64.0, 0.5), None),
        baby_seen(81, DVec3::new(4.5, 64.0, 0.5), None),
        baby_seen(82, DVec3::new(6.5, 64.0, 0.5), Some(81)),
    ]);
    let mut ticks = 0;
    while v.walking.walk.is_none() && ticks < 200 {
        v.head_goals(&w, &c, &mut rng, false);
        assert!(v.walking.hold.is_none(), "the play RunOne waits while babies are seen");
        ticks += 1;
    }
    assert!(matches!(v.walking.walk, Some(Walk { to: WalkTo::Mob(81, _), speed: 0.6, close: 1 })));
    assert_eq!(v.walking.interaction, Some(81));
    assert!(matches!(v.head.target, Some(Tracked::Mob(81, _))));
    // Chased, it runs off within the village.
    let mut v = baby(vec![baby_seen(83, DVec3::new(2.5, 64.0, 0.5), Some(5))]);
    v.walking.villages = vec![IVec3::new(0, 4, 0)];
    let mut ticks = 0;
    while v.walking.walk.is_none() && ticks < 400 {
        v.head_goals(&w, &c, &mut rng, false);
        ticks += 1;
    }
    let Some(Walk { to: WalkTo::Point(p), speed, close }) = v.walking.walk else { panic!("{:?}", v.walking.walk) };
    assert_eq!((speed, close), (0.6, 0));
    assert!((p - v.pos).with_y(0.0).abs().max_element() <= 21.0);
    assert_eq!(v.walking.interaction, None);
}

/// `TradeWithVillager`: a villager whose `INTERACTION_TARGET` is a villager
/// it sees walks to it and looks at it (0.5, 2 short), asks the sense pass
/// to turn the partner to it too, and after its 60 ticks the gate erases
/// the target.
#[test]
fn trading_villagers_walk_to_each_other() {
    let w = Grid::flat(64);
    let mut e = crate::entity::Entities::new(9);
    e.spawn(MobKind::Villager, DVec3::new(0.5, 64.0, 0.5));
    e.spawn(MobKind::Villager, DVec3::new(6.5, 64.0, 0.5));
    e.ensure_mob_ids();
    let (a, b) = (e.mobs[0].uid, e.mobs[1].uid);
    let partner = e.mobs[1].pos;
    let m = &mut e.mobs[0];
    m.head.seen = vec![seen(b, MobKind::Villager, partner)];
    m.walking.interaction = Some(b);
    m.trade_gate();
    assert!(matches!(m.walking.walk, Some(Walk { to: WalkTo::Mob(u, _), speed: 0.5, close: 2 }) if u == b));
    assert!(matches!(m.head.target, Some(Tracked::Mob(u, _)) if u == b));
    assert_eq!(m.walking.lock_with, Some(b));
    e.sense_looks(&w);
    // (The sense pass rescans what it sees; this test keeps the partner seen.)
    e.mobs[0].head.seen = vec![seen(b, MobKind::Villager, partner)];
    let p = &e.mobs[1];
    assert!(matches!(p.walking.walk, Some(Walk { to: WalkTo::Mob(u, _), speed: 0.5, close: 2 }) if u == a));
    assert!(matches!(p.head.target, Some(Tracked::Mob(u, _)) if u == a));
    let m = &mut e.mobs[0];
    let mut ran = 1;
    while m.walking.interaction.is_some() && ran < 100 {
        m.trade_gate();
        ran += 1;
    }
    assert_eq!(ran, 62, "61 ticks, then it stops");
    assert_eq!(m.walking.trading_villager, 0);
    // A target it doesn't see is erased at once.
    m.walking.interaction = Some(b);
    m.head.seen.clear();
    m.trade_gate();
    assert_eq!(m.walking.interaction, None);
}

/// v1.0 gap Track 2f: the brute's idle movement `RunOne`: within 100 of its
/// `HOME` it strolls back to it (`StrollToPoi`, 0.6, 2 short) among its
/// random strolls and waits, never the old wandering.
#[test]
fn brutes_stroll_back_to_their_bastion_home() {
    let w = Grid::flat(64);
    let c = ctx(&[]);
    let mut homeward = 0;
    for seed in 0..40 {
        let mut rng = Rng::new(seed);
        let mut b = mob(MobKind::PiglinBrute);
        b.pos = DVec3::new(20.5, 64.0, 0.5);
        b.nether.as_mut().unwrap().home = Some(DVec3::new(0.5, 64.0, 0.5));
        for _ in 0..10 {
            b.head_goals(&w, &c, &mut rng, false);
            if let Some(Walk { to: WalkTo::Point(p), speed, close }) = b.walking.walk
                && p == DVec3::new(0.5, 64.0, 0.5)
            {
                assert_eq!((speed, close), (0.6, 2));
                homeward += 1;
                break;
            }
            b.walking.walk = None;
        }
        assert_ne!(b.ai, Ai::Wander);
    }
    assert!(homeward > 0);
}
