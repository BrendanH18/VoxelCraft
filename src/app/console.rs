//! In-game slash command entry, scrollback and shared host command dispatch.
use super::Game;
use super::settings::{ChatOptions, ChatVisibility};
use crate::render::ui::{Ui, WHITE};
use crate::world::terrain::Dimension;
use glam::{DVec3, IVec2, IVec3};
use std::collections::VecDeque;
use voxelcraft::agent::{Command, HELP, TimeQuery, WeatherKind};
use voxelcraft::rules::GameRules;
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
    /// v1.0 gap track 6d: when each of `lines` came (the same length), so
    /// each fades out as Java's chat does (`ChatComponent.AlphaCalculator
    /// .timeBased`, 200 ticks).
    pub added: VecDeque<std::time::Instant>,
    /// v1.0 final F6: `ChatComponent.chatScrollbarPos`, in wrapped rows up
    /// from the newest, and `newMessageSinceScroll`.
    pub scroll: usize,
    pub new_since_scroll: bool,
    /// Players' chat lines Chat Delay holds back (`ChatListener`'s queue),
    /// and when the last one was let through.
    pub queue: VecDeque<String>,
    pub released: Option<std::time::Instant>,
}

/// v1.0 final F6: the chat lines `ChatComponent` keeps (`MAX_CHAT_HISTORY`).
pub(super) const CHAT_HISTORY: usize = 100;

impl Console {
    /// Adds a line to the scrollback and the chat, keeping the last 100.
    /// A player scrolled back keeps their place (`ChatComponent.addMessage`
    /// scrolls one more and marks the scroll bar).
    pub fn push(&mut self, line: String) {
        self.lines.push_back(line);
        self.added.push_back(std::time::Instant::now());
        while self.lines.len() > CHAT_HISTORY {
            self.lines.pop_front();
            self.added.pop_front();
        }
        if self.open && self.scroll > 0 {
            self.new_since_scroll = true;
            self.scroll += 1;
        }
    }

    /// A player's chat line: held back while Chat Delay is set and others
    /// wait or the last came too recently (`ChatListener.handleMessage`).
    pub fn push_player_chat(&mut self, line: String, delay: f32) {
        if delay <= 0.0 {
            self.push(line);
        } else {
            self.queue.push_back(line);
            self.release(delay);
        }
    }

    /// `ChatListener.tick`: lets the next held line through once `delay`
    /// seconds have passed since the last; everything at once at zero.
    pub fn release(&mut self, delay: f32) {
        let now = std::time::Instant::now();
        if delay <= 0.0 {
            while let Some(line) = self.queue.pop_front() {
                self.push(line);
            }
            return;
        }
        if self.released.is_none_or(|t| now.duration_since(t).as_secs_f32() >= delay)
            && let Some(line) = self.queue.pop_front()
        {
            self.push(line);
            self.released = Some(now);
        }
    }

    /// `ChatScreen`'s Page Up and Page Down, and its wheel: `scrollChat`,
    /// clamped to the rows above the page.
    pub fn scroll_chat(&mut self, by: i32, rows: usize, per_page: usize) {
        let max = rows.saturating_sub(per_page) as i64;
        let to = (self.scroll as i64 + i64::from(by)).min(max);
        if to <= 0 {
            self.scroll = 0;
            self.new_since_scroll = false;
        } else {
            self.scroll = to as usize;
        }
    }
}

/// Whether a chat line is a player's message (`<name> text`) rather than
/// a system or command line: Commands Only hides them, Chat Delay holds them.
pub(super) fn is_player_chat(line: &str) -> bool {
    line.starts_with('<') && line.contains("> ")
}

/// v1.0 final F6: the chat's lines wrapped to its width (`ChatComponent
/// .refreshTrimmedMessages`, `floor(width / scale)` pixels), newest last,
/// each with when its line came. Chat: Hidden shows none; Commands Only
/// leaves out players' messages.
pub(super) fn chat_rows(console: &Console, chat: &ChatOptions) -> Vec<(String, std::time::Instant)> {
    if chat.visibility == ChatVisibility::Hidden || chat.scale <= 0.0 {
        return Vec::new();
    }
    let width = (ChatOptions::width_px(chat.width) as f32 / chat.scale).floor().max(1.0);
    let mut rows = Vec::new();
    for (line, &added) in console.lines.iter().zip(&console.added) {
        if line.is_empty() || (chat.visibility == ChatVisibility::System && is_player_chat(line)) {
            continue;
        }
        let mut row = String::new();
        for word in line.split(' ') {
            if !row.is_empty() && Ui::text_width(&format!("{row} {word}")) > width {
                rows.push((std::mem::take(&mut row), added));
            }
            if !row.is_empty() {
                row.push(' ');
            }
            row.push_str(word);
        }
        rows.push((row, added));
    }
    rows
}

/// What the chat draws besides its rows.
pub(super) struct ChatView {
    /// The chat screen is open: every row fully shown, the focused height,
    /// the scroll bar.
    pub focused: bool,
    pub scroll: usize,
    pub new_since_scroll: bool,
    /// Lines Chat Delay holds back (`chat.queue`).
    pub queued: usize,
}

/// v1.0 final F6: `ChatComponent.extractRenderState` under Java's chat
/// options: Chat Text Size scales it about the top left, Width and the two
/// Heights size it, Line Spacing spaces its rows, Chat Text Opacity and Text
/// Background Opacity shade it, and the focused chat shows a scroll bar
/// when there are more rows than fit. `prefix` draws the start of a row in
/// its own colour (the yellow, bold debug prefix) and returns its width.
pub(super) fn draw_chat(
    ui: &mut Ui,
    settings: &crate::app::settings::Settings,
    rows: &[(String, std::time::Instant)],
    view: &ChatView,
    prefix: impl Fn(&mut Ui, f32, f32, &str, f32, f32) -> (f32, usize),
) {
    let chat = &settings.chat;
    let total = rows.len();
    if (total == 0 && view.queued == 0) || chat.scale <= 0.0 {
        return;
    }
    let (_, sh) = ui.size();
    let k = chat.scale;
    let max_width = (ChatOptions::width_px(chat.width) as f32 / k).ceil();
    let chat_bottom = ((sh - 40.0) / k).floor();
    let text_opacity = chat.opacity * 0.9 + 0.1;
    let background = settings.text_background_opacity;
    let spacing = f64::from(chat.line_spacing);
    let entry_h = (9.0 * (spacing + 1.0)) as i32 as f32;
    let to_text = (8.0 * (spacing + 1.0) - 4.0 * spacing).round() as f32;
    let per_page = chat.lines_per_page(view.focused);
    let scroll = view.scroll.min(total);
    // Scaled chat coordinates to the screen: `pose.scale(k)`, then 4 right.
    let px = |x: f32| (x + 4.0) * k;
    let shown = (total - scroll).min(per_page);
    let mut count = 0;
    for i in 0..shown {
        let (text, added) = &rows[total - scroll - 1 - i];
        let alpha = if view.focused { 1.0 } else { chat_alpha(added.elapsed().as_secs_f32()) };
        if alpha <= 1e-5 {
            continue;
        }
        count += 1;
        let entry_bottom = chat_bottom - i as f32 * entry_h;
        let top = entry_bottom - entry_h;
        ui.rect(px(-4.0), top * k, (max_width + 12.0) * k, entry_h * k, crate::render::ui::shade(alpha * background));
        let a = alpha * text_opacity;
        // Java skips text whose alpha byte is 3 or less (`ARGB.alpha`).
        if a * 255.0 <= 3.0 {
            continue;
        }
        let y = (entry_bottom - to_text) * k;
        let (w, skip) = prefix(ui, px(0.0), y, text, a, k);
        ui.text_scaled(px(0.0) + w, y, &text[skip..], [1.0, 1.0, 1.0, a], k);
    }
    if view.queued > 0 {
        ui.rect(px(-2.0), chat_bottom * k, (max_width + 6.0) * k, 9.0 * k, crate::render::ui::shade(background));
        let line = format!("[+{} pending line(s)]", view.queued);
        ui.text_scaled(px(0.0), (chat_bottom + 1.0) * k, &line, [1.0, 1.0, 1.0, 0.5 * text_opacity], k);
    }
    // The scroll bar, in Java's integer pixels.
    if total > 0 && view.focused {
        let (count, total, entry_h, bottom) = (count as i64, total as i64, entry_h as i64, chat_bottom as i64);
        let chat_height = count * entry_h;
        let virtual_height = total * entry_h;
        if virtual_height != chat_height && virtual_height > 0 {
            let y = scroll as i64 * chat_height / total - bottom;
            let height = chat_height * chat_height / virtual_height;
            let alpha = if y > 0 { 170.0 } else { 96.0 } / 255.0;
            let colour = if view.new_since_scroll { 0xCC3333 } else { 0x3333AA };
            let rgb = |c: u32| [((c >> 16) & 255) as f32 / 255.0, ((c >> 8) & 255) as f32 / 255.0, (c & 255) as f32 / 255.0];
            let [r, g, b] = rgb(colour);
            let x = max_width + 4.0;
            let (y0, y1) = ((-y - height) as f32, -y as f32);
            ui.rect(px(x), y0 * k, 2.0 * k, (y1 - y0) * k, [r, g, b, alpha]);
            ui.rect(px(x + 1.0), y0 * k, k, (y1 - y0) * k, [0.8, 0.8, 0.8, alpha]);
        }
    }
}

/// v1.0 gap track 6d: a chat line's opacity `seconds` after it came, as
/// `ChatComponent.AlphaCalculator.timeBased` computes it from its 200-tick
/// life: fully shown for 9 seconds, then fading over the last second as
/// the square of what is left.
pub(super) fn chat_alpha(seconds: f32) -> f32 {
    let t = ((1.0 - seconds * 20.0 / 200.0) * 10.0).clamp(0.0, 1.0);
    t * t
}

/// What a finished console command prints: failures always, successes only
/// with `send_command_feedback` (Java's `CommandSourceStack.sendSuccess`).
fn console_line(result: Result<String, String>, feedback: bool) -> Option<String> {
    match result {
        Ok(text) => feedback.then_some(text),
        Err(e) => Some(format!("Error: {e}")),
    }
}

impl Game {
    /// Open Chat (`chat`) or Open Command: the console, starting with a
    /// slash unless chatting with LAN players.
    pub(super) fn open_console(&mut self, chat: bool) {
        self.console.open = true;
        self.console.input =
            if chat && (self.lan.host.is_some() || self.lan.client.is_some()) { String::new() } else { "/".into() };
        self.keys.clear();
        self.body.left_held = false;
        self.body.right_held = false;
        self.body.actions.reset();
        self.session.cancel_mining();
        self.session.cancel_consumption();
        self.set_grab(false);
    }

    /// Consume text entry before gameplay bindings so commands cannot move or drop items.
    pub(super) fn console_key(&mut self, event: &KeyEvent) -> bool {
        let PhysicalKey::Code(code) = event.physical_key else {
            return self.console.open;
        };
        if !self.console.open {
            // Opened by the Open Chat and Open Command bindings (`Game::press`).
            return false;
        }
        match code {
            KeyCode::Escape => {
                self.console.open = false;
                // `ChatComponent.resetChatScroll` when the chat closes.
                self.console.scroll = 0;
                self.console.new_since_scroll = false;
                self.set_grab(true);
            }
            KeyCode::Enter | KeyCode::NumpadEnter => {
                let input = std::mem::replace(&mut self.console.input, "/".into());
                self.console.history_index = None;
                // Java's server refuses chat and commands from a player whose
                // chat is Hidden (`chat.disabled.options`, red), and they
                // see it only on the action bar.
                if self.settings.chat.visibility == ChatVisibility::Hidden
                    && !input.trim().is_empty()
                    && input.trim() != "/"
                {
                    self.show_popup("Chat disabled in client options.");
                } else if !input.trim().is_empty() && input.trim() != "/" {
                    self.console.scroll = 0;
                    self.console.new_since_scroll = false;
                    if self.console.history.len() == 64 {
                        self.console.history.remove(0);
                    }
                    self.console.history.push(input.clone());
                    self.console.push(input.clone());
                    let words: Vec<&str> = input.trim_start_matches('/').split_whitespace().collect();
                    let result = if let Some(result) = self.lan_console(&input) {
                        result
                    } else {
                        match words[..] {
                            ["splitscreen"] => self.split_command(""),
                            ["splitscreen", arg] => self.split_command(arg),
                            ["splitscreen", ..] => Err("usage: /splitscreen [player|off|side|stacked]".into()),
                            ["save-all"] => {
                                self.save();
                                Ok("Saved the game".into())
                            }
                            _ => Command::parse(&input).and_then(|c| self.host_command(c)),
                        }
                    };
                    // Java's `CommandSourceStack.sendSuccess` is silent without
                    // `send_command_feedback`; failures are always shown.
                    if let Some(line) = console_line(result, self.gamerules.bool("send_command_feedback")) {
                        self.console.push(line);
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
            // `ChatScreen.keyPressed`: a page less one.
            KeyCode::PageUp => {
                let page = self.settings.chat.lines_per_page(true) as i32;
                self.scroll_chat(page - 1);
            }
            KeyCode::PageDown => {
                let page = self.settings.chat.lines_per_page(true) as i32;
                self.scroll_chat(-page + 1);
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
                let left = self.session.inventory.add(item, count);
                return Ok(format!("Gave {} {}", count - left, item.name()));
            }
            Command::Clear => {
                let count: u32 = self.session.inventory.take_all().into_iter().map(|stack| stack.count as u32).sum();
                self.session.work.fill(None);
                self.craft.cells.fill(None);
                return Ok(format!("Removed {count} items"));
            }
            Command::Kill => {
                let was_dead = self.session.vitals.is_dead();
                self.session.vitals.damage(f32::MAX, voxelcraft::simulation::damage::Cause::GENERIC_KILL, false);
                if !was_dead && self.session.vitals.is_dead() {
                    self.on_death();
                }
            }
            Command::SummonLightning(pos) => {
                self.mobs.entities.summon_lightning(pos.resolve(self.session.player.pos)?, false);
                return Ok("Summoned lightning bolt".into());
            }
            Command::Summon(kind, pos) => {
                let at = pos.resolve(self.session.player.pos)?;
                self.mobs.entities.spawn(kind, at);
                return Ok(format!("Summoned {} at {:.1} {:.1} {:.1}", kind.name(), at.x, at.y, at.z));
            }
            Command::Mode(mode) => self.set_mode(mode),
            Command::Teleport(pos, turn) => {
                voxelcraft::agent::command::teleport(&mut self.session.player, pos, turn)?;
                self.session.player.vel = glam::DVec3::ZERO;
                self.session.vitals.reset_fall();
                self.previous_eye = self.session.player.eye();
            }
            // The camera reads the player's yaw and pitch each frame without
            // interpolation, so the new facing shows at once; mouse motion
            // turns from it.
            Command::Rotate(turn) => voxelcraft::agent::command::rotate(&mut self.session.player, turn)?,
            Command::SpawnPoint(pos) => {
                let p = pos.resolve(self.session.player.pos)?.floor().as_ivec3();
                self.session.spawn_bed = None;
                self.session.spawn_anchor = None;
                self.session.spawn_point = Some(p);
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
                // Java's DifficultyCommand forces the change past the
                // difficulty lock (`setDifficulty(difficulty, true)`); only
                // the Options button respects it.
                self.difficulty = difficulty;
                return Ok(format!("Set difficulty to {difficulty}"));
            }
            Command::GameRule { name, value: None } => {
                let value = self.gamerules.get(&name).ok_or_else(|| GameRules::unknown(&name))?;
                return Ok(format!("{name} = {value}"));
            }
            Command::GameRule { name, value: Some(text) } => {
                let value = self.gamerules.set(&name, &text)?;
                self.world.set_tile_drops(self.gamerules.bool("block_drops"));
                return Ok(format!("Set {name} to {value}"));
            }
            Command::Seed => return Ok(format!("Seed: [{}]", self.world.generator.seed)),
            Command::SetWorldSpawn(pos) => {
                let p = pos.resolve(self.session.player.pos)?.floor().as_ivec3();
                self.world_spawn = p;
                return Ok(format!("Set the world spawn point to {} {} {}", p.x, p.y, p.z));
            }
            Command::LocateStructure(name) => {
                let key = name.strip_prefix("minecraft:").unwrap_or(&name);
                let at = match key {
                    "ocean_ruin_warm" | "ocean_ruin_cold" if self.dimension == Dimension::Overworld => {
                        self.world.generator.temples.nearest_ocean_ruin(
                            &self.world.generator,
                            IVec2::new(
                                self.session.player.pos.x.floor() as i32,
                                self.session.player.pos.z.floor() as i32,
                            ),
                            6400,
                            key == "ocean_ruin_warm",
                        )
                    }
                    "ocean_ruin_warm" | "ocean_ruin_cold" => None,
                    "ruined_portal" => self.world.generator.ruined_portals.nearest(
                        &self.world.generator,
                        self.session.player.pos.floor().as_ivec3(),
                        10,
                    ),
                    "buried_treasure" => self.world.generator.buried_treasure.nearest_buried_treasure(
                        &self.world.generator,
                        self.session.player.pos.floor().as_ivec3(),
                        400,
                    ),
                    "village" => self
                        .world
                        .generator
                        .villages
                        .nearest(&self.world.generator, self.session.player.pos.floor().as_ivec3()),
                    "stronghold" => {
                        self.world.generator.strongholds.nearest(self.session.player.pos.floor().as_ivec3())
                    }
                    "fortress" | "nether_fortress" => self.world.generator.nearest_fortress(IVec2::new(
                        self.session.player.pos.x.floor() as i32,
                        self.session.player.pos.z.floor() as i32,
                    )),
                    "bastion_remnant" | "bastion" if self.dimension == Dimension::Nether => {
                        crate::world::bastion::Bastions::with_generation(
                            self.world.generator.seed,
                            self.world.generator.generation,
                        )
                        .nearest(IVec2::new(
                            self.session.player.pos.x.floor() as i32,
                            self.session.player.pos.z.floor() as i32,
                        ))
                    }
                    "bastion_remnant" | "bastion" => None,
                    "nether_fossil" => self.world.generator.nearest_nether_fossil(
                        IVec2::new(self.session.player.pos.x.floor() as i32, self.session.player.pos.z.floor() as i32),
                        6400,
                    ),
                    "ancient_city" => self.world.generator.ancient_cities.nearest(
                        &self.world.generator,
                        IVec2::new(self.session.player.pos.x.floor() as i32, self.session.player.pos.z.floor() as i32),
                        6400,
                    ),
                    "trail_ruins" => self.world.generator.trail_ruins.nearest(
                        &self.world.generator,
                        IVec2::new(self.session.player.pos.x.floor() as i32, self.session.player.pos.z.floor() as i32),
                        6400,
                    ),
                    "trial_chambers" => self.world.generator.trial_chambers.nearest(
                        &self.world.generator,
                        IVec2::new(self.session.player.pos.x.floor() as i32, self.session.player.pos.z.floor() as i32),
                        6400,
                    ),
                    // v0.16 Track A: generation 5's End cities.
                    "end_city" => self.world.generator.end().and_then(|e| {
                        e.nearest_city(
                            IVec2::new(
                                self.session.player.pos.x.floor() as i32,
                                self.session.player.pos.z.floor() as i32,
                            ),
                            6400,
                        )
                    }),
                    "mineshaft" | "abandoned_mineshaft" => self
                        .world
                        .generator
                        .mineshafts
                        .nearest(&*self.world.generator, self.session.player.pos.floor().as_ivec3()),
                    other
                        if self.dimension == Dimension::Overworld
                            && crate::world::temples::Kind::from_name(other).is_some() =>
                    {
                        let kind = crate::world::temples::Kind::from_name(other).unwrap();
                        let origin = IVec2::new(
                            self.session.player.pos.x.floor() as i32,
                            self.session.player.pos.z.floor() as i32,
                        );
                        self.world.generator.temples.nearest(&self.world.generator, kind, origin, 6400)
                    }
                    other if crate::world::temples::Kind::from_name(other).is_some() => None,
                    _ => return Err(format!("unknown structure: {name}")),
                }
                .ok_or("Could not find that structure nearby")?;
                return Ok(self.locate_message(at, key));
            }
            Command::LocateBiome(biome) => {
                let origin =
                    IVec2::new(self.session.player.pos.x.floor() as i32, self.session.player.pos.z.floor() as i32);
                let at = self
                    .world
                    .generator
                    .nearest_biome(origin, biome, 12_800)
                    .ok_or("Could not find that biome nearby")?;
                return Ok(self.locate_message(at, biome.name()));
            }
            Command::LocateNetherBiome(biome) => {
                let at = self
                    .world
                    .generator
                    .nearest_nether_biome(self.session.player.pos.floor().as_ivec3(), biome, 12_800)
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
                        crate::world::terrain::Dimension::End => {
                            super::dimension::Arrival::EndSpawn { vel: glam::DVec3::ZERO }
                        }
                        crate::world::terrain::Dimension::Nether => super::dimension::Arrival::Portal(
                            crate::world::portal::PortalTrip::without_portal(glam::IVec3::new(0, 64, 0)),
                        ),
                        crate::world::terrain::Dimension::Overworld => super::dimension::Arrival::Respawn,
                    };
                    if !self.switch_dimension(to, arrival) {
                        return Err("the world couldn't be saved, so it stayed".into());
                    }
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
                if let Some(chime) = change.apply(&mut self.session.vitals.xp) {
                    self.audio.play(crate::audio::sounds::Sound::LevelUp, None, chime, (1.0, 1.0));
                }
                return Ok(self.xp_summary());
            }
            Command::XpQuery => return Ok(self.xp_summary()),
            Command::Enchant(e, level) => {
                let slot = &mut self.session.inventory.slots[self.body.actions.selected];
                let stack = crate::enchant::command(*slot, e, level)?;
                *slot = Some(stack);
                return Ok(format!("Applied {} to {}", e.describe(level), stack.item.name()));
            }
            Command::Effect(change) => {
                let (damage, text) = change.apply(&mut self.session.vitals, "Host")?;
                self.damage_player(damage, voxelcraft::simulation::damage::Cause::MAGIC);
                return Ok(text);
            }
            Command::Trade(index) => {
                let id = self
                    .mobs
                    .entities
                    .target_merchant(
                        &self.world,
                        self.session.player.eye(),
                        self.session.player.forward().as_dvec3(),
                        self.session.entity_reach(),
                    )
                    .ok_or("no villager within reach")?;
                if let Some(i) = index {
                    // As the trading screen does: an offer unlocked since can be an unresolved map.
                    self.mobs.entities.prepare_merchant_maps(&mut self.world, id);
                    let xp = self.mobs.entities.trade_for(id, i, &mut self.session.inventory, self.session.id)?;
                    self.mobs.entities.spawn_xp(self.session.player.pos, xp);
                } else {
                    self.body.container = super::Container::Trading(id);
                    self.toggle_inventory();
                }
            }
            Command::Observe(_) => {
                return Ok(format!(
                    "{} at {:.1}, {:.1}, {:.1}",
                    self.dimension.name(),
                    self.session.player.pos.x,
                    self.session.player.pos.y,
                    self.session.player.pos.z
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
        // Whole days and the remainder in constant time: huge `/time add`
        // values must not loop once per day.
        let now = (self.day_time * 24_000.0).round() as i64;
        let total = now.saturating_add(ticks);
        let days = total.div_euclid(24_000);
        self.day_time = total.rem_euclid(24_000) as f64 / 24_000.0;
        self.day_count = self.day_count.saturating_add(days);
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
        let blocks = (there - self.session.player.pos).length().round() as i32;
        format!("The nearest {label} is at {} {} {} ({blocks} blocks away)", at.x, at.y, at.z)
    }

    /// Formats the host's level and progress toward the next level.
    fn xp_summary(&self) -> String {
        let xp = &self.session.vitals.xp;
        let next = crate::simulation::experience::points_to_next(xp.level);
        format!("Level {} ({}/{} points to the next)", xp.level, xp.points, next)
    }

    /// The open chat, as Java's `ChatScreen` draws it: the focused chat
    /// (every row, the focused height, the scroll bar) above the input box,
    /// a strip of black at the text background's opacity two pixels in from
    /// the screen's edges with the input at (4, height - 12).
    pub(super) fn console_ui(&self, ui: &mut Ui) {
        let (w, h) = ui.size();
        let rows = chat_rows(&self.console, &self.settings.chat);
        let view = ChatView {
            focused: true,
            scroll: self.console.scroll,
            new_since_scroll: self.console.new_since_scroll,
            queued: self.console.queue.len(),
        };
        draw_chat(ui, &self.settings, &rows, &view, |ui, x, y, text, a, k| self.chat_prefix(ui, x, y, text, a, k));
        ui.rect(2.0, h - 14.0, w - 4.0, 12.0, crate::render::ui::shade(self.settings.background_opacity(0.5)));
        // The end of the input that fits, caret included.
        let width = (w - 8.0).max(40.0);
        let input = format!("{}_", self.console.input);
        let start = input.char_indices().map(|(i, _)| i).find(|&i| Ui::text_width(&input[i..]) <= width).unwrap_or(0);
        ui.text(4.0, h - 12.0, &input[start..], WHITE);
    }

    /// `ChatScreen.mouseScrolled`: the (sensitivity-scaled) wheel clamped
    /// to one notch, seven rows a notch unless Shift is held.
    pub(super) fn chat_wheel(&mut self, lines: f64) {
        let s = &self.settings;
        let lines = if s.discrete_scroll && lines != 0.0 { lines.signum() } else { lines } * s.scroll_sensitivity;
        let mut by = lines.clamp(-1.0, 1.0);
        if !self.modifiers.shift_key() {
            by *= 7.0;
        }
        self.scroll_chat(by as i32);
    }

    fn scroll_chat(&mut self, by: i32) {
        let rows = chat_rows(&self.console, &self.settings.chat).len();
        let per_page = self.settings.chat.lines_per_page(true);
        self.console.scroll_chat(by, rows, per_page);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// v1.0 gap track 6d: `ChatComponent.AlphaCalculator.timeBased`.
    #[test]
    fn chat_lines_fade_as_javas_do() {
        assert_eq!(chat_alpha(0.0), 1.0);
        assert_eq!(chat_alpha(9.0), 1.0);
        assert!((chat_alpha(9.5) - 0.25).abs() < 1e-4);
        // 195 ticks: (1 - 195/200) * 10 = 0.25, squared.
        assert!((chat_alpha(9.75) - 0.0625).abs() < 1e-4);
        assert_eq!(chat_alpha(10.0), 0.0);
        assert_eq!(chat_alpha(60.0), 0.0);
        let mut c = Console::default();
        for i in 0..70 {
            c.push(format!("line {i}"));
        }
        assert_eq!((c.lines.len(), c.added.len()), (64, 64));
        assert_eq!(c.lines.front().map(String::as_str), Some("line 6"));
    }

    #[test]
    fn send_command_feedback_silences_successes_but_never_failures() {
        let ok = || Ok("Set the time to 1000".to_string());
        assert_eq!(console_line(ok(), true).as_deref(), Some("Set the time to 1000"));
        assert_eq!(console_line(ok(), false), None);
        for feedback in [true, false] {
            assert_eq!(
                console_line(Err("unknown gamerule: x".into()), feedback).as_deref(),
                Some("Error: unknown gamerule: x")
            );
        }
        assert!(voxelcraft::rules::GameRules::default().bool("send_command_feedback"), "Java default is true");
    }
}
