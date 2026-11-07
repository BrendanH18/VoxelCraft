use super::*;

#[test]
fn placement_matches_java_random_spread() {
    // Independently calculated with a Python 48-bit java.util.Random
    // (seed 12345, salt 94251327, spacing 34, separation 12); the same
    // oracle reproduces the trail ruin vectors.
    let t = TrialChambers::new(12345);
    for (r, c) in [((0, 0), (0, 14)), ((-1, 2), (-19, 79)), ((5, -7), (190, -221)), ((-13, 21), (-427, 733))] {
        assert_eq!(t.start_chunk(IVec2::new(r.0, r.1)), IVec2::new(c.0, c.1));
    }
}

#[test]
fn templates_parse_and_every_alias_resolves() {
    let lib = jigsaw::trial_chambers();
    assert!(lib.templates.len() >= 20);
    for name in &lib.loot {
        assert!(Table::from_name(name).is_some(), "unknown table {name}");
    }
    let aliases = Aliases { ranged: 2, melee: 1, small_melee: 3 };
    for name in &lib.mobs {
        let alias = name.strip_prefix("trial:").expect("trial spawner marker");
        let config = aliases.config(alias, IVec3::ZERO, 1).unwrap();
        assert!(super::super::trial::spawner::config_id(config).is_some(), "{config}");
    }
}

#[test]
fn a_generation_3_chamber_assembles_paints_and_registers_its_blocks() {
    use super::super::chunk::index;
    let g = Generator::new(12345);
    let at = g.trial_chambers.nearest(&g, IVec2::ZERO, 6400).expect("a chamber within 6400 blocks");
    let region = IVec2::new(at.x, at.z).div_euclid(IVec2::splat(16 * SPACING));
    let c = g.trial_chambers.get(&g, region).unwrap();
    eprintln!("trial chamber at {at}, {} pieces", c.pieces.len());
    assert!(c.pieces.len() >= 6, "{}", c.pieces.len());
    let start = c.pieces[0];
    assert!((-41..=-21).contains(&start.origin.y), "start floor {}", start.origin.y);
    let b = c.bounds();
    assert!((b.max - b.min).max_element() <= 2 * MAX_DISTANCE + 32);
    // Every piece keeps clear of its neighbours.
    for (i, p) in c.pieces.iter().enumerate() {
        for q in &c.pieces[i + 1..] {
            assert!(!p.bounds.intersects(&q.bounds), "{p:?} {q:?}");
        }
    }
    // Paint every chunk the chamber touches; spawners, vaults and loot
    // containers must be where their features say.
    let min = b.min.div_euclid(IVec3::splat(CHUNK_SIZE_I));
    let max = b.max.div_euclid(IVec3::splat(CHUNK_SIZE_I));
    let (mut spawners, mut vaults, mut loot) = (0, 0, 0);
    for cz in min.z..=max.z {
        for cy in min.y..=max.y {
            for cx in min.x..=max.x {
                let cpos = IVec3::new(cx, cy, cz);
                let base = cpos * CHUNK_SIZE_I;
                let mut blocks = Box::new([Block::STONE; CHUNK_VOLUME]);
                g.trial_chambers.paint(&g, &mut blocks, base);
                for (p, f) in g.trial_chambers.features(&g, cpos) {
                    let l = p - base;
                    let block = blocks[index(l.x as usize, l.y as usize, l.z as usize)];
                    match f {
                        Feature::TrialSpawner(_) => {
                            assert!(super::super::trial::spawner(block).is_some(), "{p}: {block:?}");
                            spawners += 1;
                        }
                        Feature::TrialVault => {
                            assert!(super::super::trial::vault(block).is_some(), "{p}: {block:?}");
                            vaults += 1;
                        }
                        Feature::TrialLoot(_, _) => {
                            assert!(super::super::chest::is_chest(block), "{p}: {block:?}");
                            loot += 1;
                        }
                        _ => {}
                    }
                }
            }
        }
    }
    assert!(spawners >= 2 && vaults >= 2 && loot >= 2, "{spawners} spawners, {vaults} vaults, {loot} containers");
    assert!(g.trial_chambers.holds(&g, start.world(IVec3::new(7, 2, 7))));
}

#[test]
fn older_generations_have_no_trial_chambers() {
    use super::super::terrain::Generation;
    for generation in [Generation::V1, Generation::V2] {
        let g = Generator::with_generation(12345, Dimension::Overworld, generation);
        assert!(g.trial_chambers.nearest(&g, IVec2::ZERO, 6400).is_none());
        assert!(g.trial_chambers.features(&g, IVec3::new(0, -1, 0)).is_empty());
    }
}

#[test]
fn bulbs_weather_by_java_rule_order() {
    // Every waxed bulb comes out lit, in one of the four stages.
    let piece = jigsaw::single(jigsaw::trial_chambers(), "trial_chambers/hallway/straight", IVec3::ZERO, 0, 1);
    let bulb = Block::from_name("waxed_copper_bulb").unwrap();
    let mut stages = std::collections::BTreeSet::new();
    for x in 0..64 {
        let out = ChamberPalette.process(bulb, IVec3::new(x, -30, x * 7), &piece).unwrap();
        let (stage, lit, powered) = super::super::copper::bulb(out).unwrap();
        assert!(lit && !powered);
        stages.insert(stage);
    }
    assert_eq!(stages.len(), 4, "{stages:?}");
}
