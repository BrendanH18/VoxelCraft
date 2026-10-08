use super::*;

#[test]
fn java_delay_and_replacement_rules() {
    assert_eq!(Situation::Menu.rules(), Rules { min: 20, max: 600, replace: true });
    assert_eq!(Situation::Game.rules(), Rules { min: 12000, max: 24000, replace: false });
    assert_eq!(Situation::End.rules(), Rules { min: 6000, max: 24000, replace: true });
    for s in [Situation::Dragon, Situation::Credits] {
        assert_eq!(s.rules().max, 0);
    }
    for seed in 0..64 {
        let mut m = Manager::new(seed);
        for _ in 0..100 {
            assert_eq!(m.tick(Situation::Game, false), Action::None);
        }
        assert_eq!(m.tick(Situation::Game, false), Action::Start(Situation::Game));
        assert_eq!(m.tick(Situation::Creative, false), Action::None);
        assert_eq!(m.current, Some(Situation::Game));
        m.tick(Situation::Creative, true);
        assert!((11999..=23999).contains(&m.delay), "delay {}", m.delay);
        assert_eq!(m.tick(Situation::Dragon, false), Action::Start(Situation::Dragon));
        assert_eq!(m.tick(Situation::Credits, false), Action::Replace(Situation::Credits));
        let action = m.tick(Situation::Menu, false);
        assert!(matches!(action, Action::Stop | Action::Replace(Situation::Menu)));
        if action == Action::Stop {
            assert!(m.delay <= 9);
        }
    }
}

#[test]
fn completion_uses_the_new_situations_range_and_clamps_existing_delay() {
    let mut m = Manager::new(5);
    assert_eq!(m.tick(Situation::Dragon, false), Action::Start(Situation::Dragon));
    // End replacing Dragon schedules 0..3000, rather than 6000..24000.
    assert_eq!(m.tick(Situation::End, false), Action::Stop);
    assert!(m.delay < 3000);
    m.current = None;
    m.delay = 24000;
    m.tick(Situation::Menu, false);
    assert_eq!(m.delay, 599);
    m.current = Some(Situation::Game);
    m.delay = u32::MAX;
    m.tick(Situation::NetherWastes, true);
    assert!((11999..=23999).contains(&m.delay));
}

#[test]
fn situational_priority_and_underwater_latching() {
    let mut c = Context::default();
    assert_eq!(c.select(None), Situation::Menu);
    c.credits = true;
    assert_eq!(c.select(None), Situation::Credits);
    c.credits = false;
    c.title = false;
    c.creative = true;
    assert_eq!(c.select(None), Situation::Creative);
    c.underwater = true;
    c.biome = Biome::River;
    assert_eq!(c.select(None), Situation::Creative, "rivers do not have the ocean music tag");
    c.biome = Biome::Ocean;
    assert_eq!(c.select(None), Situation::Underwater);
    c.underwater = false;
    assert_eq!(c.select(Some(Situation::Underwater)), Situation::Underwater);
    c.dimension = Dimension::Nether;
    for s in [
        Situation::NetherWastes,
        Situation::CrimsonForest,
        Situation::WarpedForest,
        Situation::SoulSandValley,
        Situation::BasaltDeltas,
    ] {
        c.nether = s;
        assert_eq!(c.select(Some(Situation::Underwater)), s);
    }
    c.dimension = Dimension::End;
    assert_eq!(c.select(None), Situation::End);
    c.dragon = true;
    assert_eq!(c.select(None), Situation::Dragon);
}

#[test]
fn composition_is_seeded_and_block_size_independent() {
    let render = |seed, variant, block_size| {
        let mut piece = Composition::new(Situation::Game, seed, variant);
        let mut out = vec![[0.0; 2]; RATE as usize * 16];
        for block in out.chunks_mut(block_size) {
            piece.render(block);
        }
        out
    };
    let a = render(42, 0, 512);
    assert_eq!(a, render(42, 0, 137));
    assert_ne!(a, render(43, 0, 512));
    assert_ne!(a, render(42, 1, 512));
}

#[test]
fn all_situations_have_safe_finite_levels_and_clean_boundaries() {
    for situation in Situation::ALL {
        for variant in 0..3 {
            let mut piece = Composition::new(situation, 127 + variant as u64, variant);
            let mut block = [[0.0f32; 2]; 512];
            let mut previous = [0.0f32; 2];
            let mut energy = 0.0f64;
            let mut sum = [0.0f64; 2];
            let mut peak = 0.0f32;
            let mut jump = 0.0f32;
            while !piece.finished() {
                piece.render(&mut block);
                for frame in block {
                    for ch in 0..2 {
                        assert!(frame[ch].is_finite());
                        peak = peak.max(frame[ch].abs());
                        jump = jump.max((frame[ch] - previous[ch]).abs());
                        sum[ch] += frame[ch] as f64;
                        energy += (frame[ch] as f64).powi(2);
                    }
                    previous = frame;
                }
            }
            let n = piece.frames() as f64;
            assert!(peak > 0.03 && peak < 0.5, "{situation:?}/{variant}: peak {peak}");
            assert!(jump < 0.06, "{situation:?}/{variant}: jump {jump}");
            assert!((energy / (2.0 * n)).sqrt() > 0.004, "nearly silent {situation:?}");
            assert!(sum.iter().all(|s| (s / n).abs() < 1e-4), "DC {sum:?}");
            assert_eq!(previous, [0.0; 2]);
        }
    }
}

#[test]
fn nullable_music_info_fades_then_waits_without_restarting() {
    let mut m = Manager::new(9);
    m.tick_info(MusicInfo { music: Some(Situation::Dragon), volume: 1.0 }, false);
    let silent = MusicInfo { music: None, volume: 0.0 };
    let mut stopped = false;
    for _ in 0..400 {
        stopped |= m.tick_info(silent, false) == Action::Stop;
    }
    assert!(stopped);
    assert!(m.current.is_none());
    assert_eq!(m.gain, 0.0);
    assert!(m.delay >= 100);
    assert_eq!(
        m.tick_info(MusicInfo { music: Some(Situation::Credits), volume: 0.7 }, false),
        Action::Start(Situation::Credits)
    );
    assert_eq!(m.gain, 0.7);
}
