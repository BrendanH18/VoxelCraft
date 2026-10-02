//! Embedded agent hosting in the desktop world. Gameplay mutations stay on the game thread.
use super::Game;
use crossbeam_channel::Sender;
use glam::DVec3;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use voxelcraft::{
    agent::{Agent, Command},
    control::{Host, VERSION},
};

pub(super) struct Bot {
    pub agent: Agent,
    pub active: bool,
    reply: Option<Sender<Value>>,
}
#[derive(Default)]
pub(super) struct Agents {
    pub host: Option<Host>,
    pub players: BTreeMap<String, Bot>,
    pub cheats: bool,
}

impl Agents {
    pub fn positions(&self) -> Vec<DVec3> {
        self.players.values().filter(|b| b.active).map(|b| b.agent.player.pos).collect()
    }
    pub fn serialize(&self, dimension: &str) -> String {
        let profiles: Vec<_>=self.players.iter().map(|(name,b)|json!({"name":name,"position":b.agent.player.pos.to_array(),"yaw":b.agent.player.yaw,"pitch":b.agent.player.pitch,"creative":b.agent.creative,"selected":b.agent.selected,"flying":b.agent.player.flying,"health":b.agent.vitals.health,"air":b.agent.vitals.air,"food":b.agent.vitals.hunger.food,"saturation":b.agent.vitals.hunger.saturation,"exhaustion":b.agent.vitals.hunger.exhaustion,"inventory":b.agent.inventory.serialize(),"dimension":dimension})).collect();
        json!(profiles).to_string()
    }
    pub fn restore(&mut self, text: &str, dimension: &str, spawn: DVec3) {
        let Ok(Value::Array(profiles)) = serde_json::from_str(text) else {
            return;
        };
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
            agent.creative = p["creative"] == true;
            agent.player.can_fly = agent.creative;
            agent.player.flying = agent.creative && p["flying"] == true;
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
            self.players.insert(name.into(), Bot { agent, active: false, reply: None });
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
                if !self.agents.players.contains_key(&req.player) {
                    if self.agents.players.len() >= 32 {
                        return Err("profile limit reached".into());
                    }
                    self.agents.players.insert(
                        req.player.clone(),
                        Bot {
                            agent: Agent::new(self.player.pos + DVec3::new(2.0, 0.0, 0.0)),
                            active: false,
                            reply: None,
                        },
                    );
                }
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
                                json!({"ok":true,"version":VERSION,"players":self.agents.players.iter().filter(|(_,b)|b.active).map(|(n,b)|json!({"name":n,"position":b.agent.player.pos.to_array()})).chain(std::iter::once(json!({"name":req.player,"position":bot.agent.player.pos.to_array()}))).collect::<Vec<_>>()}),
                            ));
                        }
                        Command::Leave => {
                            bot.active = false;
                            if let Some(reply) = bot.reply.take() {
                                let _ = reply.send(json!({"ok":false,"error":"player left"}));
                            }
                            bot.agent.remaining = 0;
                        }
                        Command::Time(t) => self.day_time = t,
                        Command::Weather(r) => self.weather.set(r, true),
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
        for bot in players.values_mut().filter(|b| b.active) {
            bot.agent.tick(&mut self.world, &mut self.mobs.entities);
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
    fn profiles_restore_without_reactivating_or_losing_inventory() {
        let spawn = DVec3::new(1.0, 150.0, 2.0);
        let mut agents = Agents::default();
        let mut agent = Agent::new(DVec3::new(300.0, 100.0, -250.0));
        agent.inventory.add(crate::item::Item::DIAMOND, 3);
        agent.creative = true;
        agent.selected = 2;
        agent.player.flying = true;
        agent.vitals.hunger = crate::simulation::survival::Hunger::restore(8.0, 2.0, 1.0);
        agents.players.insert("builder".into(), Bot { agent, active: true, reply: None });
        let text = agents.serialize("overworld");
        let mut restored = Agents::default();
        restored.restore(&text, "overworld", spawn);
        let bot = &restored.players["builder"];
        assert!(!bot.active);
        assert!(bot.agent.player.flying);
        assert_eq!(bot.agent.selected, 2);
        assert_eq!(bot.agent.player.pos, DVec3::new(300.0, 100.0, -250.0));
        assert_eq!(bot.agent.inventory.get(0).unwrap().count, 3);
        assert_eq!(bot.agent.vitals.hunger.food, 8.0);
        restored.restore(&text, "nether", spawn);
        assert_eq!(restored.players["builder"].agent.player.pos, spawn);
    }
}
