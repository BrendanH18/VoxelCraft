//! In-game slash command entry, scrollback and shared host command dispatch.
use super::{Game, GameMode};
use crate::render::ui::{Ui, WHITE};
use std::collections::VecDeque;
use voxelcraft::agent::{Command, HELP};
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
                    let result = Command::parse(&input).and_then(|c| self.host_command(c));
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
                let names = [
                    "help",
                    "give",
                    "gamemode",
                    "tp",
                    "time",
                    "weather",
                    "setblock",
                    "dimension",
                    "players",
                    "observe",
                ];
                let prefix = self.console.input.trim_start_matches('/');
                let matches: Vec<_> = names.iter().filter(|n| n.starts_with(prefix)).collect();
                if matches.len() == 1 {
                    self.console.input = format!("/{} ", matches[0]);
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

    pub(super) fn host_command(&mut self, command: Command) -> Result<String, String> {
        match command {
            Command::Catalog(query) => {
                return Ok(crate::item::Item::creative_palette()
                    .filter(|i| i.matches_query(&query))
                    .map(|i| i.name())
                    .collect::<Vec<_>>()
                    .join(", "));
            }
            Command::Help => return Ok(HELP.into()),
            Command::Give(item, count) => {
                let left = self.inventory.add(item, count);
                return Ok(format!("Gave {} {}", count - left, item.name()));
            }
            Command::Mode(creative) => self.set_mode(if creative { GameMode::Creative } else { GameMode::Survival }),
            Command::Teleport(pos) => {
                self.player.pos = pos;
                self.player.vel = glam::DVec3::ZERO;
                self.vitals.reset_fall();
                self.previous_eye = self.player.eye();
            }
            Command::Time(time) => self.day_time = time,
            Command::Weather(raining) => self.weather.set(raining, true),
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
            Command::Observe(_) => {
                return Ok(format!(
                    "{} at {:.1}, {:.1}, {:.1}",
                    self.dimension.name(),
                    self.player.pos.x,
                    self.player.pos.y,
                    self.player.pos.z
                ));
            }
            _ => return Err("use the agent CLI for movement and interaction commands".into()),
        }
        Ok("Done".into())
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
