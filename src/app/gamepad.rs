//! Local controller players for split-screen. Pressing Start on a gamepad
//! joins it as a player with its own view. Each controller drives an engine
//! `Agent` (the same session hosted agents use) through a profile named
//! Player2..Player8, so inventory and position are kept between sessions.
//!
//! Bedrock-style layout: left stick moves, right stick looks, A jumps (double
//! tap to fly in creative), RT mines and attacks, LT places or eats, LB/RB cycle the
//! hotbar (holding food eats it), B drops, left stick click sprints, right stick click toggles
//! sneaking. LT on a door, gate, chest or crafting table uses it instead.
//! Start opens a pause menu (resume or leave) and Y the inventory; see
//! `pad_menu`. The world keeps running while a player is in a menu.

use std::time::{Duration, Instant};

use gilrs::{Axis, Button, EventType, GamepadId, Gilrs};
use glam::Vec2;

use super::pad_menu::{Action, Menu, Nav, Tab};
use super::{Game, GameMode};
use crate::player::MoveInput;
use voxelcraft::agent::{Agent, Command};

/// Stick travel ignored around the centre, as a fraction of full tilt.
const DEADZONE: f32 = 0.15;
/// Full right-stick tilt turns this fast (radians per second).
const LOOK_RATE: f32 = 4.0;
/// Trigger travel that counts as pressed.
const TRIGGER: f32 = 0.3;
/// Two jumps this close toggle flight in creative (Java uses 7 ticks).
const DOUBLE_TAP_TICKS: u32 = 7;
/// Menu cursor repeat while the left stick is held over.
const NAV_DELAY: Duration = Duration::from_millis(350);
const NAV_REPEAT: Duration = Duration::from_millis(120);

/// One-shot actions pressed since the last gameplay tick.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Press {
    Jump,
    Attack,
    Use,
    Next,
    Previous,
    Drop,
    Sprint,
    Sneak,
}

/// Held controls, sampled every frame.
#[derive(Default, Clone, Copy)]
struct Held {
    walk: Vec2,
    look: Vec2,
    jump: bool,
    mine: bool,
    place: bool,
}

/// One joined controller.
struct Seat {
    /// Agent profile this controller plays as.
    name: String,
    /// Identifies the same controller when it reconnects.
    uuid: [u8; 16],
    /// None while unplugged.
    id: Option<GamepadId>,
    held: Held,
    presses: Vec<Press>,
    sneaking: bool,
    sprinting: bool,
    /// Ticks since the last jump press.
    since_jump: u32,
    /// Open screen; gameplay input is ignored while it's up.
    menu: Option<Menu>,
    /// Left-stick menu direction and when it next repeats.
    nav: Option<(Nav, Instant)>,
    /// LT opened or toggled something; it won't place until released.
    used: bool,
}

/// What a seat asks of its agent for one tick.
struct TickInput {
    input: MoveInput,
    mine: bool,
    place: bool,
    actions: Vec<Press>,
    /// A was pressed (respawns when dead).
    jumped: bool,
    toggle_flight: bool,
}

impl Seat {
    fn new(name: String, uuid: [u8; 16], id: Option<GamepadId>) -> Self {
        Self {
            name,
            uuid,
            id,
            held: Held::default(),
            presses: Vec::new(),
            sneaking: false,
            sprinting: false,
            since_jump: u32::MAX,
            menu: None,
            nav: None,
            used: false,
        }
    }

    /// Opens a screen, dropping held gameplay input.
    fn open(&mut self, menu: Menu) {
        self.menu = Some(menu);
        self.held = Held::default();
        self.presses.clear();
        self.nav = None;
    }

    /// Menu moves from the left stick: one on a flick, then repeating.
    fn stick_nav(&mut self, stick: Vec2, now: Instant) -> Option<Nav> {
        let dir = if stick.length() < 0.6 {
            None
        } else if stick.x.abs() > stick.y.abs() {
            Some(if stick.x > 0.0 { Nav::Right } else { Nav::Left })
        } else {
            Some(if stick.y > 0.0 { Nav::Up } else { Nav::Down })
        };
        match (dir, self.nav) {
            (None, _) => {
                self.nav = None;
                None
            }
            (Some(d), Some((held, at))) if d == held => {
                if now < at {
                    return None;
                }
                self.nav = Some((d, now + NAV_REPEAT));
                Some(d)
            }
            (Some(d), _) => {
                self.nav = Some((d, now + NAV_DELAY));
                Some(d)
            }
        }
    }

    /// Consume this tick's presses into agent input. Sprint lasts until the
    /// player stops walking forward, like Java's sprint key.
    fn tick_input(&mut self) -> TickInput {
        let mut toggle_flight = false;
        let mut jumped = false;
        let mut actions = Vec::new();
        for press in self.presses.drain(..) {
            match press {
                Press::Sneak => self.sneaking = !self.sneaking,
                Press::Sprint => self.sprinting = true,
                Press::Jump => {
                    jumped = true;
                    toggle_flight |= self.since_jump <= DOUBLE_TAP_TICKS;
                    self.since_jump = if toggle_flight { u32::MAX } else { 0 };
                }
                other => actions.push(other),
            }
        }
        self.since_jump = self.since_jump.saturating_add(1);
        let walk = self.held.walk;
        self.sprinting &= walk.y > 0.5;
        TickInput {
            input: MoveInput {
                forward: walk.y as f64,
                right: walk.x as f64,
                jump: self.held.jump,
                descend: self.sneaking,
                sprint: self.sprinting,
            },
            mine: self.held.mine,
            place: self.held.place,
            actions,
            jumped,
            toggle_flight,
        }
    }
}

/// Radial deadzone, rescaled so motion starts smoothly at its edge.
fn deadzone(v: Vec2) -> Vec2 {
    let len = v.length();
    if len <= DEADZONE {
        return Vec2::ZERO;
    }
    v / len * ((len.min(1.0) - DEADZONE) / (1.0 - DEADZONE))
}

/// Right-stick turn for one frame. Squaring the tilt keeps small aiming
/// adjustments precise while full tilt still turns quickly.
fn look_delta(stick: Vec2, dt: f32, sensitivity: f32) -> Vec2 {
    let v = deadzone(stick);
    v * v.length() * LOOK_RATE * sensitivity * dt
}

#[derive(Default)]
pub(super) struct Pads {
    gilrs: Option<Gilrs>,
    seats: Vec<Seat>,
}

impl Pads {
    pub fn new() -> Self {
        let gilrs = Gilrs::new().map_err(|e| log::warn!("gamepads unavailable: {e}")).ok();
        Self { gilrs, seats: Vec::new() }
    }

    /// Whether a local controller plays as this profile.
    pub fn seated(&self, name: &str) -> bool {
        self.seats.iter().any(|s| s.name == name)
    }
}

fn press_for(button: Button) -> Option<Press> {
    Some(match button {
        Button::South => Press::Jump,
        Button::RightTrigger2 => Press::Attack,
        Button::LeftTrigger2 => Press::Use,
        Button::RightTrigger => Press::Next,
        Button::LeftTrigger => Press::Previous,
        Button::East => Press::Drop,
        Button::LeftThumb => Press::Sprint,
        Button::RightThumb => Press::Sneak,
        _ => return None,
    })
}

fn nav_for(button: Button) -> Option<Nav> {
    Some(match button {
        Button::DPadUp => Nav::Up,
        Button::DPadDown => Nav::Down,
        Button::DPadLeft => Nav::Left,
        Button::DPadRight => Nav::Right,
        Button::South => Nav::A,
        Button::East => Nav::B,
        Button::West => Nav::X,
        Button::North => Nav::Y,
        Button::LeftTrigger => Nav::Lb,
        Button::RightTrigger => Nav::Rb,
        Button::Start => Nav::Start,
        _ => return None,
    })
}

impl Game {
    /// Drain controller events every frame: join, hot-plug, presses and
    /// right-stick look (applied per frame, like the mouse).
    pub(super) fn poll_pads(&mut self, dt: f32, paused: bool) {
        let Some(gilrs) = self.pads.gilrs.as_mut() else {
            return;
        };
        let mut joins = Vec::new();
        let mut popups = Vec::new();
        let mut navs = Vec::new();
        let mut closing = Vec::new();
        while let Some(event) = gilrs.next_event() {
            let seat = self.pads.seats.iter_mut().find(|s| s.id == Some(event.id));
            match (event.event, seat) {
                (EventType::ButtonPressed(Button::Start, _), None) => joins.push(event.id),
                (EventType::ButtonPressed(button, _), Some(seat)) => match button {
                    _ if seat.menu.is_some() => navs.extend(nav_for(button).map(|n| (seat.name.clone(), n))),
                    Button::Start => seat.open(Menu::Pause { choice: 0 }),
                    Button::North => seat.open(Menu::items(Tab::Inventory)),
                    _ => seat.presses.extend(press_for(button)),
                },
                (EventType::Connected, None) => {
                    let uuid = gilrs.gamepad(event.id).uuid();
                    if let Some(seat) = self.pads.seats.iter_mut().find(|s| s.id.is_none() && s.uuid == uuid) {
                        seat.id = Some(event.id);
                        popups.push(format!("{}'s controller reconnected", seat.name));
                    } else {
                        popups.push("Controller connected: press Start to join".into());
                    }
                }
                (EventType::Disconnected, Some(seat)) => {
                    seat.id = None;
                    seat.held = Held::default();
                    seat.presses.clear();
                    if seat.menu.take().is_some() {
                        closing.push(seat.name.clone());
                    }
                    popups.push(format!("{}'s controller disconnected", seat.name));
                }
                _ => {}
            }
        }
        let now = Instant::now();
        for seat in &mut self.pads.seats {
            let Some(id) = seat.id else { continue };
            let pad = gilrs.gamepad(id);
            if seat.menu.is_some() {
                let stick = Vec2::new(pad.value(Axis::LeftStickX), pad.value(Axis::LeftStickY));
                navs.extend(seat.stick_nav(stick, now).map(|n| (seat.name.clone(), n)));
                continue;
            }
            let trigger = |b: Button| pad.button_data(b).is_some_and(|d| d.value() > TRIGGER);
            seat.held = Held {
                walk: deadzone(Vec2::new(pad.value(Axis::LeftStickX), pad.value(Axis::LeftStickY))),
                look: Vec2::new(pad.value(Axis::RightStickX), pad.value(Axis::RightStickY)),
                jump: pad.is_pressed(Button::South),
                mine: trigger(Button::RightTrigger2),
                place: trigger(Button::LeftTrigger2),
            };
        }
        if !paused {
            for seat in &self.pads.seats {
                if let Some(bot) = self.agents.players.get_mut(&seat.name)
                    && seat.menu.is_none()
                    && !bot.agent.vitals.is_dead()
                {
                    let d = look_delta(seat.held.look, dt, self.settings.sensitivity);
                    bot.agent.player.look(d.x, -d.y);
                }
            }
        }
        for id in joins {
            match self.join_pad(Some(id)) {
                Ok(name) => popups.push(format!("{name} joined")),
                Err(e) => popups.push(e),
            }
        }
        for name in closing {
            self.close_pad_menu(&name);
        }
        for (name, nav) in navs {
            if self.pad_nav(&name, nav) {
                popups.push(format!("{name} left the game"));
            }
        }
        if let Some(text) = popups.last() {
            self.show_popup(text);
        }
    }

    /// Seat a controller as the first free local profile and give it a view.
    fn join_pad(&mut self, id: Option<GamepadId>) -> Result<String, String> {
        if self.split.follow.len() >= super::split::MAX_VIEWS - 1 {
            return Err("Split-screen is full".into());
        }
        let name = (2..=8)
            .map(|n| format!("Player{n}"))
            .find(|n| !self.pads.seated(n) && !self.agents.players.get(n).is_some_and(|b| b.active))
            .ok_or("No free player profile")?;
        if !self.agents.players.contains_key(&name) {
            let mut agent = Agent::new(self.beside_host());
            agent.player.yaw = self.player.yaw;
            agent.creative = self.mode == GameMode::Creative;
            agent.player.can_fly = agent.creative;
            self.agents.insert(name.clone(), agent);
        }
        self.agents.players.get_mut(&name).unwrap().active = true;
        if !self.split.follow.contains(&name) {
            self.split.follow.push(name.clone());
        }
        let uuid = self.pads.gilrs.as_ref().zip(id).map_or([0; 16], |(g, id)| g.gamepad(id).uuid());
        self.pads.seats.push(Seat::new(name.clone(), uuid, id));
        Ok(name)
    }

    /// A free standing spot next to the host, or the host's own (players
    /// don't collide, but a shared spot puts the camera inside the host).
    fn beside_host(&self) -> glam::DVec3 {
        let host = self.player.pos;
        let open = |p: glam::IVec3| self.world.get_block(p).is_some_and(|b| !b.is_solid());
        [(1, 0), (-1, 0), (0, 1), (0, -1), (1, 1), (-1, -1), (1, -1), (-1, 1)]
            .into_iter()
            .map(|(x, z)| host + glam::DVec3::new(x as f64, 0.0, z as f64))
            .find(|p| {
                let feet = p.floor().as_ivec3();
                open(feet) && open(feet + glam::IVec3::Y) && !open(feet - glam::IVec3::Y)
            })
            .unwrap_or(host)
    }

    /// Free the seat and its view. The profile stays saved for next time.
    fn leave_pad(&mut self, name: &str) {
        self.close_pad_menu(name);
        self.pads.seats.retain(|s| s.name != name);
        self.split.follow.retain(|n| n != name);
        if let Some(bot) = self.agents.players.get_mut(name) {
            bot.active = false;
            bot.agent.hold(MoveInput::default(), false, false);
        }
    }

    /// `--pad-player`: seat an unplugged controller player holding a copy
    /// of the host's inventory, with a screen open.
    pub(super) fn virtual_pad(&mut self, screen: &str) {
        let Ok(name) = self.join_pad(None) else { return };
        let inventory = self.inventory.clone();
        if let Some(bot) = self.agents.players.get_mut(&name) {
            bot.agent.inventory = inventory;
        }
        let seat = self.pads.seats.last_mut().unwrap();
        match screen {
            "pause" => seat.open(Menu::Pause { choice: 0 }),
            "inventory" => seat.open(Menu::items(Tab::Inventory)),
            "crafting" => seat.open(Menu::items(Tab::Crafting)),
            _ => {}
        }
    }

    /// One menu press. Returns whether the player left the game.
    fn pad_nav(&mut self, name: &str, nav: Nav) -> bool {
        let Some(i) = self.pads.seats.iter().position(|s| s.name == name) else { return false };
        let Some(mut menu) = self.pads.seats[i].menu else { return false };
        let crafts =
            if matches!(menu, Menu::Items { tab: Tab::Crafting, .. }) { self.pad_crafts(name).len() } else { 0 };
        let action = menu.press(nav, crafts);
        if action == Action::Leave {
            self.leave_pad(name);
            return true;
        }
        if self.apply_menu(name, menu, action) {
            self.pads.seats[i].menu = Some(menu);
        } else {
            self.pads.seats[i].menu = None;
            self.close_pad_menu(name);
        }
        false
    }

    /// The open screen of a controller player, for drawing.
    pub(super) fn pad_menu(&self, name: &str) -> Option<Menu> {
        self.pads.seats.iter().find(|s| s.name == name).and_then(|s| s.menu)
    }

    /// Feed one gameplay tick of controller input to each seated agent.
    /// Runs before the agents tick.
    pub(super) fn drive_pads(&mut self) {
        let mut others = self.agents.positions();
        others.push(self.player.pos);
        let mut closing = Vec::new();
        let mut doors = Vec::new();
        let mut chests = Vec::new();
        for seat in &mut self.pads.seats {
            let tick = seat.tick_input();
            let Some(bot) = self.agents.players.get_mut(&seat.name) else {
                continue;
            };
            let agent = &mut bot.agent;
            let world = &mut self.world;
            let entities = &mut self.mobs.entities;
            if let Some(menu) = seat.menu {
                // Screens close on death, and chests when they're gone or out of reach.
                let gone = match menu {
                    Menu::Items { tab: Tab::Chest(pos), .. } => {
                        !world.get_block(pos).is_some_and(crate::world::chest::is_chest)
                            || agent.player.eye().distance(pos.as_dvec3() + 0.5) > 6.0
                    }
                    Menu::Items { .. } => agent.vitals.is_dead(),
                    Menu::Pause { .. } => false,
                };
                if gone {
                    seat.menu = None;
                    closing.push(seat.name.clone());
                }
                agent.hold(MoveInput::default(), false, false);
                continue;
            }
            if agent.vitals.is_dead() {
                if tick.jumped {
                    let _ = agent.execute(Command::Respawn, world, entities, &others);
                    seat.sneaking = false;
                }
                continue;
            }
            if tick.toggle_flight && agent.player.can_fly {
                let _ = agent.execute(Command::Fly(!agent.player.flying), world, entities, &others);
            }
            for action in tick.actions {
                // Failed actions (nothing in reach, empty hand) are silent, like a missed click.
                let _ = match action {
                    Press::Attack => agent.execute(Command::Attack, world, entities, &others),
                    Press::Next => agent.execute(Command::Select((agent.selected + 1) % 9), world, entities, &others),
                    Press::Previous => {
                        agent.execute(Command::Select((agent.selected + 8) % 9), world, entities, &others)
                    }
                    Press::Drop => agent.execute(Command::Drop, world, entities, &others),
                    Press::Use if !seat.sneaking => {
                        let target = agent.target(world).map(|(p, _)| p);
                        match target.and_then(|p| Some((p, world.get_block(p)?))) {
                            Some((pos, b)) if b.is_door() || b.is_gate() => {
                                doors.push((pos, agent.player.forward()));
                                seat.used = true;
                            }
                            Some((pos, b)) if crate::world::chest::is_chest(b) => {
                                seat.open(Menu::items(Tab::Chest(pos)));
                                chests.push(pos);
                                seat.used = true;
                            }
                            Some((_, b)) if b.base() == crate::world::block::Block::CRAFTING_TABLE => {
                                seat.open(Menu::items(Tab::Crafting));
                                seat.used = true;
                            }
                            _ => {}
                        }
                        Ok(())
                    }
                    _ => Ok(()),
                };
            }
            if seat.menu.is_some() {
                agent.hold(MoveInput::default(), false, false);
                continue;
            }
            seat.used &= tick.place;
            let food = agent.inventory.get(agent.selected).is_some_and(|s| s.item.food().is_some());
            let using = tick.place && !seat.used;
            if using && !food {
                let _ = agent.execute(Command::Place, world, entities, &others);
            }
            agent.hold(tick.input, tick.mine, using && food);
        }
        for name in closing {
            self.close_pad_menu(&name);
        }
        for (pos, forward) in doors {
            self.toggle_door(pos, forward);
        }
        for pos in chests {
            self.chest_sound(pos, 0.9);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seat() -> Seat {
        Seat::new("Player2".into(), [0; 16], None)
    }

    #[test]
    fn deadzone_ignores_drift_and_reaches_full_tilt() {
        assert_eq!(deadzone(Vec2::new(0.1, -0.1)), Vec2::ZERO);
        assert!((deadzone(Vec2::new(0.0, 1.0)).y - 1.0).abs() < 1e-6);
        // Just past the edge starts near zero instead of jumping to 0.15.
        assert!(deadzone(Vec2::new(0.16, 0.0)).x < 0.02);
        // Diagonals beyond the unit circle are clamped.
        assert!(deadzone(Vec2::new(1.0, 1.0)).length() <= 1.0 + 1e-6);
    }

    #[test]
    fn look_is_finer_near_the_centre() {
        let half = look_delta(Vec2::new(0.5, 0.0), 1.0, 1.0).x;
        let full = look_delta(Vec2::new(1.0, 0.0), 1.0, 1.0).x;
        assert!((full - LOOK_RATE).abs() < 1e-5);
        assert!(half < full / 4.0, "{half} vs {full}");
    }

    #[test]
    fn double_tapping_jump_toggles_flight_once() {
        let mut s = seat();
        s.presses.push(Press::Jump);
        assert!(!s.tick_input().toggle_flight);
        for _ in 0..3 {
            s.tick_input();
        }
        s.presses.push(Press::Jump);
        assert!(s.tick_input().toggle_flight);
        // A third tap starts a new pair rather than toggling back.
        s.presses.push(Press::Jump);
        assert!(!s.tick_input().toggle_flight);
        for _ in 0..DOUBLE_TAP_TICKS + 1 {
            s.tick_input();
        }
        s.presses.push(Press::Jump);
        assert!(!s.tick_input().toggle_flight, "slow taps don't fly");
    }

    #[test]
    fn sprint_lasts_until_forward_is_released_and_sneak_toggles() {
        let mut s = seat();
        s.held.walk = Vec2::new(0.0, 1.0);
        s.presses.extend([Press::Sprint, Press::Sneak]);
        let t = s.tick_input();
        assert!(t.input.sprint && t.input.descend);
        assert!(s.tick_input().input.sprint);
        s.held.walk = Vec2::ZERO;
        assert!(!s.tick_input().input.sprint);
        s.held.walk = Vec2::new(0.0, 1.0);
        assert!(!s.tick_input().input.sprint, "sprint needs a new click");
        s.presses.push(Press::Sneak);
        assert!(!s.tick_input().input.descend);
    }

    #[test]
    fn other_presses_become_actions_in_order() {
        let mut s = seat();
        s.presses.extend([Press::Next, Press::Attack, Press::Drop]);
        assert_eq!(s.tick_input().actions, vec![Press::Next, Press::Attack, Press::Drop]);
        assert!(s.tick_input().actions.is_empty());
    }

    #[test]
    fn stick_menu_moves_once_then_repeats() {
        let mut s = seat();
        let t = Instant::now();
        let right = Vec2::new(0.9, 0.1);
        assert_eq!(s.stick_nav(right, t), Some(Nav::Right));
        assert_eq!(s.stick_nav(right, t + Duration::from_millis(200)), None);
        assert_eq!(s.stick_nav(right, t + NAV_DELAY), Some(Nav::Right));
        assert_eq!(s.stick_nav(right, t + NAV_DELAY + Duration::from_millis(50)), None);
        assert_eq!(s.stick_nav(right, t + NAV_DELAY + NAV_REPEAT), Some(Nav::Right));
        assert_eq!(s.stick_nav(Vec2::ZERO, t + NAV_DELAY * 2), None);
        assert_eq!(s.stick_nav(Vec2::new(0.0, -0.8), t + NAV_DELAY * 2), Some(Nav::Down));
    }
}
