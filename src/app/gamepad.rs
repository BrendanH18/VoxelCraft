//! Local controller players for split-screen. Pressing Start on a gamepad
//! joins it as a player with its own view. Each controller drives an engine
//! `Agent` (the same session hosted agents use) through a profile named
//! Player2..Player8, so inventory and position are kept between sessions.
//!
//! Bedrock-style layout: left stick moves, right stick looks, A jumps (double
//! tap to fly in creative), RT mines and attacks, LT uses (place, eat, draw a
//! bow, buckets, doors, beds and containers), LB/RB cycle the hotbar, B drops
//! one item (hold for the stack), left stick click sprints, right stick click
//! toggles sneaking. X cycles camera perspective. Start opens a pause menu and Y the inventory (see
//! `pad_menu`); the world keeps running while a player is in a menu.
//!
//! Hands run on the controller player's own session, hand animation and
//! [`Body`] through `hands::Actor`, the same code as the mouse, so mining,
//! attacking and every right-click action behave identically. Screen clicks
//! run `screen::Screen` the same way; beds and respawn points use the named
//! player's own session. No player's state is swapped into the host's.

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use gilrs::{Axis, Button, EventType, GamepadId, Gilrs};
use glam::{DVec3, IVec3, Vec2};

use super::actions::Actions;
use super::pad_menu::{Action, Menu, Nav, Tab};
use super::{Container, Game};
use crate::inventory::Stack;
use crate::player::MoveInput;
use crate::world::block::Block;
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
/// Holding B this long drops the rest of the stack.
const DROP_STACK_HOLD: Duration = Duration::from_millis(500);
/// How long a controller player's own messages stay up.
const MESSAGE_TIME: Duration = Duration::from_secs(2);

/// One-shot actions pressed since the last gameplay tick.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Press {
    Jump,
    MineDown,
    MineUp,
    UseDown,
    UseUp,
    Next,
    Previous,
    DropOne,
    DropStack,
    Sprint,
    Sneak,
    Camera,
}

/// Held controls, sampled every frame.
#[derive(Default, Clone, Copy)]
struct Held {
    walk: Vec2,
    look: Vec2,
    jump: bool,
}

/// One player's hand and screen state: the host's own, each controller
/// seat's and each LAN peer's.
pub(super) struct Body {
    pub(super) actions: Actions,
    pub(super) action_cooldown: f64,
    pub(super) left_held: bool,
    pub(super) right_held: bool,
    /// Preserve mining clicks that begin and end between fixed game ticks.
    pub(super) mine_pressed: bool,
    /// The current click started as an attack, so it does not mine.
    pub(super) attack_held: bool,
    pub(super) container: Container,
}

impl Default for Body {
    fn default() -> Self {
        Self {
            actions: Actions::default(),
            action_cooldown: 0.0,
            left_held: false,
            right_held: false,
            mine_pressed: false,
            attack_held: false,
            container: Container::Inventory,
        }
    }
}

impl Body {
    pub(super) fn container(&self) -> Container {
        self.container
    }
    /// The block this player is breaking and how far along (0..1).
    pub(super) fn breaking(&self) -> Option<(glam::IVec3, f32)> {
        self.actions.breaking
    }
    /// Eating progress 0..1.
    pub(super) fn eating(&self) -> f32 {
        (self.actions.eat_timer / super::EAT_TIME) as f32
    }
    /// Lets go of both triggers (opening a screen, sleeping, dying).
    pub(super) fn let_go(&mut self) {
        self.left_held = false;
        self.right_held = false;
        self.mine_pressed = false;
        self.attack_held = false;
        self.actions.reset();
    }
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
    /// Last trigger states (RT, LT), for press and release edges.
    triggers: (bool, bool),
    /// When B went down, until the stack drops or it's released.
    drop_since: Option<Instant>,
    sneaking: bool,
    sprinting: bool,
    /// Ticks since the last jump press.
    since_jump: u32,
    /// Open screen; gameplay input is ignored while it's up.
    menu: Option<Menu>,
    /// Left-stick menu direction and when it next repeats.
    nav: Option<(Nav, Instant)>,
    body: Body,
    /// A message for this player's view ("Respawn point set").
    message: Option<(String, Instant)>,
}

/// What a seat asks of its agent for one tick.
struct TickInput {
    input: MoveInput,
    actions: Vec<Press>,
    /// A was pressed (respawns when dead, leaves a bed).
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
            triggers: (false, false),
            drop_since: None,
            sneaking: false,
            sprinting: false,
            since_jump: u32::MAX,
            menu: None,
            nav: None,
            body: Body::default(),
            message: None,
        }
    }

    /// Opens a screen, dropping held gameplay input.
    fn open(&mut self, menu: Menu) {
        self.menu = Some(menu);
        self.held = Held::default();
        self.presses.clear();
        self.nav = None;
        self.drop_since = None;
        self.body.let_go();
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

    /// Trigger presses and releases since the last frame. Edges come from
    /// the triggers' own last state, so closing a screen with a trigger
    /// still down doesn't act again.
    fn trigger_edges(&mut self, rt: bool, lt: bool, playing: bool) {
        let (was_rt, was_lt) = std::mem::replace(&mut self.triggers, (rt, lt));
        if !playing {
            return;
        }
        if rt != was_rt {
            self.presses.push(if rt { Press::MineDown } else { Press::MineUp });
        }
        if lt != was_lt {
            self.presses.push(if lt { Press::UseDown } else { Press::UseUp });
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
    /// Which profile each controller played last, so it gets it back.
    assigned: BTreeMap<[u8; 16], String>,
}

impl Pads {
    pub fn new() -> Self {
        let gilrs = Gilrs::new().map_err(|e| log::warn!("gamepads unavailable: {e}")).ok();
        Self { gilrs, ..Default::default() }
    }

    /// Saved with the world: `uuid=profile` pairs separated by `;`.
    pub fn serialize(&self) -> String {
        let hex = |u: &[u8; 16]| u.iter().map(|b| format!("{b:02x}")).collect::<String>();
        let assigned: Vec<String> = self.assigned.iter().map(|(u, n)| format!("{}={n}", hex(u))).collect();
        assigned.join(";")
    }

    /// Reads what [`Pads::serialize`] wrote, skipping anything malformed.
    pub fn restore(&mut self, assigned: &str) {
        for (uuid, name) in assigned.split(';').filter_map(|p| p.split_once('=')) {
            let bytes: Option<Vec<u8>> = (0..uuid.len())
                .step_by(2)
                .map(|i| uuid.get(i..i + 2).and_then(|h| u8::from_str_radix(h, 16).ok()))
                .collect();
            if let Some(Ok(uuid)) = bytes.map(<[u8; 16]>::try_from)
                && voxelcraft::control::valid_name(name)
            {
                self.assigned.insert(uuid, name.into());
            }
        }
    }

    /// Whether a local controller plays as this profile.
    pub fn seated(&self, name: &str) -> bool {
        self.seats.iter().any(|s| s.name == name)
    }
}

/// What a controller player's view shows of their hands and bed.
#[derive(Default)]
pub(super) struct PadView<'a> {
    pub breaking: Option<(IVec3, f32)>,
    /// Bite progress, 0..1.
    pub eating: f32,
    pub bow: Option<f32>,
    pub message: Option<&'a str>,
}

/// Start and the face button that jumps skip the end credits, like Escape and Space.
fn skips_credits(button: Button) -> bool {
    matches!(button, Button::Start | Button::South | Button::East)
}

fn press_for(button: Button) -> Option<Press> {
    Some(match button {
        Button::South => Press::Jump,
        Button::RightTrigger => Press::Next,
        Button::LeftTrigger => Press::Previous,
        Button::East => Press::DropOne,
        Button::LeftThumb => Press::Sprint,
        Button::RightThumb => Press::Sneak,
        Button::West => Press::Camera,
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

impl Pads {
    pub(super) fn trading(&self, id: u64) -> bool {
        self.seats.iter().any(|s| matches!(s.menu,Some(Menu::Items {tab:Tab::Trading(other),..}) if id==other))
    }
}
impl Game {
    pub(super) fn open_pad_trading(&mut self, id: u64) -> bool {
        if let Some(seat) = self.pads.seats.first_mut() {
            seat.open(Menu::items(Tab::Trading(id)));
            true
        } else {
            false
        }
    }
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
        let mut skip_credits = false;
        while let Some(event) = gilrs.next_event() {
            if self.credits.is_some() {
                // The credits swallow every press, like the keyboard's Escape/Space/Enter.
                skip_credits |= matches!(event.event, EventType::ButtonPressed(button, _) if skips_credits(button));
                if matches!(event.event, EventType::ButtonPressed(..) | EventType::ButtonReleased(..)) {
                    continue;
                }
            }
            let seat = self.pads.seats.iter_mut().find(|s| s.id == Some(event.id));
            match (event.event, seat) {
                (EventType::ButtonPressed(Button::Start, _), None) => joins.push(event.id),
                (EventType::ButtonPressed(button, _), Some(seat)) => match button {
                    _ if seat.menu.is_some() => navs.extend(nav_for(button).map(|n| (seat.name.clone(), n))),
                    Button::Start => seat.open(Menu::Pause { choice: 0 }),
                    Button::North => seat.open(Menu::items(Tab::Inventory)),
                    Button::East => {
                        seat.drop_since = Some(Instant::now());
                        seat.presses.push(Press::DropOne);
                    }
                    _ => seat.presses.extend(press_for(button)),
                },
                (EventType::ButtonReleased(Button::East, _), Some(seat)) => seat.drop_since = None,
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
                    seat.triggers = (false, false);
                    seat.drop_since = None;
                    seat.body.let_go();
                    if let Some(bot) = self.agents.players.get_mut(&seat.name) {
                        bot.agent.session.cancel_consumption();
                        bot.agent.session.cancel_mining();
                    }
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
            let trigger = |b: Button| pad.button_data(b).is_some_and(|d| d.value() > TRIGGER);
            seat.trigger_edges(trigger(Button::RightTrigger2), trigger(Button::LeftTrigger2), seat.menu.is_none());
            if seat.menu.is_some() {
                let stick = Vec2::new(pad.value(Axis::LeftStickX), pad.value(Axis::LeftStickY));
                navs.extend(seat.stick_nav(stick, now).map(|n| (seat.name.clone(), n)));
                continue;
            }
            if seat.drop_since.is_some_and(|t| now - t >= DROP_STACK_HOLD) {
                seat.drop_since = None;
                seat.presses.push(Press::DropStack);
            }
            seat.held = Held {
                walk: deadzone(Vec2::new(pad.value(Axis::LeftStickX), pad.value(Axis::LeftStickY))),
                look: Vec2::new(pad.value(Axis::RightStickX), pad.value(Axis::RightStickY)),
                jump: pad.is_pressed(Button::South),
            };
        }
        if !paused {
            for seat in &self.pads.seats {
                if let Some(bot) = self.agents.players.get_mut(&seat.name)
                    && seat.menu.is_none()
                    && !bot.agent.session.vitals.is_dead()
                {
                    let d = look_delta(seat.held.look, dt, self.settings.sensitivity);
                    bot.agent.session.player.look(d.x, -d.y);
                }
            }
        }
        if skip_credits {
            self.skip_credits();
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
        let uuid = self.pads.gilrs.as_ref().zip(id).map_or([0; 16], |(g, id)| g.gamepad(id).uuid());
        let free = |n: &String| !self.pads.seated(n) && !self.agents.players.get(n).is_some_and(|b| b.active);
        // A controller gets the profile it played last, if that's free.
        let name = self
            .pads
            .assigned
            .get(&uuid)
            .filter(|n| uuid != [0; 16] && free(n))
            .cloned()
            .or_else(|| (2..=8).map(|n| format!("Player{n}")).find(free))
            .ok_or("No free player profile")?;
        if !self.agents.players.contains_key(&name) {
            let mut agent = Agent::new(self.beside_host());
            agent.session.player.yaw = self.session.player.yaw;
            agent.set_mode(self.session.mode);
            self.agents.insert(name.clone(), agent);
        }
        self.agents.players.get_mut(&name).unwrap().active = true;
        if !self.split.follow.contains(&name) {
            self.split.follow.push(name.clone());
        }
        self.pads.seats.push(Seat::new(name.clone(), uuid, id));
        if uuid != [0; 16] {
            self.pads.assigned.insert(uuid, name.clone());
        }
        Ok(name)
    }

    /// A free standing spot next to the host, or the host's own (players
    /// don't collide, but a shared spot puts the camera inside the host).
    pub(super) fn beside_host(&self) -> DVec3 {
        let host = self.session.player.pos;
        let open = |p: IVec3| self.world.get_block(p).is_some_and(|b| !b.is_solid());
        [(1, 0), (-1, 0), (0, 1), (0, -1), (1, 1), (-1, -1), (1, -1), (-1, 1)]
            .into_iter()
            .map(|(x, z)| host + DVec3::new(x as f64, 0.0, z as f64))
            .find(|p| {
                let feet = p.floor().as_ivec3();
                open(feet) && open(feet + IVec3::Y) && !open(feet - IVec3::Y)
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
            bot.agent.session.cancel_consumption();
            bot.agent.session.cancel_mining();
            bot.agent.session.sleeping = None;
            bot.agent.hold(MoveInput::default(), false, false);
        }
    }

    /// `--pad-player`: seat an unplugged controller player holding a copy
    /// of the host's inventory, with a screen open.
    pub(super) fn virtual_pad(&mut self, screen: &str) {
        let Ok(name) = self.join_pad(None) else { return };
        let inventory = self.session.inventory.clone();
        if let Some(bot) = self.agents.players.get_mut(&name) {
            bot.agent.session.inventory = inventory;
        }
        let seat = self.pads.seats.last_mut().unwrap();
        match screen {
            "pause" => seat.open(Menu::Pause { choice: 0 }),
            "inventory" => seat.open(Menu::items(Tab::Inventory)),
            "crafting" => seat.open(Menu::items(Tab::Crafting)),
            "palette" => seat.open(Menu::items(Tab::Palette)),
            _ => {}
        }
    }

    /// One menu press. Returns whether the player left the game.
    fn pad_nav(&mut self, name: &str, nav: Nav) -> bool {
        let Some(i) = self.pads.seats.iter().position(|s| s.name == name) else { return false };
        let Some(mut menu) = self.pads.seats[i].menu else { return false };
        let tab = match menu {
            Menu::Items { tab, .. } => tab,
            Menu::Pause { .. } => Tab::Inventory,
        };
        let action = menu.press(nav, self.pad_lists(name, tab));
        if action == Action::Leave {
            self.leave_pad(name);
            return true;
        }
        if self.apply_menu(i, name, menu, action) {
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

    /// Hands, bed and messages of a controller player, for their view.
    pub(super) fn pad_view(&self, name: &str) -> Option<PadView<'_>> {
        let seat = self.pads.seats.iter().find(|s| s.name == name)?;
        let a = &seat.body.actions;
        let session = &self.agents.players.get(name)?.agent.session;
        Some(PadView {
            breaking: a.breaking,
            eating: (a.eat_timer / super::EAT_TIME) as f32,
            bow: session.draw_power(a.selected),
            message: seat.message.as_ref().filter(|(_, t)| t.elapsed() < MESSAGE_TIME).map(|(m, _)| m.as_str()),
        })
    }

    /// Controller player `name`'s enchanting, anvil or smithing input slots.
    pub(super) fn pad_work(&self, name: &str) -> super::enchanting::WorkSlots {
        self.agents.players.get(name).map_or([None; 3], |b| b.agent.session.work)
    }

    /// Empties controller player `name`'s workstation input slots.
    pub(super) fn take_pad_work(&mut self, name: &str) -> Vec<crate::inventory::Stack> {
        let Some(bot) = self.agents.players.get_mut(name) else { return Vec::new() };
        bot.agent.session.work.iter_mut().filter_map(Option::take).collect()
    }

    /// Feed one gameplay tick of controller input to each seated player:
    /// movement to their agent, and button presses and held triggers to
    /// their own hands (`hands::Actor`). Runs before the agents tick.
    pub(super) fn drive_pads(&mut self, dt: f64) {
        for i in 0..self.pads.seats.len() {
            let tick = self.pads.seats[i].tick_input();
            let name = self.pads.seats[i].name.clone();
            let Some(bot) = self.agents.players.get_mut(&name) else { continue };
            let agent = &mut bot.agent;
            agent.hold(MoveInput::default(), false, false);
            if let Some(menu) = self.pads.seats[i].menu {
                agent.session.cancel_consumption();
                agent.session.cancel_mining();
                // Screens close on death, and containers when they're gone or out of reach.
                let eye = agent.session.player.eye();
                let gone = match menu {
                    Menu::Items {
                        tab:
                            tab @ (Tab::Chest(pos)
                            | Tab::Furnace(pos)
                            | Tab::Brewing(pos)
                            | Tab::Enchanting(pos)
                            | Tab::Anvil(pos)
                            | Tab::Grindstone(pos)
                            | Tab::Smithing(pos)),
                        ..
                    } => {
                        !self.world.get_block(pos).is_some_and(|b| tab.matches_block(b))
                            || !agent.session.can_keep_using_block(pos)
                            || agent.session.vitals.is_dead()
                    }
                    Menu::Items { tab: Tab::Vehicle(id), .. } => {
                        agent.session.vitals.is_dead()
                            || !self.mobs.entities.vehicle_in_reach(id, eye, agent.session.entity_screen_range())
                    }
                    Menu::Items { tab: Tab::Trading(id), .. } => {
                        agent.session.vitals.is_dead()
                            || !self.mobs.entities.merchant_in_reach(id, eye, agent.session.entity_screen_range())
                    }
                    Menu::Items { .. } => agent.session.vitals.is_dead(),
                    Menu::Pause { .. } => false,
                };
                if gone {
                    self.pads.seats[i].menu = None;
                    self.close_pad_menu(&name);
                }
                continue;
            }
            if agent.session.vitals.is_dead() {
                self.pads.seats[i].body.let_go();
                if tick.jumped {
                    self.respawn_pad(i);
                }
                continue;
            }
            if agent.session.sleeping.is_some() {
                // Jumping gets up; being hurt wakes you (Agent::hurt).
                if tick.jumped {
                    agent.session.sleeping = None;
                }
                continue;
            }
            if tick.toggle_flight && agent.session.player.can_fly {
                let _ = agent.execute(
                    Command::Fly(!agent.session.player.flying),
                    &mut self.world,
                    &mut self.mobs.entities,
                    &[],
                );
            }
            let mut presses = Vec::new();
            for action in tick.actions {
                match action {
                    Press::Camera => bot.camera.cycle(),
                    Press::Next => agent.selected = (agent.selected + 1) % 9,
                    Press::Previous => agent.selected = (agent.selected + 8) % 9,
                    other => presses.push(other),
                }
            }
            agent.hold(tick.input, false, false);
            let mut body = std::mem::take(&mut self.pads.seats[i].body);
            let fighter = super::lan::PlayerRef::Agent(name.clone());
            let mut requests = Vec::new();
            for press in presses {
                let done = match press {
                    Press::MineDown => {
                        let (slot, sprint, interact) = self.agent_attack_context(&name);
                        let hit = interact && self.attack_lan_player(&fighter, slot, sprint);
                        self.agent_hands(&name, &mut body, |h| h.attack_button(true, hit))
                    }
                    Press::MineUp => self.agent_hands(&name, &mut body, |h| h.attack_button(false, false)),
                    Press::UseDown => self.agent_hands(&name, &mut body, |h| h.use_button(true)),
                    Press::UseUp => self.agent_hands(&name, &mut body, |h| h.use_button(false)),
                    Press::DropOne | Press::DropStack => {
                        self.drop_agent_selected(&name, &mut body, press == Press::DropStack);
                        None
                    }
                    _ => None,
                };
                requests.extend(done.map(|(_, r)| r).unwrap_or_default());
            }
            let aiming = self.agent_aiming_at_target(&name, &body);
            if let Some((_, r)) = self.agent_hands(&name, &mut body, |h| h.act(true, dt, aiming)) {
                requests.extend(r);
            }
            self.pads.seats[i].body = body;
            for request in requests {
                self.pad_request(i, request);
            }
        }
    }

    /// Opens what a controller player's hands requested, or shows their message.
    fn pad_request(&mut self, i: usize, request: super::hands::Request) {
        use super::hands::Request;
        use voxelcraft::session::interact::Used;
        let name = self.pads.seats[i].name.clone();
        match request {
            Request::Popup(text) => self.pads.seats[i].message = Some((text, Instant::now())),
            Request::Open(Used::OpenVehicle(id)) => self.pads.seats[i].open(Menu::items(Tab::Vehicle(id))),
            Request::Open(Used::OpenMerchant(id)) => self.pads.seats[i].open(Menu::items(Tab::Trading(id))),
            Request::Open(Used::Screen { cell, .. } | Used::Bed { cell }) => self.pad_use_block(i, cell),
            Request::Open(_) => {}
            Request::MagicDamage(amount) => {
                if let Some(bot) = self.agents.players.get_mut(&name) {
                    bot.agent.hurt_magic(amount, &mut self.mobs.entities);
                }
            }
        }
    }

    /// Runs `f` on agent `name`'s own hands (session, hand animation and
    /// `body`), returning its result and what it requested.
    pub(super) fn agent_hands<R>(
        &mut self,
        name: &str,
        body: &mut Body,
        f: impl FnOnce(&mut super::hands::Actor) -> R,
    ) -> Option<(R, Vec<super::hands::Request>)> {
        let bot = self.agents.players.get(name)?;
        let session = &bot.agent.session;
        let sneaking = session.player.sneaking;
        let sprinting = bot.agent.movement_input().sprint && session.vitals.may_sprint(session.mode.invulnerable());
        let id = bot.id;
        let others: Vec<glam::DVec3> = std::iter::once(self.session.player.pos)
            .chain(self.agents.players.values().filter(|b| b.active && b.id != id).map(|b| b.agent.session.player.pos))
            .collect();
        body.actions.select(bot.agent.selected);
        let bot = self.agents.players.get_mut(name)?;
        let mut actor = super::hands::Actor {
            session: &mut bot.agent.session,
            body: &mut *body,
            hand: &mut bot.hand,
            world: &mut self.world,
            entities: &mut self.mobs.entities,
            audio: &mut self.audio,
            gamerules: &self.gamerules,
            sneaking,
            sprinting,
            others: &others,
            requests: Vec::new(),
        };
        let result = f(&mut actor);
        let requests = std::mem::take(&mut actor.requests);
        bot.agent.selected = body.actions.selected;
        bot.agent.creative = bot.agent.session.mode.is_creative();
        Some((result, requests))
    }

    /// Runs `f` on agent `name`'s open screen: their session with the given
    /// container and crafting grid. Returns its result and requests.
    pub(super) fn agent_screen<R>(
        &mut self,
        name: &str,
        container: &mut Container,
        craft: &mut crate::crafting::Grid,
        f: impl FnOnce(&mut super::screen::Screen) -> R,
    ) -> Option<(R, Vec<super::screen::Request>)> {
        let bot = self.agents.players.get_mut(name)?;
        let mut screen = super::screen::Screen {
            session: &mut bot.agent.session,
            container,
            craft,
            world: &mut self.world,
            entities: &mut self.mobs.entities,
            audio: &mut self.audio,
            requests: Vec::new(),
        };
        let result = f(&mut screen);
        let requests = std::mem::take(&mut screen.requests);
        Some((result, requests))
    }

    /// A controller player's click (or quick move) on a slot of `container`.
    pub(super) fn pad_click(
        &mut self,
        seat: usize,
        name: &str,
        container: Container,
        slot: super::hud::SlotRef,
        quick: bool,
        right: bool,
    ) {
        // Controller crafting uses recipe slots, so their screens have no grid of their own.
        let mut container = container;
        let mut no_grid = crate::crafting::Grid::new(2);
        let done = self.agent_screen(name, &mut container, &mut no_grid, |s| {
            if quick { s.quick_move(slot) } else { s.click_slot(slot, right) }
        });
        self.pads.seats[seat].body.container = container;
        for super::screen::Request::Popup(text) in done.map(|(_, r)| r).unwrap_or_default() {
            self.pads.seats[seat].message = Some((text, Instant::now()));
        }
    }

    /// Selected slot, sprint state and whether agent `name` may attack now.
    pub(super) fn agent_attack_context(&self, name: &str) -> (usize, bool, bool) {
        let Some(bot) = self.agents.players.get(name) else { return (0, false, false) };
        let s = &bot.agent.session;
        let sprint = bot.agent.movement_input().sprint && s.vitals.may_sprint(s.mode.invulnerable());
        (bot.agent.selected, sprint, s.mode.can_interact())
    }

    /// Whether a mob, fight part or player is under agent `name`'s crosshair,
    /// which suppresses mining (the click-started-as-attack flag is in `body`).
    pub(super) fn agent_aiming_at_target(&self, name: &str, body: &Body) -> bool {
        let Some(bot) = self.agents.players.get(name) else { return false };
        let s = &bot.agent.session;
        body.attack_held
            || s.mob_target(&self.world, &self.mobs.entities).is_some()
            || {
                let (eye, dir) = (s.player.eye(), s.player.forward().as_dvec3());
                let block = s.block_target(&self.world).and_then(|(b, _)| {
                    crate::physics::ray_aabb(eye, dir, b.as_dvec3(), b.as_dvec3() + glam::DVec3::ONE)
                });
                let reach = block.map_or(s.entity_reach(), |d| d.min(s.entity_reach()));
                self.mobs.entities.fight_raycast(eye, dir, reach).is_some()
            }
            || self.lan_player_target(&super::lan::PlayerRef::Agent(name.to_string())).is_some()
    }

    /// Drops one item or the whole selected stack for agent `name`.
    pub(super) fn drop_agent_selected(&mut self, name: &str, body: &mut Body, whole_stack: bool) {
        let Some(bot) = self.agents.players.get_mut(name) else { return };
        let slot = bot.agent.selected;
        let s = &mut bot.agent.session;
        let Some(stack) = s.inventory.slots[slot] else { return };
        let count = if whole_stack { stack.count } else { 1 };
        s.inventory.slots[slot] = (stack.count > count).then_some(Stack { count: stack.count - count, ..stack });
        body.actions.reset();
        s.cancel_mining();
        s.cancel_consumption();
        let (eye, dir) = (s.player.eye(), s.player.forward().as_dvec3());
        self.mobs.entities.throw(Stack { count, ..stack }, eye, dir);
    }

    /// LT on a container or bed: the controller player's own screen opens,
    /// or they lie down.
    fn pad_use_block(&mut self, i: usize, pos: IVec3) {
        let Some(block) = self.world.get_block(pos) else { return };
        let seat = &mut self.pads.seats[i];
        if block == Block::CRAFTING_TABLE {
            seat.open(Menu::items(Tab::Crafting));
        } else if crate::world::chest::is_chest(block) {
            seat.open(Menu::items(Tab::Chest(pos)));
            self.chest_sound(pos, 0.9);
        } else if crate::world::furnace::is_furnace(block) {
            seat.open(Menu::items(Tab::Furnace(pos)));
        } else if block == Block::BREWING_STAND {
            seat.open(Menu::items(Tab::Brewing(pos)));
        } else if block == Block::ENCHANTING_TABLE {
            seat.open(Menu::items(Tab::Enchanting(pos)));
        } else if block.is_anvil() {
            seat.open(Menu::items(Tab::Anvil(pos)));
        } else if block.base() == Block::GRINDSTONE {
            seat.open(Menu::items(Tab::Grindstone(pos)));
        } else if block == Block::SMITHING_TABLE {
            seat.open(Menu::items(Tab::Smithing(pos)));
        } else if block.is_bed() {
            self.pad_sleep(i, pos);
        }
    }

    /// Lies controller player `i` down in the bed at `pos`, if the host's
    /// bed rules allow (night or rain, no monsters, not in the Nether).
    fn pad_sleep(&mut self, i: usize, pos: IVec3) {
        if self.bed_explodes(pos) {
            return;
        }
        let name = self.pads.seats[i].name.clone();
        let (changed, rest) = self.agent_sleep_at(&name, pos);
        let seat = &mut self.pads.seats[i];
        match rest {
            Ok(()) => seat.body.let_go(),
            Err(e) => seat.message = Some((e, Instant::now())),
        }
        if changed && seat.message.is_none() {
            seat.message = Some(("Respawn point set".into(), Instant::now()));
        }
    }

    /// Whether this profile's controller is unplugged: it can't reach a
    /// bed (or leave), so it doesn't hold up the night.
    pub(super) fn unplugged(&self, name: &str) -> bool {
        self.pads.seats.iter().any(|s| s.name == name && s.id.is_none())
    }

    /// A: back to life at their bed (if it's still there) or the world spawn.
    /// Controller players share the host's dimension, so outside the
    /// Overworld they come back beside the host, and their Overworld bed is
    /// left alone rather than looked up in the wrong world.
    fn respawn_pad(&mut self, i: usize) {
        let name = self.pads.seats[i].name.clone();
        let Some(bot) = self.agents.players.get_mut(&name) else { return };
        let _ = bot.agent.execute(Command::Respawn, &mut self.world, &mut self.mobs.entities, &[]);
        let at = if self.dimension == crate::world::terrain::Dimension::Overworld {
            let (at, missing) = self.respawn_point_of(&super::lan::PlayerRef::Agent(name.clone()));
            if missing {
                self.pads.seats[i].message = Some(("Your home bed was missing".into(), Instant::now()));
            }
            at
        } else {
            self.beside_host()
        };
        let seat = &mut self.pads.seats[i];
        seat.sneaking = false;
        seat.body = Body::default();
        if let Some(bot) = self.agents.players.get_mut(&name) {
            bot.agent.session.player.pos = at;
            bot.agent.previous_pos = at;
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
    fn triggers_press_and_release_once_and_not_after_a_screen() {
        let mut s = seat();
        s.trigger_edges(true, false, true);
        s.trigger_edges(true, true, true);
        s.trigger_edges(false, true, true);
        assert_eq!(s.tick_input().actions, vec![Press::MineDown, Press::UseDown, Press::MineUp]);
        // LT released and pressed again behind a screen: nothing on return.
        s.trigger_edges(false, false, false);
        s.trigger_edges(false, true, false);
        s.trigger_edges(false, true, true);
        assert!(s.tick_input().actions.is_empty());
    }

    #[test]
    fn controller_profiles_survive_saving() {
        let mut pads = Pads::default();
        let uuid = [0xab; 16];
        pads.assigned.insert(uuid, "Player3".into());
        let mut restored = Pads::default();
        restored.restore(&pads.serialize());
        assert_eq!(restored.assigned.get(&uuid).map(String::as_str), Some("Player3"));
        // Garbage is skipped rather than failing the load.
        restored.restore("zz=Player2;abcd=Player5;=;");
        assert_eq!(restored.assigned.len(), 1);
    }

    #[test]
    fn start_and_the_face_buttons_skip_the_credits() {
        assert!(skips_credits(Button::Start) && skips_credits(Button::South) && skips_credits(Button::East));
        assert!(!skips_credits(Button::LeftTrigger) && !skips_credits(Button::DPadUp));
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
