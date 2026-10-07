//! Embedded agent hosting in the desktop world. Gameplay mutations stay on the game thread.
use super::Game;
use crossbeam_channel::Sender;
use glam::{DVec3, IVec3};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use voxelcraft::{
    agent::{Agent, Command},
    control::{Host, VERSION},
    entity::{PlayerId, Target},
};

pub(super) struct Bot {
    /// Stable for the life of the profile and saved with it.
    pub id: PlayerId,
    pub agent: Agent,
    pub active: bool,
    pub camera: voxelcraft::camera::CameraMode,
    reply: Option<Sender<Value>>,
    /// First-person hand for split-screen views, with the swings it has shown
    /// and where the feet were drawn last frame (for the walking bob).
    pub hand: super::hand::HandAnim,
    pub seen_swings: u32,
    pub drawn_feet: DVec3,
    /// Distance walked toward the next footstep.
    pub stride: f64,
}

impl Bot {
    /// Assigns the session's player ID and starts with simulation inactive.
    fn new(id: PlayerId, mut agent: Agent) -> Self {
        agent.id = id;
        let (seen_swings, drawn_feet) = (agent.swings, agent.player.pos);
        Self {
            id,
            agent,
            active: false,
            camera: Default::default(),
            reply: None,
            hand: Default::default(),
            seen_swings,
            drawn_feet,
            stride: 0.0,
        }
    }
}
#[derive(Default)]
pub(super) struct Agents {
    pub host: Option<Host>,
    pub players: BTreeMap<String, Bot>,
    pub cheats: bool,
}

impl Agents {
    /// The lowest unused player ID; the host is [`PlayerId::HOST`]. Never
    /// overflows, whatever IDs a save file restored.
    fn next_id(&self) -> PlayerId {
        let used: std::collections::BTreeSet<u32> = self.players.values().map(|b| b.id.0).collect();
        PlayerId((1..=u32::MAX).find(|id| !used.contains(id)).expect("fewer profiles than IDs"))
    }
    /// Adds an inactive profile with a fresh ID.
    pub fn insert(&mut self, name: String, agent: Agent) {
        let id = self.next_id();
        self.players.insert(name, Bot::new(id, agent));
    }
    /// Active agents as mob targets.
    pub fn targets(&self) -> impl Iterator<Item = Target> + '_ {
        self.players.values().filter(|b| b.active).map(|b| Target {
            alive: !b.agent.vitals.is_dead(),
            look: b.agent.player.forward().as_dvec3(),
            thorns: Target::thorns_of(&b.agent.inventory.armor),
            held_enchants: b.agent.inventory.get(b.agent.selected).map_or(Default::default(), |s| s.active_enchants()),
            ..Target::new(b.id, b.agent.player.pos, b.agent.targetable())
        })
    }
    pub fn by_id_mut(&mut self, id: PlayerId) -> Option<&mut Bot> {
        self.players.values_mut().find(|b| b.id == id)
    }
    pub fn positions(&self) -> Vec<DVec3> {
        self.players.values().filter(|b| b.active).map(|b| b.agent.player.pos).collect()
    }
    /// Saves named player profiles, progression and inventory in this dimension.
    pub fn serialize(&self, dimension: &str) -> String {
        let profiles: Vec<_> = self.players.iter().map(|(name, b)| {
            let mut inventory = b.agent.inventory.clone();
            inventory.return_stacks(b.agent.work.into_iter().flatten());
            let mode = if b.agent.creative { voxelcraft::rules::GameMode::Creative } else { b.agent.mode };
            json!({"name":name,"id":b.id.0,"position":b.agent.player.pos.to_array(),"yaw":b.agent.player.yaw,"pitch":b.agent.player.pitch,"mode":mode.name(),"creative":b.agent.creative,"selected":b.agent.selected,"flying":b.agent.player.flying,"health":b.agent.vitals.health,"air":b.agent.vitals.air,"food":b.agent.vitals.hunger.food,"saturation":b.agent.vitals.hunger.saturation,"exhaustion":b.agent.vitals.hunger.exhaustion,"xp":b.agent.vitals.xp.serialize(),"effects":b.agent.vitals.effects.serialize(),"inventory":inventory.serialize(),"bed":b.agent.spawn_bed.map(|p|p.to_array()),"spawn_point":b.agent.spawn_point.map(|p|p.to_array()),"dimension":dimension})
        }).collect();
        json!(profiles).to_string()
    }
    /// Restores valid profiles, moving players from other dimensions to `spawn`.
    pub fn restore(&mut self, text: &str, dimension: &str, spawn: DVec3) {
        let Ok(Value::Array(profiles)) = serde_json::from_str(text) else {
            return;
        };
        let mut missing = Vec::new();
        for p in profiles.into_iter().take(32) {
            let Some(name) = p["name"].as_str().filter(|n| voxelcraft::control::valid_name(n)) else {
                continue;
            };
            let pos = p["position"]
                .as_array()
                .filter(|v| v.len() == 3)
                .and_then(|v| Some(DVec3::new(v[0].as_f64()?, v[1].as_f64()?, v[2].as_f64()?)))
                .filter(|p| p.is_finite() && p.abs().max_element() <= 30_000_000.0)
                .unwrap_or(spawn);
            let mut agent = Agent::new(if p["dimension"] == dimension { pos } else { spawn });
            agent.inventory =
                p["inventory"].as_str().and_then(crate::inventory::Inventory::deserialize).unwrap_or_default();
            let mode = p["mode"].as_str().and_then(voxelcraft::rules::GameMode::from_name).unwrap_or(
                if p["creative"] == true {
                    voxelcraft::rules::GameMode::Creative
                } else {
                    voxelcraft::rules::GameMode::Survival
                },
            );
            agent.set_mode(mode);
            agent.player.flying =
                (mode.can_fly() && p["flying"] == true) || mode == voxelcraft::rules::GameMode::Spectator;
            agent.spawn_bed = p["bed"]
                .as_array()
                .filter(|v| v.len() == 3)
                .and_then(|v| Some(IVec3::new(v[0].as_i64()? as i32, v[1].as_i64()? as i32, v[2].as_i64()? as i32)));
            agent.spawn_point = p["spawn_point"]
                .as_array()
                .filter(|v| v.len() == 3)
                .and_then(|v| Some(IVec3::new(v[0].as_i64()? as i32, v[1].as_i64()? as i32, v[2].as_i64()? as i32)));
            agent.selected = p["selected"].as_u64().filter(|n| *n < 9).unwrap_or(0) as usize;
            agent.player.yaw = p["yaw"].as_f64().unwrap_or(0.0) as f32;
            agent.player.pitch = p["pitch"].as_f64().unwrap_or(0.0) as f32;
            agent.vitals = crate::simulation::survival::Vitals::restore(
                p["health"].as_f64().unwrap_or(20.0) as f32,
                p["air"].as_f64().unwrap_or(15.0) as f32,
                None,
            );
            agent.vitals.hunger = crate::simulation::survival::Hunger::restore(
                p["food"].as_f64().unwrap_or(20.0) as f32,
                p["saturation"].as_f64().unwrap_or(5.0) as f32,
                p["exhaustion"].as_f64().unwrap_or(0.0) as f32,
            );
            if let Some(xp) = p["xp"].as_str().and_then(crate::simulation::experience::Experience::parse) {
                agent.vitals.xp = xp;
            }
            if let Some(text) = p["effects"].as_str() {
                agent.vitals.effects = crate::simulation::effects::Effects::deserialize(text);
            }
            // Replacing a profile frees its old ID.
            self.players.remove(name);
            let id = p["id"]
                .as_u64()
                .and_then(|id| u32::try_from(id).ok())
                .map(PlayerId)
                .filter(|&id| id != PlayerId::HOST && !self.players.values().any(|b| b.id == id));
            match id {
                Some(id) => {
                    self.players.insert(name.into(), Bot::new(id, agent));
                }
                None => missing.push((name.to_string(), agent)),
            }
        }
        // Profiles saved before IDs existed (or with clashing ones) get new IDs.
        for (name, agent) in missing {
            self.insert(name, agent);
        }
    }

    /// Controller players' beds from saves before beds moved onto profiles:
    /// `profile=x,y,z` pairs separated by `;`.
    pub fn restore_pad_beds(&mut self, text: &str) {
        for (name, pos) in text.split(';').filter_map(|p| p.split_once('=')) {
            let v: Vec<i32> = pos.split(',').filter_map(|n| n.parse().ok()).collect();
            if let (&[x, y, z], Some(bot)) = (&v[..], self.players.get_mut(name)) {
                bot.agent.spawn_bed.get_or_insert(IVec3::new(x, y, z));
            }
        }
    }
}

impl Game {
    fn agent_response(&self, agent: &Agent, radius: i32) -> Value {
        json!({"ok":true,"version":VERSION,"tick":self.clock.ticks(),"time":self.day_time,"raining":self.weather.raining,"state":agent.observe(&self.world,radius)})
    }
    /// Process a bounded batch each frame. Timed commands reply once their final tick completes.
    pub(super) fn poll_agents(&mut self) {
        for _ in 0..32 {
            let Some(req) = self.agents.host.as_ref().and_then(|h| h.requests.try_recv().ok()) else {
                break;
            };
            let parsed = Command::parse(&req.command);
            let result = (|| -> Result<Option<Value>, String> {
                let command = parsed?;
                if command.cheat() && !self.agents.cheats {
                    return Err("agent cheats disabled; host needs --agent-cheats".into());
                }
                if matches!(command, Command::Dimension(_)) {
                    return Err("dimension travel is controlled by the host in this prototype".into());
                }
                if self.pads.seated(&req.player) {
                    return Err("player is controlled by a local gamepad".into());
                }
                if !self.agents.players.contains_key(&req.player) {
                    if self.agents.players.len() >= 32 {
                        return Err("profile limit reached".into());
                    }
                    self.agents.insert(req.player.clone(), Agent::new(self.player.pos + DVec3::new(2.0, 0.0, 0.0)));
                }
                let sleep = matches!(command, Command::Sleep);
                let active = self.agents.players.values().filter(|b| b.active).count();
                let mut bot = self.agents.players.remove(&req.player).unwrap();
                let result = (|| {
                    if !bot.active && active >= 8 {
                        return Err("eight agent players already active".into());
                    }
                    let observation =
                        matches!(command, Command::Observe(_) | Command::Players | Command::Catalog(_) | Command::Help);
                    if bot.reply.is_some() && !observation && !matches!(command, Command::Leave) {
                        return Err("player has a timed action in progress".into());
                    }
                    bot.active = true;
                    let radius = if let Command::Observe(r) = command { r } else { 0 };
                    match command {
                        Command::Catalog(query) => {
                            return Ok(Some(
                                json!({"ok":true,"version":VERSION,"items":crate::item::Item::creative_palette().filter(|i|i.matches_query(&query)).map(|i|json!({"id":i.0,"name":i.name(),"command_name":i.name().replace(' ',"_"),"max_stack":i.max_stack()})).collect::<Vec<_>>()}),
                            ));
                        }
                        Command::Help => {
                            return Ok(Some(json!({"ok":true,"version":VERSION,"help":voxelcraft::agent::HELP})));
                        }
                        Command::Players => {
                            return Ok(Some(
                                json!({"ok":true,"version":VERSION,"players":self.agents.players.iter().filter(|(_,b)|b.active).map(|(n,b)|json!({"name":n,"id":b.id.0,"position":b.agent.player.pos.to_array()})).chain(std::iter::once(json!({"name":req.player,"id":bot.id.0,"position":bot.agent.player.pos.to_array()}))).collect::<Vec<_>>()}),
                            ));
                        }
                        Command::Leave => {
                            bot.active = false;
                            if let Some(reply) = bot.reply.take() {
                                let _ = reply.send(json!({"ok":false,"error":"player left"}));
                            }
                            bot.agent.remaining = 0;
                            bot.agent.sleeping = None;
                        }
                        // Needs the bot back in the world, as a bed in the
                        // Nether explodes on everyone nearby.
                        Command::Sleep if bot.agent.vitals.is_dead() => {
                            return Err("player is dead; respawn first".into());
                        }
                        Command::Sleep => {}
                        Command::Time(t) => self.day_time = t.rem_euclid(1.0),
                        Command::TimeAdd(ticks) => self.add_time_ticks(ticks),
                        Command::Weather(kind) => self.apply_weather(kind),
                        Command::Respawn => {
                            let kept_xp = self.gamerules.bool("keepInventory").then_some(bot.agent.vitals.xp);
                            bot.agent.execute(Command::Respawn, &mut self.world, &mut self.mobs.entities, &[])?;
                            if let Some(xp) = kept_xp {
                                bot.agent.vitals.xp = xp;
                            }
                        }
                        _ => {
                            let mut others = self.agents.positions();
                            others.push(self.player.pos);
                            bot.agent.execute(command, &mut self.world, &mut self.mobs.entities, &others)?;
                        }
                    }
                    if bot.agent.remaining > 0 && !observation {
                        bot.reply = Some(req.reply.clone());
                        Ok(None)
                    } else {
                        Ok(Some(self.agent_response(&bot.agent, radius)))
                    }
                })();
                self.agents.players.insert(req.player.clone(), bot);
                if sleep && result.is_ok() {
                    self.agent_sleep(&req.player)?;
                    return Ok(Some(self.agent_response(&self.agents.players[&req.player].agent, 0)));
                }
                result
            })();
            match result {
                Ok(Some(value)) => {
                    let _ = req.reply.send(value);
                }
                Ok(None) => {}
                Err(e) => {
                    let _ = req.reply.send(json!({"ok":false,"version":VERSION,"error":e}));
                }
            }
        }
    }
    pub(super) fn tick_agents(&mut self) {
        // Agents still have source-dimension positions until arrival relocates them.
        if self.arrival.is_some() {
            return;
        }
        let mut players = std::mem::take(&mut self.agents.players);
        for (name, bot) in players.iter_mut().filter(|(_, b)| b.active) {
            let was_dead = bot.agent.vitals.is_dead();
            bot.agent.tick_rules(&mut self.world, &mut self.mobs.entities, self.difficulty, &self.gamerules);
            if !was_dead && bot.agent.vitals.is_dead() && self.gamerules.bool("showDeathMessages") {
                log::info!("{name} {}", bot.agent.vitals.death.as_deref().unwrap_or("died"));
            }
            if self.hardcore {
                bot.agent.hardcore_spectate();
            } else if self.gamerules.bool("doImmediateRespawn") && bot.agent.vitals.is_dead() {
                let kept_xp = self.gamerules.bool("keepInventory").then_some(bot.agent.vitals.xp);
                let _ = bot.agent.execute(Command::Respawn, &mut self.world, &mut self.mobs.entities, &[]);
                if let Some(xp) = kept_xp {
                    bot.agent.vitals.xp = xp;
                }
            }
            if bot.agent.remaining == 0
                && let Some(reply) = bot.reply.take()
            {
                let _ = reply.send(self.agent_response(&bot.agent, 0));
            }
        }
        self.agents.players = players;
    }
    /// This initial host keeps agents in the host's active dimension, retaining inventory across travel.
    pub(super) fn relocate_agents(&mut self) {
        for bot in self.agents.players.values_mut() {
            bot.agent.player.pos = self.player.pos + DVec3::new(2.0, 0.0, 0.0);
            bot.agent.previous_pos = bot.agent.player.pos;
            bot.agent.player.vel = DVec3::ZERO;
            bot.agent.vitals.reset_fall();
            bot.agent.remaining = 0;
            bot.agent.sleeping = None;
            if let Some(reply) = bot.reply.take() {
                let _ = reply.send(json!({"ok":false,"error":"host changed dimension; observe before continuing"}));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn saving_returns_work_inputs_without_mutating_live_slots() {
        use crate::inventory::{SLOTS, Stack};
        use crate::item::Item;
        let mut agents = Agents::default();
        let mut agent = Agent::new(DVec3::ZERO);
        agent.inventory.slots.fill(Some(Stack::new(Item::STICK, 64)));
        agent.work = [
            Some(Stack::new(Item::ENCHANTED_BOOK, 1)),
            Some(Stack::new(Item::LAPIS_LAZULI, 3)),
            Some(Stack::new(Item::NETHERITE_INGOT, 2)),
        ];
        agents.insert("Player2".into(), agent);
        let text = agents.serialize("overworld");
        let mut restored = Agents::default();
        restored.restore(&text, "overworld", DVec3::ZERO);
        let inv = &mut restored.players.get_mut("Player2").unwrap().agent.inventory;
        assert_eq!(inv.slots.iter().flatten().count(), SLOTS);
        assert_eq!(inv.take_spill(), agents.players["Player2"].agent.work.into_iter().flatten().collect::<Vec<_>>());
        assert_eq!(restored.players["Player2"].agent.work, [None; 3]);
        assert!(agents.players["Player2"].agent.work.iter().all(Option::is_some));
    }

    #[test]
    fn profiles_restore_without_reactivating_or_losing_inventory() {
        let spawn = DVec3::new(1.0, 150.0, 2.0);
        let mut agents = Agents::default();
        let mut agent = Agent::new(DVec3::new(300.0, 100.0, -250.0));
        agent.inventory.add(crate::item::Item::DIAMOND, 3);
        agent.creative = true;
        agent.selected = 2;
        agent.spawn_bed = Some(IVec3::new(4, 70, -2));
        agent.player.flying = true;
        agent.vitals.hunger = crate::simulation::survival::Hunger::restore(8.0, 2.0, 1.0);
        agent.vitals.xp = crate::simulation::experience::Experience::restore(7, 4, 90);
        agents.insert("builder".into(), agent);
        agents.players.get_mut("builder").unwrap().active = true;
        let text = agents.serialize("overworld");
        let mut restored = Agents::default();
        restored.restore(&text, "overworld", spawn);
        let bot = &restored.players["builder"];
        assert_eq!(bot.id, PlayerId(1), "stable ID survives saving");
        assert!(!bot.active);
        assert!(bot.agent.player.flying);
        assert_eq!(bot.agent.selected, 2);
        assert_eq!(bot.agent.player.pos, DVec3::new(300.0, 100.0, -250.0));
        assert_eq!(bot.agent.inventory.get(0).unwrap().count, 3);
        assert_eq!(bot.agent.vitals.hunger.food, 8.0);
        assert_eq!(bot.agent.vitals.xp, crate::simulation::experience::Experience::restore(7, 4, 90));
        assert_eq!(bot.agent.spawn_bed, Some(IVec3::new(4, 70, -2)));
        restored.restore(&text, "nether", spawn);
        assert_eq!(restored.players["builder"].agent.player.pos, spawn);
        assert_eq!(restored.players["builder"].id, PlayerId(1));
    }

    #[test]
    fn old_controller_beds_move_onto_profiles() {
        let mut agents = Agents::default();
        agents.restore(r#"[{"name":"Player2"},{"name":"Player3","bed":[9,9,9]}]"#, "overworld", DVec3::ZERO);
        agents.restore_pad_beds("Player2=1,-2,3;Player3=4,5,6;Ghost=7,8,9;Player4=a,b,c");
        assert_eq!(agents.players["Player2"].agent.spawn_bed, Some(IVec3::new(1, -2, 3)));
        assert_eq!(agents.players["Player3"].agent.spawn_bed, Some(IVec3::new(9, 9, 9)), "newer bed wins");
        assert_eq!(agents.players.len(), 2);
    }

    #[test]
    fn profiles_without_ids_get_unique_ones() {
        let spawn = DVec3::new(0.0, 100.0, 0.0);
        let text = r#"[{"name":"a","id":5},{"name":"b"},{"name":"c","id":5},{"name":"d","id":0}]"#;
        let mut agents = Agents::default();
        agents.restore(text, "overworld", spawn);
        let ids: Vec<u32> = ["a", "b", "c", "d"].iter().map(|n| agents.players[*n].id.0).collect();
        assert_eq!(ids[0], 5);
        let mut unique = ids.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), 4, "{ids:?}");
        assert!(ids.iter().all(|&id| id != 0), "{ids:?}");
    }

    #[test]
    fn the_largest_saved_id_does_not_overflow_new_ids() {
        let text = format!(r#"[{{"name":"max","id":{}}},{{"name":"old"}}]"#, u32::MAX);
        let mut agents = Agents::default();
        agents.restore(&text, "overworld", DVec3::ZERO);
        agents.insert("new".into(), Agent::new(DVec3::ZERO));
        assert_eq!(agents.players["max"].id, PlayerId(u32::MAX));
        assert_eq!(agents.players["old"].id, PlayerId(1));
        assert_eq!(agents.players["new"].id, PlayerId(2));
    }
}
