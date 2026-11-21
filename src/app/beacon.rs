//! v0.16 Track C glue: beacons tick with the world clock and their powers
//! land on every player in range; wither roses wither whoever stands in
//! them. The beacon screen's shared transactions are here too (see
//! [`beacon_confirm`]), used by the desktop, controller, LAN and CLI paths.
use glam::DVec3;

use crate::audio::sounds::Sound;
use voxelcraft::rules::GameMode;
use voxelcraft::session::PlayerSession;
use voxelcraft::simulation::effects::{Active, Effect};
use voxelcraft::world::beacon::{self, Cue, Event};

use super::hud::SlotRef;
use super::screen::Screen;
use super::{Container, Game};
use crate::render::ui::Ui;

/// `applyEffects` on one player: `MobEffectInstance(effect, ticks, amp,
/// ambient true, visible true)`.
fn give_power(s: &mut PlayerSession, effect: Effect, amplifier: u8, ticks: u32) {
    s.vitals.give_effect(Active::new(effect, amplifier, ticks).with_flags(true, true, true));
}

/// `WitherRoseBlock.entityInside` for a player: Wither I for 40 ticks while
/// standing in a rose, unless they can't be hurt (Creative, Spectator).
fn rose_contact(s: &mut PlayerSession, world: &crate::world::World) {
    if s.vitals.is_dead() || matches!(s.mode, GameMode::Creative | GameMode::Spectator) {
        return;
    }
    let shape = s.player.collision_shape();
    if crate::physics::touches_block(world, s.player.pos, shape, |b| b == voxelcraft::world::wither_rose::WITHER_ROSE) {
        s.vitals.apply_effect(Effect::Wither, 0, voxelcraft::world::wither_rose::CONTACT_TICKS);
    }
}

impl Game {
    /// Every beacon in a loaded chunk, on the world's game time, then what
    /// they did: powers on the players inside their boxes and their sounds.
    pub(super) fn tick_beacons(&mut self) {
        self.world.tick_beacons(self.world.maps.game_ticks);
        let height = f64::from(self.world.max_y() - self.world.min_y());
        for event in self.world.take_beacon_events() {
            match event {
                Event::Powers { pos, levels, primary, amplifier, secondary, ticks } => {
                    let reaches = |s: &PlayerSession| {
                        let (lo, hi) = s.player.collision_shape().aabb(s.player.pos);
                        !s.vitals.is_dead()
                            && s.mode != GameMode::Spectator
                            && beacon::reaches(pos, levels, height, lo, hi)
                    };
                    let apply = |s: &mut PlayerSession| {
                        if reaches(s) {
                            give_power(s, primary, amplifier, ticks);
                            if let Some(secondary) = secondary {
                                give_power(s, secondary, 0, ticks);
                            }
                        }
                    };
                    apply(&mut self.session);
                    for bot in self.agents.players.values_mut().filter(|b| b.active) {
                        apply(&mut bot.agent.session);
                    }
                }
                Event::Ambient(p) => self.beacon_sound(Cue::Ambient, p),
                Event::Activate(p) => self.beacon_sound(Cue::Activate, p),
                Event::Deactivate(p) => self.beacon_sound(Cue::Deactivate, p),
                Event::PowerSelect(p) => self.beacon_sound(Cue::PowerSelect, p),
            }
        }
        if self.difficulty != voxelcraft::simulation::difficulty::Difficulty::Peaceful {
            rose_contact(&mut self.session, &self.world);
            for bot in self.agents.players.values_mut().filter(|b| b.active) {
                rose_contact(&mut bot.agent.session, &self.world);
            }
        }
    }

    /// v0.16 Track H: potent sulfur's block entities on the world's game
    /// time, then their gas, lifts and sounds.
    pub(super) fn tick_potent_sulfur(&mut self) {
        use voxelcraft::world::potent_sulfur::Event as Geyser;
        self.world.tick_potent_sulfur(self.world.maps.game_ticks);
        for event in self.world.take_potent_sulfur_events() {
            if let Geyser::Sound { cue, pos } = event {
                self.audio.play(Sound::Geyser(cue), Some(pos), 1.0, (1.0, 1.0));
                continue;
            }
            self.session.potent_sulfur(&self.world, &event);
            for bot in self.agents.players.values_mut().filter(|b| b.active) {
                bot.agent.session.potent_sulfur(&self.world, &event);
            }
            self.mobs.entities.potent_sulfur(&self.world, &event);
        }
    }

    fn beacon_sound(&mut self, cue: Cue, p: glam::IVec3) {
        self.audio.play(Sound::Beacon(cue), Some(p.as_dvec3() + DVec3::splat(0.5)), 1.0, (1.0, 1.0));
    }
}

/// The powers a beacon screen offers, in Java's button order: the four
/// level 1–2 powers and Strength as primaries, then the secondary's
/// Regeneration (the other secondary button upgrades the chosen primary).
pub(super) const PRIMARIES: [Effect; 5] =
    [Effect::Speed, Effect::Haste, Effect::Resistance, Effect::JumpBoost, Effect::Strength];

/// The screen's choice in `session.work_choice`: the primary's index in
/// [`PRIMARIES`] plus one in the low byte, the secondary in the high byte
/// (1 Regeneration, 2 the primary's level II).
pub(super) fn choice(session: &PlayerSession) -> (Option<Effect>, Option<Effect>) {
    let v = session.work_choice.unwrap_or(0);
    let primary = (v & 0xff).checked_sub(1).and_then(|i| PRIMARIES.get(i as usize)).copied();
    let secondary = match v >> 8 {
        1 => Some(Effect::Regeneration),
        2 => primary,
        _ => None,
    };
    (primary, secondary)
}

pub(super) fn set_choice(session: &mut PlayerSession, primary: Option<Effect>, secondary: Option<Effect>) {
    let p = primary.and_then(|e| PRIMARIES.iter().position(|&q| q == e)).map_or(0, |i| i as u16 + 1);
    let s = match secondary {
        Some(Effect::Regeneration) => 1,
        Some(e) if Some(e) == primary => 2,
        _ => 0,
    };
    session.work_choice = Some(p | s << 8);
}

/// `BeaconScreen`'s Done button through `BeaconMenu.updateEffects`: with a
/// payment in the slot and powers the pyramid allows, the beacon takes them
/// and one payment item. Returns whether it did.
pub(super) fn beacon_confirm(session: &mut PlayerSession, world: &mut crate::world::World, pos: glam::IVec3) -> bool {
    let Some(payment) = session.work[0].filter(|s| beacon::is_payment(s.item)) else { return false };
    let (primary, secondary) = choice(session);
    if !world.set_beacon_powers(pos, primary, secondary) {
        return false;
    }
    let mut left = payment;
    left.count -= 1;
    session.work[0] = (left.count > 0).then_some(left);
    true
}

// ---- The beacon screen (`BeaconMenu`, `BeaconScreen`) ------------------------

/// What power button `i` stands for: its effect, the pyramid level it
/// needs and whether it is a primary. Button 6 (`BeaconUpgradePowerButton`)
/// is the chosen primary's level II and shows only with a primary.
pub(super) fn button(i: u8, primary: Option<Effect>) -> Option<(Effect, u8, bool)> {
    match i {
        0..=4 => {
            let e = PRIMARIES[i as usize];
            Some((e, beacon::required_levels(e).unwrap_or(4), true))
        }
        5 => Some((Effect::Regeneration, 4, false)),
        6 => primary.map(|p| (p, 4, false)),
        _ => None,
    }
}

/// `BeaconPowerButton.updateStatus`: usable when the pyramid has its tier.
pub(super) fn button_active(i: u8, levels: u8, primary: Option<Effect>) -> bool {
    button(i, primary).is_some_and(|(_, tier, _)| levels >= tier)
}

/// Whether button `i` shows as selected.
pub(super) fn button_selected(i: u8, session: &PlayerSession) -> bool {
    let (primary, secondary) = choice(session);
    match button(i, primary) {
        Some((e, _, true)) => primary == Some(e),
        Some((e, _, false)) => secondary == Some(e),
        None => false,
    }
}

/// `BeaconPowerButton.onPress`: a primary replaces the choice and keeps the
/// secondary only when it is that same power; a secondary is set as is.
pub(super) fn press(session: &mut PlayerSession, i: u8, levels: u8) -> bool {
    let (primary, secondary) = choice(session);
    let Some((e, _, is_primary)) = button(i, primary).filter(|_| button_active(i, levels, primary)) else {
        return false;
    };
    if button_selected(i, session) {
        return false;
    }
    if is_primary {
        set_choice(session, Some(e), secondary.filter(|&s| s == e));
    } else {
        set_choice(session, primary, Some(e));
    }
    true
}

/// `BeaconConfirmButton.updateStatus`: a payment and a primary.
pub(super) fn confirm_active(session: &PlayerSession) -> bool {
    session.work[0].is_some() && choice(session).0.is_some()
}

/// The menu opens on the beacon's current powers (`dataChanged`).
pub(super) fn init_choice(session: &mut PlayerSession, world: &crate::world::World, pos: glam::IVec3) {
    let (primary, secondary) = world.beacon(pos).map_or((None, None), |b| (b.primary, b.secondary));
    set_choice(session, primary, secondary);
}

/// The payment slot, from the panel's corner.
pub(super) const PAYMENT: (f32, f32) = (71.0, 46.0);

/// The buttons' rectangles from the panel's corner: the primaries in their
/// three tiers, the two secondaries, then Done and Cancel.
fn rect(slot: SlotRef) -> Option<(f32, f32, f32, f32)> {
    Some(match slot {
        SlotRef::BeaconPower(i) => match i {
            0 => (8.0, 14.0, 18.0, 18.0),
            1 => (27.0, 14.0, 18.0, 18.0),
            2 => (8.0, 33.0, 18.0, 18.0),
            3 => (27.0, 33.0, 18.0, 18.0),
            4 => (17.5, 51.0, 18.0, 18.0),
            5 => (70.0, 14.0, 18.0, 18.0),
            6 => (89.0, 14.0, 18.0, 18.0),
            _ => return None,
        },
        SlotRef::BeaconDone => (100.0, 46.0, 34.0, 18.0),
        SlotRef::BeaconCancel => (137.0, 46.0, 34.0, 18.0),
        _ => return None,
    })
}

const CONTROLS: [SlotRef; 9] = [
    SlotRef::BeaconPower(0),
    SlotRef::BeaconPower(1),
    SlotRef::BeaconPower(2),
    SlotRef::BeaconPower(3),
    SlotRef::BeaconPower(4),
    SlotRef::BeaconPower(5),
    SlotRef::BeaconPower(6),
    SlotRef::BeaconDone,
    SlotRef::BeaconCancel,
];

/// The control under (`x`, `y`) from the panel's corner.
pub(super) fn control_at(x: f32, y: f32) -> Option<SlotRef> {
    CONTROLS.into_iter().find(|&c| rect(c).is_some_and(|(cx, cy, w, h)| x >= cx && x < cx + w && y >= cy && y < cy + h))
}

/// The beacon panel: the power buttons with their effect icons (dark when
/// the pyramid is too small, green when chosen), the pyramid's level and
/// the hovered power's name, and Done and Cancel.
pub(super) fn draw(ui: &mut Ui, session: &PlayerSession, levels: u8, px: f32, py: f32, hovered: Option<SlotRef>) {
    let (primary, _) = choice(session);
    for c in CONTROLS {
        let Some((x, y, w, h)) = rect(c) else { continue };
        let (x, y) = (px + x, py + y);
        match c {
            SlotRef::BeaconPower(i) => {
                let Some((effect, _, _)) = button(i, primary) else { continue };
                let active = button_active(i, levels, primary);
                let fill = if !active {
                    [0.3, 0.3, 0.3, 1.0]
                } else if button_selected(i, session) {
                    [0.35, 0.6, 0.35, 1.0]
                } else if hovered == Some(c) {
                    [0.85, 0.85, 0.85, 1.0]
                } else {
                    [0.62, 0.62, 0.62, 1.0]
                };
                ui.rect(x, y, w, h, fill);
                let alpha = if active { 1.0 } else { 0.4 };
                ui.icon(x + 1.0, y + 1.0, 16.0, crate::item::effect_icon_layer(effect), [1.0, 1.0, 1.0, alpha]);
                if i == 6 {
                    ui.text_flat(x + 9.0, y + 10.0, "II", [1.0, 1.0, 1.0, 1.0]);
                }
            }
            _ => {
                let active = c == SlotRef::BeaconCancel || confirm_active(session);
                let fill = if !active {
                    [0.3, 0.3, 0.3, 1.0]
                } else if hovered == Some(c) {
                    [0.85, 0.85, 0.85, 1.0]
                } else {
                    [0.62, 0.62, 0.62, 1.0]
                };
                ui.rect(x, y, w, h, fill);
                let label = if c == SlotRef::BeaconDone { "Done" } else { "Cancel" };
                let colour = if active { [0.1, 0.1, 0.1, 1.0] } else { [0.5, 0.5, 0.5, 1.0] };
                ui.text_flat(x + ((w - Ui::text_width(label)) / 2.0).floor(), y + 5.0, label, colour);
            }
        }
    }
    let text = [0.25, 0.25, 0.25, 1.0];
    ui.text_flat(px + 49.0, py + 18.0, "I", text);
    ui.text_flat(px + 49.0, py + 37.0, "II", text);
    ui.text_flat(px + 39.0, py + 55.0, "III", text);
    ui.text_flat(px + 110.0, py + 15.0, &format!("Pyramid {levels}"), text);
    let name = match hovered {
        Some(SlotRef::BeaconPower(i)) => {
            button(i, primary).map(|(e, _, _)| if i == 6 { format!("{} II", e.name()) } else { e.name().to_string() })
        }
        _ => None,
    };
    if let Some(name) = name {
        ui.text_flat(px + 110.0, py + 27.0, &name, text);
    }
}

impl Game {
    /// Opens a beacon's screen on its current powers.
    pub(super) fn open_beacon(&mut self, pos: glam::IVec3) {
        self.open_workstation(Container::Beacon(pos));
        init_choice(&mut self.session, &self.world, pos);
    }
}

impl Screen<'_> {
    /// A click on the beacon screen: the payment slot takes one payment
    /// item at a time (`PaymentSlot.getMaxStackSize`), the power buttons
    /// choose, Done sends the choice (`ServerboundSetBeaconPacket`) and both
    /// Done and Cancel close the screen.
    pub(super) fn beacon_click(&mut self, pos: glam::IVec3, slot: SlotRef, right: bool) -> bool {
        let levels = self.world.beacon(pos).map_or(0, |b| b.levels);
        match slot {
            SlotRef::BeaconPayment => {
                let (held, cursor) = payment_click(self.session.work[0], self.session.inventory.cursor, right);
                self.session.work[0] = held;
                self.session.inventory.cursor = cursor;
            }
            SlotRef::BeaconPower(i) => {
                if press(self.session, i, levels) {
                    self.audio.ui_click();
                }
            }
            SlotRef::BeaconDone => {
                if confirm_active(self.session) {
                    beacon_confirm(self.session, self.world, pos);
                    self.requests.push(super::screen::Request::Close);
                }
            }
            SlotRef::BeaconCancel => self.requests.push(super::screen::Request::Close),
            _ => return false,
        }
        true
    }
}

/// A click on the payment slot (`PaymentSlot`, one item at most): the
/// held slot and cursor after it.
pub(super) fn payment_click(
    held: Option<voxelcraft::inventory::Stack>,
    cursor: Option<voxelcraft::inventory::Stack>,
    right: bool,
) -> (Option<voxelcraft::inventory::Stack>, Option<voxelcraft::inventory::Stack>) {
    match (held, cursor) {
        // Put one payment item in.
        (None, Some(c)) if beacon::is_payment(c.item) => {
            let mut one = c;
            one.count = 1;
            let mut rest = c;
            rest.count -= 1;
            (Some(one), (rest.count > 0).then_some(rest))
        }
        // Take it out.
        (Some(h), None) => (None, Some(h)),
        // Swap for another lone payment item.
        (Some(h), Some(c)) if c.count == 1 && beacon::is_payment(c.item) && !h.stacks_with(&c) => (Some(c), Some(h)),
        // Add it back onto a matching cursor stack.
        (Some(h), Some(mut c)) if h.stacks_with(&c) && c.count < c.max() => {
            let _ = right;
            c.count += 1;
            (None, Some(c))
        }
        other => other,
    }
}

/// The controller's beacon tab: the payment slot, the seven power buttons
/// and Done in its first row (B cancels, as on every tab).
pub(super) const PAD_ROW: [SlotRef; 9] = [
    SlotRef::BeaconPayment,
    SlotRef::BeaconPower(0),
    SlotRef::BeaconPower(1),
    SlotRef::BeaconPower(2),
    SlotRef::BeaconPower(3),
    SlotRef::BeaconPower(4),
    SlotRef::BeaconPower(5),
    SlotRef::BeaconPower(6),
    SlotRef::BeaconDone,
];

/// Draws a beacon control in a controller slot cell at (`x`, `y`).
pub(super) fn draw_pad_control(ui: &mut Ui, session: &PlayerSession, levels: u8, slot: SlotRef, x: f32, y: f32) {
    let (primary, _) = choice(session);
    match slot {
        SlotRef::BeaconPower(i) => {
            let Some((effect, _, _)) = button(i, primary) else { return };
            let active = button_active(i, levels, primary);
            if button_selected(i, session) {
                ui.rect(x, y, 17.0, 17.0, [0.3, 0.7, 0.3, 0.7]);
            }
            let alpha = if active { 1.0 } else { 0.35 };
            ui.icon(x + 0.5, y + 0.5, 16.0, crate::item::effect_icon_layer(effect), [1.0, 1.0, 1.0, alpha]);
            if i == 6 {
                ui.text(x + 8.0, y + 9.0, "II", [1.0, 1.0, 1.0, 1.0]);
            }
        }
        SlotRef::BeaconDone => {
            let colour = if confirm_active(session) { [0.5, 1.0, 0.5, 1.0] } else { [0.5, 0.5, 0.5, 1.0] };
            ui.text(x + 2.0, y + 5.0, "OK", colour);
        }
        _ => {}
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use voxelcraft::entity::PlayerId;

    #[test]
    fn choices_round_trip_through_the_session() {
        let mut s = PlayerSession::new(PlayerId::HOST, DVec3::ZERO);
        assert_eq!(choice(&s), (None, None));
        set_choice(&mut s, Some(Effect::Haste), Some(Effect::Regeneration));
        assert_eq!(choice(&s), (Some(Effect::Haste), Some(Effect::Regeneration)));
        set_choice(&mut s, Some(Effect::Strength), Some(Effect::Strength));
        assert_eq!(choice(&s), (Some(Effect::Strength), Some(Effect::Strength)));
        set_choice(&mut s, Some(Effect::Speed), None);
        assert_eq!(choice(&s), (Some(Effect::Speed), None));
    }
}
