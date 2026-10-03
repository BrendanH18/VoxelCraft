//! Local controller players for split-screen. Pressing Start on a gamepad
//! joins it as a player with its own view. Each controller drives an engine
//! `Agent` (the same session hosted agents use) through a profile named
//! Player2..Player8, so inventory and position are kept between sessions.
//!
//! Bedrock-style layout: left stick moves, right stick looks, A jumps (double
//! tap to fly in creative), RT mines and attacks, LT places or eats, LB/RB cycle the
//! hotbar (holding food eats it), B drops, left stick click sprints, right stick click toggles
//! sneaking. Hold View/Back to leave.

use std::time::{Duration, Instant};

use gilrs::{Axis, Button, EventType, GamepadId, Gilrs};
use glam::Vec2;

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
/// Holding View this long leaves the game.
const LEAVE_HOLD: Duration = Duration::from_secs(1);

/// One-shot actions pressed since the last gameplay tick.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Press {
    Jump,
    Attack,
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
    leave_since: Option<Instant>,
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
    fn new(name: String, uuid: [u8; 16], id: GamepadId) -> Self {
        Self {
            name,
            uuid,
            id: Some(id),
            held: Held::default(),
            presses: Vec::new(),
            sneaking: false,
            sprinting: false,
            since_jump: u32::MAX,
            leave_since: None,
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
        Button::RightTrigger => Press::Next,
        Button::LeftTrigger => Press::Previous,
        Button::East => Press::Drop,
        Button::LeftThumb => Press::Sprint,
        Button::RightThumb => Press::Sneak,
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
        while let Some(event) = gilrs.next_event() {
            let seat = self.pads.seats.iter_mut().find(|s| s.id == Some(event.id));
            match (event.event, seat) {
                (EventType::ButtonPressed(Button::Start, _), None) => joins.push(event.id),
                (EventType::ButtonPressed(button, _), Some(seat)) => {
                    if button == Button::Select {
                        seat.leave_since = Some(Instant::now());
                    }
                    seat.presses.extend(press_for(button));
                }
                (EventType::ButtonReleased(Button::Select, _), Some(seat)) => seat.leave_since = None,
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
                    seat.leave_since = None;
                    popups.push(format!("{}'s controller disconnected", seat.name));
                }
                _ => {}
            }
        }
        let mut leaving = Vec::new();
        for seat in &mut self.pads.seats {
            let Some(id) = seat.id else { continue };
            let pad = gilrs.gamepad(id);
            let trigger = |b: Button| pad.button_data(b).is_some_and(|d| d.value() > TRIGGER);
            seat.held = Held {
                walk: deadzone(Vec2::new(pad.value(Axis::LeftStickX), pad.value(Axis::LeftStickY))),
                look: Vec2::new(pad.value(Axis::RightStickX), pad.value(Axis::RightStickY)),
                jump: pad.is_pressed(Button::South),
                mine: trigger(Button::RightTrigger2),
                place: trigger(Button::LeftTrigger2),
            };
            if seat.leave_since.is_some_and(|t| t.elapsed() >= LEAVE_HOLD) {
                leaving.push(seat.name.clone());
            }
        }
        if !paused {
            for seat in &self.pads.seats {
                if let Some(bot) = self.agents.players.get_mut(&seat.name)
                    && !bot.agent.vitals.is_dead()
                {
                    let d = look_delta(seat.held.look, dt, self.settings.sensitivity);
                    bot.agent.player.look(d.x, -d.y);
                }
            }
        }
        for id in joins {
            match self.join_pad(id) {
                Ok(name) => popups.push(format!("{name} joined")),
                Err(e) => popups.push(e),
            }
        }
        for name in leaving {
            self.leave_pad(&name);
            popups.push(format!("{name} left the game"));
        }
        if let Some(text) = popups.last() {
            self.show_popup(text);
        }
    }

    /// Seat a controller as the first free local profile and give it a view.
    fn join_pad(&mut self, id: GamepadId) -> Result<String, String> {
        if self.split.follow.len() >= super::split::MAX_VIEWS - 1 {
            return Err("Split-screen is full".into());
        }
        let name = (2..=8)
            .map(|n| format!("Player{n}"))
            .find(|n| !self.pads.seated(n) && !self.agents.players.get(n).is_some_and(|b| b.active))
            .ok_or("No free player profile")?;
        if !self.agents.players.contains_key(&name) {
            let mut agent = Agent::new(self.player.pos + glam::DVec3::new(1.0, 0.0, 0.0));
            agent.player.yaw = self.player.yaw;
            agent.creative = self.mode == GameMode::Creative;
            agent.player.can_fly = agent.creative;
            self.agents.insert(name.clone(), agent);
        }
        self.agents.players.get_mut(&name).unwrap().active = true;
        if !self.split.follow.contains(&name) {
            self.split.follow.push(name.clone());
        }
        let uuid = self.pads.gilrs.as_ref().map_or([0; 16], |g| g.gamepad(id).uuid());
        self.pads.seats.push(Seat::new(name.clone(), uuid, id));
        Ok(name)
    }

    /// Free the seat and its view. The profile stays saved for next time.
    fn leave_pad(&mut self, name: &str) {
        self.pads.seats.retain(|s| s.name != name);
        self.split.follow.retain(|n| n != name);
        if let Some(bot) = self.agents.players.get_mut(name) {
            bot.active = false;
            bot.agent.hold(MoveInput::default(), false, false);
        }
    }

    /// Feed one gameplay tick of controller input to each seated agent.
    /// Runs before the agents tick.
    pub(super) fn drive_pads(&mut self) {
        let mut others = self.agents.positions();
        others.push(self.player.pos);
        for seat in &mut self.pads.seats {
            let tick = seat.tick_input();
            let Some(bot) = self.agents.players.get_mut(&seat.name) else {
                continue;
            };
            let agent = &mut bot.agent;
            let world = &mut self.world;
            let entities = &mut self.mobs.entities;
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
                    _ => Ok(()),
                };
            }
            let food = agent.inventory.get(agent.selected).is_some_and(|s| s.item.food().is_some());
            if tick.place && !food {
                let _ = agent.execute(Command::Place, world, entities, &others);
            }
            agent.hold(tick.input, tick.mine, tick.place && food);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seat() -> Seat {
        Seat {
            name: "Player2".into(),
            uuid: [0; 16],
            id: None,
            held: Held::default(),
            presses: Vec::new(),
            sneaking: false,
            sprinting: false,
            since_jump: u32::MAX,
            leave_since: None,
        }
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
}
