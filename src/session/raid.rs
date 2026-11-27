//! v0.15 Track E: a player's raid omens (Java 26.3 `BadOmenMobEffect` and
//! `RaidOmenMobEffect`), shared by the host and CLI agents.
use glam::IVec3;

use super::PlayerSession;
use crate::entity::{Entities, MobWorld, raid};
use crate::rules::GameMode;
use crate::simulation::effects::{Active, Effect};

/// What a tick of the omens did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Omen {
    /// Bad Omen became Raid Omen here.
    Gathering(IVec3),
    /// The Raid Omen ran out and started or strengthened this raid.
    Raid(u32),
}

impl PlayerSession {
    /// Before the effects tick down: Bad Omen in a village whose raid (if
    /// any) is below level 5 becomes Raid Omen for 600 ticks, remembering
    /// where; a Raid Omen with one tick left starts or extends the raid
    /// there (`Raids.createOrExtendRaid`) and ends. `can_start` is the
    /// dimension's `can_start_raid` attribute (false in the Nether).
    pub fn tick_raid_omens<W: MobWorld + ?Sized>(
        &mut self,
        entities: &mut Entities,
        world: &W,
        peaceful: bool,
        can_start: bool,
    ) -> Option<Omen> {
        if self.mode == GameMode::Spectator || self.vitals.is_dead() {
            return None;
        }
        if let Some(omen) = self.vitals.effects.get(Effect::RaidOmen).copied()
            && omen.ticks <= raid::RAID_OMEN_FIRES_AT
            && let Some(at) = self.raid_omen_pos.take()
        {
            self.vitals.effects.remove(Effect::RaidOmen);
            return entities.create_or_extend_raid(world, self.id, at, omen.amplifier, can_start).map(Omen::Raid);
        }
        let bad = self.vitals.effects.get(Effect::BadOmen).copied()?;
        let here = self.player.pos.floor().as_ivec3();
        if peaceful || !entities.is_village(world, here) {
            return None;
        }
        if entities.raid_at(here).is_some_and(|r| r.omen_level >= raid::MAX_RAID_OMEN_LEVEL) {
            return None;
        }
        self.vitals.effects.remove(Effect::BadOmen);
        self.vitals.give_effect(Active::new(Effect::RaidOmen, bad.amplifier, raid::RAID_OMEN_TICKS));
        self.raid_omen_pos = Some(here);
        Some(Omen::Gathering(here))
    }

    /// `Raid.tick`'s reward: Hero of the Village for 48000 ticks, without
    /// particles but with its icon.
    pub fn crown_hero(&mut self, amplifier: u8) {
        let hero = Active::new(Effect::HeroOfTheVillage, amplifier, raid::HERO_OF_THE_VILLAGE_TICKS)
            .with_flags(false, false, true);
        self.vitals.give_effect(hero);
    }

    /// `raid_omen_position` for saves: `x,y,z`.
    pub fn raid_omen_to_string(&self) -> Option<String> {
        self.raid_omen_pos.map(|p| format!("{},{},{}", p.x, p.y, p.z))
    }

    pub fn load_raid_omen(&mut self, text: &str) {
        let mut parts = text.split(',').map(|n| n.trim().parse::<i32>());
        if let (Some(Ok(x)), Some(Ok(y)), Some(Ok(z)), None) = (parts.next(), parts.next(), parts.next(), parts.next())
        {
            self.raid_omen_pos = Some(IVec3::new(x, y, z));
        }
    }
}

/// v1.0 Track 6: a player's custom statistics (`Stats.CUSTOM`), by Java's
/// identifier without the `minecraft:` namespace. Saved as `name=count`
/// pairs joined by commas; unknown names are kept as they are.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Stats(std::collections::BTreeMap<String, u32>);

impl Stats {
    /// `Stats.RAID_TRIGGER`: a raid started from this player's Raid Omen.
    pub const RAID_TRIGGER: &'static str = "raid_trigger";
    /// `Stats.RAID_WIN`: this player was a hero of a won raid.
    pub const RAID_WIN: &'static str = "raid_win";

    /// `Player.awardStat(stat)`: one more.
    pub fn award(&mut self, name: &str) {
        let n = self.0.entry(name.to_string()).or_default();
        *n = n.saturating_add(1);
    }

    pub fn get(&self, name: &str) -> u32 {
        self.0.get(name).copied().unwrap_or(0)
    }

    pub fn serialize(&self) -> String {
        self.0.iter().map(|(k, v)| format!("{k}={v}")).collect::<Vec<_>>().join(",")
    }

    /// Reads [`Stats::serialize`]'s text; malformed pairs are skipped.
    pub fn parse(text: &str) -> Self {
        let mut out = Self::default();
        for pair in text.split(',') {
            if let Some((k, v)) = pair.split_once('=')
                && !k.is_empty()
                && k.bytes().all(|b| b.is_ascii_lowercase() || b == b'_' || b.is_ascii_digit())
                && let Ok(v) = v.parse()
            {
                out.0.insert(k.to_string(), v);
            }
        }
        out
    }
}
