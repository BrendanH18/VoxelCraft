//! In-game slash command entry, scrollback and shared host command dispatch.
use super::Game;
use crate::render::ui::{Ui, WHITE};
use glam::{DVec3, IVec2, IVec3};
use std::collections::VecDeque;
use voxelcraft::agent::{Command, HELP, TimeQuery, WeatherKind};
use voxelcraft::simulation::survival;
use winit::event::KeyEvent;
use winit::keyboard::{KeyCode, PhysicalKey};

#[derive(Default)]
pub(super) struct Console {
    pub open: bool,
    pub input: String,
    pub history: Vec<String>,
    pub history_index: Option<usize>,
    pub draft: String,
    pub lines: VecDeque<String>,
}

impl Game {
    /// Consume text entry before gameplay bindings so commands cannot move or drop items.
    pub(super) fn console_key(&mut self, event: &KeyEvent) -> bool {
        let PhysicalKey::Code(code) = event.physical_key else {
            return self.console.open;
        };
        if !self.console.open {
            if self.menu.is_none()
                && !self.inventory_open
                && matches!(code, KeyCode::Slash | KeyCode::KeyT | KeyCode::Backquote)
            {
                self.console.open = true;
                self.console.input = "/".into();
                self.keys.clear();
                self.left_held = false;
                self.right_held = false;
                self.actions.reset();
                self.set_grab(false);
                return true;
            }
            return false;
        }
        match code {
            KeyCode::Escape => {
                self.console.open = false;
                self.set_grab(true);
            }
            KeyCode::Enter | KeyCode::NumpadEnter => {
                let input = std::mem::replace(&mut self.console.input, "/".into());
                self.console.history_index = None;
                if input.trim() != "/" {
                    if self.console.history.len() == 64 {
                        self.console.history.remove(0);
                    }
                    self.console.history.push(input.clone());
                    self.console.lines.push_back(input.clone());
                    let words: Vec<&str> = input.trim_start_matches('/').split_whitespace().collect();
                    let result = match words[..] {
                        ["splitscreen"] => self.split_command(""),
                        ["splitscreen", arg] => self.split_command(arg),
                        ["splitscreen", ..] => Err("usage: /splitscreen [player|off|side|stacked]".into()),
                        _ => Command::parse(&input).and_then(|c| self.host_command(c)),
                    };
                    self.console.lines.push_back(result.unwrap_or_else(|e| format!("Error: {e}")));
                    while self.console.lines.len() > 64 {
                        self.console.lines.pop_front();
                    }
                }
            }
            KeyCode::Backspace => {
                self.console.input.pop();
            }
            KeyCode::ArrowUp => {
                if !self.console.history.is_empty() {
                    if self.console.history_index.is_none() {
                        self.console.draft = self.console.input.clone();
                    }
                    let i = self.console.history_index.unwrap_or(self.console.history.len()).saturating_sub(1);
                    self.console.history_index = Some(i);
                    self.console.input = self.console.history[i].clone();
                }
            }
            KeyCode::ArrowDown => {
                if let Some(i) = self.console.history_index {
                    if i + 1 < self.console.history.len() {
                        self.console.history_index = Some(i + 1);
                        self.console.input = self.console.history[i + 1].clone();
                    } else {
                        self.console.history_index = None;
                        self.console.input = self.console.draft.clone();
                    }
                }
            }
            KeyCode::Tab => {
                if let Some(completed) = voxelcraft::agent::tab_complete(&self.console.input) {
                    self.console.input = completed;
                }
            }
            _ => {
                if let Some(text) = &event.text {
                    for c in text.chars().filter(|c| !c.is_control()) {
                        if self.console.input.len() + c.len_utf8() <= 1024 {
                            self.console.input.push(c);
                        }
                    }
                }
            }
        }
        true
    }

    /// Applies a parsed console command to the host and returns its feedback.
    pub(super) fn host_command(&mut self, command: Command) -> Result<String, String> {
        match command {
            Command::Catalog(query) => {
                return Ok(crate::item::Item::creative_palette()
                    .filter(|i| i.matches_query(&query))
                    .map(|i| i.name())
                    .collect::<Vec<_>>()
                    .join(", "));
            }
            Command::Help => {
                return Ok(format!("{HELP} Host only: splitscreen [player|off|side|stacked]."));
            }
            Command::Give(item, count) => {
                let left = self.inventory.add(item, count);
                return Ok(format!("Gave {} {}", count - left, item.name()));
            }
            Command::Clear => {
                let count: u32 = self.inventory.take_all().into_iter().map(|stack| stack.count as u32).sum();
                self.work.fill(None);
                self.craft.cells.fill(None);
                return Ok(format!("Removed {count} items"));
            }
            Command::Kill => {
                let was_dead = self.vitals.is_dead();
                self.vitals.damage(f32::MAX, "was killed", false);
                if !was_dead && self.vitals.is_dead() {
                    self.on_death();
                }
            }
            Command::Summon(kind, pos) => {
                let at = pos.resolve(self.player.pos)?;
                self.mobs.entities.spawn(kind, at);
                return Ok(format!("Summoned {} at {:.1} {:.1} {:.1}", kind.name(), at.x, at.y, at.z));
            }
            Command::Mode(mode) => self.set_mode(mode),
            Command::Teleport(pos) => {
                let pos = pos.resolve(self.player.pos)?;
                self.player.pos = pos;
                self.player.vel = glam::DVec3::ZERO;
                self.vitals.reset_fall();
                self.previous_eye = self.player.eye();
            }
            Command::SpawnPoint(pos) => {
                let p = pos.resolve(self.player.pos)?.floor().as_ivec3();
                self.spawn_bed = None;
                self.spawn_point = Some(p);
                return Ok(format!("Set spawn point to {} {} {}", p.x, p.y, p.z));
            }
            Command::Time(time) => {
                self.day_time = time.rem_euclid(1.0);
                return Ok(format!("Set time to {}", (self.day_time * 24_000.0).round() as i64));
            }
            Command::TimeAdd(ticks) => {
                self.add_time_ticks(ticks);
                return Ok(format!("Added {ticks} ticks"));
            }
            Command::TimeQuery(query) => {
                return Ok(match query {
                    TimeQuery::Daytime => ((self.day_time * 24_000.0).round() as i64).to_string(),
                    TimeQuery::Day => self.day_count.to_string(),
                    TimeQuery::Gametime => {
                        (self.day_count.saturating_mul(24_000) + (self.day_time * 24_000.0).round() as i64).to_string()
                    }
                });
            }
            Command::Weather(kind) => {
                self.apply_weather(kind);
            }
            Command::Difficulty(difficulty) => {
                if self.hardcore {
                    return Err("Hardcore locks difficulty to Hard".into());
                }
                self.difficulty = difficulty;
                return Ok(format!("Set difficulty to {difficulty}"));
            }
            Command::GameRule { name, value: None } => {
                let value = self.gamerules.get(&name).ok_or_else(|| format!("unknown gamerule: {name}"))?;
                return Ok(format!("{name} = {value}"));
            }
            Command::GameRule { name, value: Some(text) } => {
                let value = self.gamerules.set(&name, &text)?;
                self.world.set_tile_drops(self.gamerules.bool("doTileDrops"));
                return Ok(format!("Set {name} to {value}"));
            }
            Command::Seed => return Ok(format!("Seed: [{}]", self.world.generator.seed)),
            Command::SetWorldSpawn(pos) => {
                let p = pos.resolve(self.player.pos)?.floor().as_ivec3();
                self.world_spawn = p;
                return Ok(format!("Set the world spawn point to {} {} {}", p.x, p.y, p.z));
            }
            Command::LocateStructure(name) => {
                let key = name.strip_prefix("minecraft:").unwrap_or(&name);
                let at = match key {
                    "stronghold" => self.world.generator.strongholds.nearest(self.player.pos.floor().as_ivec3()),
                    "fortress" | "nether_fortress" => self.world.generator.nearest_fortress(IVec2::new(
                        self.player.pos.x.floor() as i32,
                        self.player.pos.z.floor() as i32,
                    )),
                    "mineshaft" | "abandoned_mineshaft" => {
                        self.world.generator.mineshafts.nearest(self.player.pos.floor().as_ivec3())
                    }
                    _ => return Err(format!("unknown structure: {name}")),
                }
                .ok_or("Could not find that structure nearby")?;
                return Ok(self.locate_message(at, key));
            }
            Command::LocateBiome(biome) => {
                let origin = IVec2::new(self.player.pos.x.floor() as i32, self.player.pos.z.floor() as i32);
                let at = self
                    .world
                    .generator
                    .nearest_biome(origin, biome, 12_800)
                    .ok_or("Could not find that biome nearby")?;
                return Ok(self.locate_message(at, biome.name()));
            }
            Command::SetBlock(pos, block) => {
                if !self.world.set_block(pos, block) {
                    return Err("block unchanged or unloaded".into());
                }
            }
            Command::Dimension(to) => {
                if to != self.dimension {
                    let arrival = match to {
                        crate::world::terrain::Dimension::End => super::dimension::Arrival::EndSpawn,
                        crate::world::terrain::Dimension::Nether => {
                            super::dimension::Arrival::Portal(glam::IVec3::new(0, 64, 0))
                        }
                        crate::world::terrain::Dimension::Overworld => super::dimension::Arrival::Respawn,
                    };
                    self.switch_dimension(to, arrival);
                }
            }
            Command::Players => {
                return Ok(format!(
                    "Host + {} agents: {}",
                    self.agents.players.values().filter(|b| b.active).count(),
                    self.agents
                        .players
                        .iter()
                        .filter(|(_, b)| b.active)
                        .map(|(n, _)| n.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            }
            Command::Xp(change) => {
                if let Some(chime) = change.apply(&mut self.vitals.xp) {
                    self.audio.play(crate::audio::sounds::Sound::LevelUp, None, chime, (1.0, 1.0));
                }
                return Ok(self.xp_summary());
            }
            Command::XpQuery => return Ok(self.xp_summary()),
            Command::Enchant(e, level) => {
                let slot = &mut self.inventory.slots[self.actions.selected];
                let stack = crate::enchant::command(*slot, e, level)?;
                *slot = Some(stack);
                return Ok(format!("Applied {} to {}", e.describe(level), stack.item.name()));
            }
            Command::Effect(change) => {
                let (damage, text) = change.apply(&mut self.vitals);
                self.damage_player(damage, survival::CAUSE_MAGIC);
                return Ok(text);
            }
            Command::Observe(_) => {
                return Ok(format!(
                    "{} at {:.1}, {:.1}, {:.1}",
                    self.dimension.name(),
                    self.player.pos.x,
                    self.player.pos.y,
                    self.player.pos.z
                ));
            }
            Command::Say(message) => {
                log::info!("[Host] {message}");
                return Ok(format!("[Host] {message}"));
            }
            _ => return Err("use the agent CLI for movement and interaction commands".into()),
        }
        Ok("Done".into())
    }

    pub(super) fn add_time_ticks(&mut self, ticks: i64) {
        if ticks == 0 {
            return;
        }
        self.day_time += ticks as f64 / 24_000.0;
        while self.day_time >= 1.0 {
            self.day_time -= 1.0;
            self.day_count = self.day_count.saturating_add(1);
        }
        while self.day_time < 0.0 {
            self.day_time += 1.0;
            self.day_count = self.day_count.saturating_sub(1);
        }
    }

    pub(super) fn apply_weather(&mut self, kind: WeatherKind) {
        match kind {
            WeatherKind::Clear => self.weather.set(false, true),
            WeatherKind::Rain => self.weather.set(true, true),
            WeatherKind::Thunder => self.weather.set_thunder(true),
        }
    }

    fn locate_message(&self, at: IVec3, label: &str) -> String {
        let there = at.as_dvec3() + DVec3::new(0.5, 0.0, 0.5);
        let blocks = (there - self.player.pos).length().round() as i32;
        format!("The nearest {label} is at {} {} {} ({blocks} blocks away)", at.x, at.y, at.z)
    }

    /// Formats the host's level and progress toward the next level.
    fn xp_summary(&self) -> String {
        let xp = &self.vitals.xp;
        let next = crate::simulation::experience::points_to_next(xp.level);
        format!("Level {} ({}/{} points to the next)", xp.level, xp.points, next)
    }

    pub(super) fn console_ui(&self, ui: &mut Ui) {
        let (w, h) = ui.size();
        let width = ((w - 24.0) / 8.0).max(8.0) as usize;
        let mut rows = Vec::new();
        for line in &self.console.lines {
            let mut row = String::new();
            for word in line.split_whitespace() {
                if row.chars().count() + word.chars().count() + 1 > width && !row.is_empty() {
                    rows.push(std::mem::take(&mut row));
                }
                if !row.is_empty() {
                    row.push(' ');
                }
                row.push_str(word);
            }
            rows.push(row);
        }
        let count = rows.len().min(((h - 70.0) / 12.0).clamp(1.0, 12.0) as usize);
        let y = h - 40.0 - count as f32 * 12.0;
        ui.rect(4.0, y - 6.0, w - 8.0, h - y - 2.0, [0.015, 0.02, 0.03, 0.94]);
        for (i, row) in rows.iter().skip(rows.len() - count).enumerate() {
            ui.text_flat(10.0, y + i as f32 * 12.0, row, WHITE);
        }
        ui.text_flat(
            10.0,
            h - 32.0,
            "Commands - Enter: run  Esc: close  Tab: complete  Up/Down: history",
            [0.6, 0.75, 0.9, 1.0],
        );
        let text: String =
            self.console.input.chars().rev().take(width.saturating_sub(2)).collect::<String>().chars().rev().collect();
        ui.text_flat(10.0, h - 17.0, &format!("{text}_"), WHITE);
    }
}
