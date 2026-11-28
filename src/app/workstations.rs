//! v0.15 Track B screens: the stonecutter (`StonecutterMenu`), loom
//! (`LoomMenu`) and cartography table (`CartographyTableMenu`). Their
//! inputs are the player's `work` slots; the chosen recipe or pattern is
//! `work_choice`. Every input path (desktop, controller seat, LAN peer)
//! goes through the shared [`Screen`] transactions here.

use glam::IVec3;

use crate::audio::sounds::{Material, Sound};
use crate::inventory::{HOTBAR_SLOTS, SLOTS, Stack, move_into};
use crate::item::Item;
use crate::render::ui::Ui;
use crate::world::World;
use crate::world::block::Block;
use voxelcraft::content::banner_patterns as bp;
use voxelcraft::crafting::stonecutting;
use voxelcraft::inventory::components::BannerPattern;
use voxelcraft::session::PlayerSession;

use super::hud::SlotRef;
use super::screen::Screen;
use super::{Container, Game};

/// Visible recipe buttons: 4 × 3 on the stonecutter, 4 × 4 on the loom.
pub(super) const COLS: usize = 4;
pub(super) const CUT_ROWS: usize = 3;
pub(super) const LOOM_ROWS: usize = 4;

pub(super) fn is_workstation(c: Container) -> bool {
    matches!(c, Container::Stonecutter(_) | Container::Loom(_) | Container::Cartography(_) | Container::Beacon(_))
}

/// The work slot a workstation input slot uses.
pub(super) fn work_index(slot: SlotRef) -> Option<usize> {
    Some(match slot {
        SlotRef::CutInput | SlotRef::LoomBanner | SlotRef::MapInput | SlotRef::BeaconPayment => 0,
        SlotRef::LoomDye | SlotRef::MapExtra => 1,
        SlotRef::LoomPattern => 2,
        _ => return None,
    })
}

pub(super) fn is_workstation_slot(slot: SlotRef) -> bool {
    work_index(slot).is_some()
        || matches!(
            slot,
            SlotRef::CutResult
                | SlotRef::CutRecipe(_)
                | SlotRef::LoomResult
                | SlotRef::LoomChoice(_)
                | SlotRef::MapResult
                | SlotRef::BeaconPower(_)
                | SlotRef::BeaconDone
                | SlotRef::BeaconCancel
        )
}

// ---- Shared state -----------------------------------------------------------

/// The stonecutter's recipe list for its input (`getVisibleRecipes`).
pub(super) fn cut_list(session: &PlayerSession) -> &'static [(Item, u8)] {
    session.work[0].map_or(&[], |s| stonecutting::menu(s.item))
}

/// `selectedRecipeIndex`, valid for the current input.
pub(super) fn cut_selected(session: &PlayerSession) -> Option<usize> {
    let input = session.work[0].map(|s| s.item);
    (session.work_input == input)
        .then_some(session.work_choice?)
        .map(usize::from)
        .filter(|&i| i < cut_list(session).len())
}

pub(super) fn cut_result(session: &PlayerSession) -> Option<Stack> {
    let &(item, count) = cut_list(session).get(cut_selected(session)?)?;
    Some(Stack::new(item, count))
}

fn pattern_index(p: BannerPattern) -> u16 {
    BannerPattern::all().position(|q| q == p).unwrap_or(0) as u16
}

fn pattern_at(i: u16) -> Option<BannerPattern> {
    BannerPattern::all().nth(i as usize)
}

/// The loom's pattern list, the selected entry and the result.
pub(super) fn loom_view(session: &PlayerSession) -> (Vec<BannerPattern>, Option<usize>, Option<Stack>) {
    let [banner, dye, pattern] = session.work;
    let (Some(banner), Some(dye)) = (banner, dye) else { return (Vec::new(), None, None) };
    let list = bp::selectable(pattern);
    let selected = if list.len() == 1 {
        Some(0)
    } else {
        session.work_choice.and_then(pattern_at).and_then(|c| list.iter().position(|&p| p == c))
    };
    let full = bp::patterns(&banner).layers.len() >= bp::MAX_LAYERS;
    let result = selected.filter(|_| !full).and_then(|i| bp::loom_result(banner, dye, list[i]));
    (list, selected.filter(|_| !full), result)
}

pub(super) fn cartography_result(session: &PlayerSession, world: &World) -> Option<Stack> {
    world.maps.cartography_result(session.work[0], session.work[1])
}

/// `slotsChanged`: the stonecutter forgets its recipe when the input item
/// changes; the loom keeps its pattern only while it's still offered (and
/// takes the only one when one is offered).
pub(super) fn sync_work_menu(container: Container, session: &mut PlayerSession) {
    match container {
        Container::Stonecutter(_) => {
            let input = session.work[0].map(|s| s.item);
            if session.work_input != input {
                session.work_input = input;
                session.work_choice = None;
            }
        }
        Container::Loom(_) => {
            let (list, selected, _) = loom_view(session);
            session.work_choice = selected.map(|i| pattern_index(list[i]));
            let full = session.work[0].is_some_and(|b| bp::patterns(&b).layers.len() >= bp::MAX_LAYERS);
            if full {
                session.work_choice = None;
            }
        }
        _ => {}
    }
}

/// Where a stack goes when shift-clicked into the open workstation:
/// `quickMoveStack`'s routing.
fn input_for(container: Container, item: Item) -> Option<usize> {
    match container {
        Container::Stonecutter(_) => stonecutting::accepts(item).then_some(0),
        Container::Loom(_) => bp::loom_slot_for(item),
        Container::Cartography(_) => {
            if item.is_map() && item != Item::MAP {
                Some(0)
            } else {
                takes_extra(item).then_some(1)
            }
        }
        _ => None,
    }
}

/// The cartography table's additional slot: paper, an empty map or a
/// glass pane.
fn takes_extra(item: Item) -> bool {
    item == Item::PAPER || item == Item::MAP || item == Item::from_block(Block::GLASS_PANE)
}

/// Whether the input slot `slot` takes `stack` (`Slot.mayPlace`).
pub(super) fn accepts(slot: SlotRef, stack: &Stack) -> bool {
    match slot {
        SlotRef::CutInput => true,
        SlotRef::LoomBanner => bp::banner_color(stack.item).is_some(),
        SlotRef::LoomDye => bp::loom_dye(stack.item).is_some(),
        SlotRef::LoomPattern => bp::is_pattern_item(stack.item),
        SlotRef::MapInput => voxelcraft::world::maps::has_map_id(stack),
        SlotRef::MapExtra => takes_extra(stack.item),
        SlotRef::BeaconPayment => voxelcraft::world::beacon::is_payment(stack.item),
        _ => false,
    }
}

fn to_player_order() -> Vec<usize> {
    (0..HOTBAR_SLOTS).rev().chain((HOTBAR_SLOTS..SLOTS).rev()).collect()
}

impl Screen<'_> {
    /// The result the open workstation would give now.
    fn workstation_result(&self) -> Option<Stack> {
        match *self.container {
            Container::Stonecutter(_) => cut_result(self.session),
            Container::Loom(_) => loom_view(self.session).2,
            Container::Cartography(_) => cartography_result(self.session, self.world),
            _ => None,
        }
    }

    /// `Slot.onTake` of the result: uses up the inputs and plays the
    /// station's sound; the map table also applies its post-processing.
    fn take_workstation_result(&mut self) -> Option<Stack> {
        let mut result = self.workstation_result()?;
        let pos = match *self.container {
            Container::Stonecutter(p) | Container::Loom(p) | Container::Cartography(p) => p,
            _ => return None,
        };
        match *self.container {
            Container::Stonecutter(_) => take_one(&mut self.session.work[0]),
            Container::Loom(_) => {
                let [banner, dye, _] = &mut self.session.work;
                take_one(banner);
                take_one(dye);
                if banner.is_none() || dye.is_none() {
                    self.session.work_choice = None;
                }
            }
            _ => {
                take_one(&mut self.session.work[0]);
                take_one(&mut self.session.work[1]);
                self.world.maps.post_process(&mut result);
            }
        }
        sync_work_menu(*self.container, self.session);
        let at = pos.as_dvec3() + glam::DVec3::splat(0.5);
        let material =
            if matches!(*self.container, Container::Stonecutter(_)) { Material::Stone } else { Material::Wood };
        self.audio.play(Sound::Place(material), Some(at), 0.8, (1.2, 1.4));
        Some(result)
    }

    /// A click on a workstation slot or button; false when `slot` isn't one.
    pub(super) fn workstation_click(&mut self, slot: SlotRef, right: bool) -> bool {
        if !is_workstation_slot(slot) || !is_workstation(*self.container) {
            return false;
        }
        if let Container::Beacon(pos) = *self.container {
            return self.beacon_click(pos, slot, right);
        }
        match slot {
            SlotRef::CutRecipe(i) => {
                // `clickMenuButton`: a valid other recipe becomes the choice.
                if i < cut_list(self.session).len() && cut_selected(self.session) != Some(i) {
                    self.session.work_choice = Some(i as u16);
                    self.session.work_input = self.session.work[0].map(|s| s.item);
                    self.audio.ui_click();
                }
            }
            SlotRef::LoomChoice(i) => {
                let (list, _, _) = loom_view(self.session);
                if let Some(&p) = list.get(i) {
                    self.session.work_choice = Some(pattern_index(p));
                    self.audio.ui_click();
                }
            }
            SlotRef::CutResult | SlotRef::LoomResult | SlotRef::MapResult => {
                let Some(result) = self.workstation_result() else { return true };
                let cursor = self.session.inventory.cursor;
                let fits = match cursor {
                    None => true,
                    Some(c) => c.stacks_with(&result) && c.count as u16 + result.count as u16 <= c.max() as u16,
                };
                if fits && let Some(out) = self.take_workstation_result() {
                    match &mut self.session.inventory.cursor {
                        Some(c) => c.count += out.count,
                        None => self.session.inventory.cursor = Some(out),
                    }
                }
            }
            input => {
                let i = work_index(input).expect("an input slot");
                if self.session.inventory.cursor.is_none_or(|c| accepts(input, &c)) {
                    crate::inventory::click_slot(&mut self.session.work[i], &mut self.session.inventory.cursor, right);
                }
            }
        }
        sync_work_menu(*self.container, self.session);
        true
    }

    /// Shift-click on a workstation slot: inputs go back to the inventory;
    /// the result is made again and again into the inventory while the
    /// inputs last (`doClick`'s `QUICK_MOVE` loop), any part that doesn't
    /// fit dropped at the player. False when `slot` isn't one.
    pub(super) fn workstation_quick_move(&mut self, slot: SlotRef) -> bool {
        if !is_workstation_slot(slot) || !is_workstation(*self.container) {
            return false;
        }
        if let Some(i) = work_index(slot) {
            if let Some(stack) = self.session.work[i].take() {
                self.session.work[i] = self.move_to_player(stack);
            }
        } else if matches!(slot, SlotRef::CutResult | SlotRef::LoomResult | SlotRef::MapResult) {
            let order = to_player_order();
            for _ in 0..64 * 9 {
                let Some(result) = self.workstation_result() else { break };
                let mut slots = self.session.inventory.slots;
                let left = move_into(result, &mut slots, &order);
                if left == Some(result) {
                    break;
                }
                let Some(made) = self.take_workstation_result() else { break };
                // The map table's take gives the result its new map id.
                let left = if made == result {
                    self.session.inventory.slots = slots;
                    left
                } else {
                    move_into(made, &mut self.session.inventory.slots, &order)
                };
                if let Some(rest) = left {
                    self.entities.throw(rest, self.session.player.eye(), self.session.player.forward().as_dvec3());
                    break;
                }
            }
        } else {
            return self.workstation_click(slot, false);
        }
        sync_work_menu(*self.container, self.session);
        true
    }

    /// Shift-click from the inventory into the open workstation. `None`
    /// when the stack has no input there (it moves between the hotbar and
    /// the main grid instead).
    pub(super) fn move_to_workstation(&mut self, stack: Stack) -> Option<Option<Stack>> {
        // `BeaconMenu.quickMoveStack`: a lone payment item into the empty slot.
        if let Container::Beacon(_) = *self.container {
            let fits =
                self.session.work[0].is_none() && stack.count == 1 && voxelcraft::world::beacon::is_payment(stack.item);
            if !fits {
                return None;
            }
            self.session.work[0] = Some(stack);
            return Some(None);
        }
        let i = input_for(*self.container, stack.item)?;
        let left = move_into(stack, std::slice::from_mut(&mut self.session.work[i]), &[0]);
        sync_work_menu(*self.container, self.session);
        Some(left)
    }
}

fn take_one(slot: &mut Option<Stack>) {
    if let Some(s) = slot {
        s.count -= 1;
        if s.count == 0 {
            *slot = None;
        }
    }
}

// ---- Desktop presentation ----------------------------------------------------

/// A recipe or pattern button: (x, y, width, height) from the panel origin.
fn button_rect(container: Container, i: usize, scroll: usize) -> Option<(f32, f32, f32, f32)> {
    let (rows, x, y, w, h) = match container {
        Container::Stonecutter(_) => (CUT_ROWS, 52.0, 14.0, 16.0, 18.0),
        Container::Loom(_) => (LOOM_ROWS, 60.0, 13.0, 14.0, 14.0),
        _ => return None,
    };
    let first = scroll * COLS;
    if i < first || i >= first + rows * COLS {
        return None;
    }
    let k = i - first;
    Some((x + (k % COLS) as f32 * w, y + (k / COLS) as f32 * h, w, h))
}

impl Game {
    pub(super) fn open_workstation(&mut self, container: Container) {
        if self.inventory_open {
            return;
        }
        self.body.container = container;
        self.work_scroll = 0;
        self.session.work_choice = None;
        self.session.work_input = None;
        self.toggle_inventory();
    }

    /// How many entries the open workstation's list has, and how many rows
    /// show at once.
    fn work_list(&self) -> (usize, usize) {
        match self.body.container {
            Container::Stonecutter(_) => (cut_list(&self.session).len(), CUT_ROWS),
            Container::Loom(_) => (loom_view(&self.session).0.len(), LOOM_ROWS),
            _ => (0, 0),
        }
    }

    /// Scrolls the recipe or pattern list by whole rows (`mouseScrolled`).
    pub(super) fn scroll_work_list(&mut self, rows: i32) -> bool {
        let (len, shown) = self.work_list();
        if shown == 0 {
            return false;
        }
        let max = len.div_ceil(COLS).saturating_sub(shown);
        self.work_scroll = self.work_scroll.saturating_add_signed(rows as isize).min(max);
        true
    }

    /// The recipe or pattern button under (`mx`, `my`), for a panel at
    /// (`px`, `py`).
    pub(super) fn work_button_at(&self, px: f32, py: f32, mx: f32, my: f32) -> Option<SlotRef> {
        if let Container::Beacon(_) = self.body.container {
            return super::beacon::control_at(mx - px, my - py);
        }
        let (len, _) = self.work_list();
        (0..len).find_map(|i| {
            let (x, y, w, h) = button_rect(self.body.container, i, self.work_scroll)?;
            let hit = mx >= px + x && mx < px + x + w && my >= py + y && my < py + y + h;
            hit.then_some(match self.body.container {
                Container::Stonecutter(_) => SlotRef::CutRecipe(i),
                _ => SlotRef::LoomChoice(i),
            })
        })
    }

    /// The stacks the workstation slots show.
    pub(super) fn workstation_slot(&self, slot: SlotRef) -> Option<Stack> {
        match slot {
            SlotRef::CutResult => cut_result(&self.session),
            SlotRef::LoomResult => loom_view(&self.session).2,
            SlotRef::MapResult => cartography_result(&self.session, &self.world),
            s => self.session.work[work_index(s)?],
        }
    }

    /// Slot positions for the open workstation, from the panel's corner.
    pub(super) fn workstation_slots(&self, px: f32, py: f32, out: &mut Vec<(SlotRef, f32, f32)>) {
        match self.body.container {
            Container::Stonecutter(_) => {
                out.push((SlotRef::CutInput, px + 19.0, py + 32.0));
                out.push((SlotRef::CutResult, px + 142.0, py + 32.0));
            }
            Container::Loom(_) => {
                out.push((SlotRef::LoomBanner, px + 12.0, py + 25.0));
                out.push((SlotRef::LoomDye, px + 32.0, py + 25.0));
                out.push((SlotRef::LoomPattern, px + 22.0, py + 44.0));
                out.push((SlotRef::LoomResult, px + 142.0, py + 49.0));
            }
            Container::Cartography(_) => {
                out.push((SlotRef::MapInput, px + 14.0, py + 14.0));
                out.push((SlotRef::MapExtra, px + 14.0, py + 46.0));
                out.push((SlotRef::MapResult, px + 144.0, py + 30.0));
            }
            Container::Beacon(_) => {
                let (x, y) = super::beacon::PAYMENT;
                out.push((SlotRef::BeaconPayment, px + x, py + y));
            }
            _ => {}
        }
    }

    pub(super) fn workstation_title(&self) -> &'static str {
        match self.body.container {
            Container::Stonecutter(_) => "Stonecutter",
            Container::Loom(_) => "Loom",
            Container::Beacon(_) => "Beacon",
            _ => "Cartography Table",
        }
    }

    /// The faint item an empty input slot shows.
    pub(super) fn workstation_outline(slot: SlotRef) -> Option<u16> {
        match slot {
            SlotRef::LoomBanner => Some(voxelcraft::world::colour_blocks::tex::BANNER_ICON),
            SlotRef::LoomDye => crate::color::DyeColor::White.dye().icon_layer(),
            SlotRef::LoomPattern => bp::item(0).icon_layer(),
            SlotRef::MapInput => Item::FILLED_MAP.icon_layer(),
            SlotRef::MapExtra => Item::PAPER.icon_layer(),
            SlotRef::BeaconPayment => Item::IRON_INGOT.icon_layer(),
            _ => None,
        }
    }

    /// Draws the recipe list, pattern list or map preview.
    pub(super) fn workstation_ui(&self, ui: &mut Ui, px: f32, py: f32) {
        let dark = [0.45, 0.45, 0.45, 1.0];
        let mouse = (self.cursor_px.0 / ui.scale, self.cursor_px.1 / ui.scale);
        let hovered = self.work_button_at(px, py, mouse.0, mouse.1);
        match self.body.container {
            Container::Stonecutter(_) => {
                ui.rect(px + 51.0, py + 13.0, 66.0, 56.0, [0.55, 0.55, 0.55, 1.0]);
                let selected = cut_selected(&self.session);
                for (i, &(item, count)) in cut_list(&self.session).iter().enumerate() {
                    let Some((x, y, w, h)) = button_rect(self.body.container, i, self.work_scroll) else { continue };
                    let fill = if selected == Some(i) {
                        [0.35, 0.55, 0.35, 1.0]
                    } else if hovered == Some(SlotRef::CutRecipe(i)) {
                        [0.85, 0.85, 0.85, 1.0]
                    } else {
                        [0.7, 0.7, 0.7, 1.0]
                    };
                    ui.rect(px + x, py + y, w - 1.0, h - 1.0, fill);
                    self.stack_ui(ui, px + x - 1.0, py + y, Stack::new(item, count));
                }
                self.list_scrollbar(ui, px + 119.0, py + 14.0, 54.0);
            }
            Container::Loom(_) => {
                let (list, selected, result) = loom_view(&self.session);
                ui.rect(px + 59.0, py + 12.0, 58.0, 58.0, [0.55, 0.55, 0.55, 1.0]);
                let banner = self.session.work[0];
                let base = banner.and_then(|b| bp::banner_color(b.item)).unwrap_or(crate::color::DyeColor::White);
                let dye =
                    self.session.work[1].and_then(|d| bp::loom_dye(d.item)).unwrap_or(crate::color::DyeColor::Black);
                for (i, &p) in list.iter().enumerate() {
                    let Some((x, y, w, h)) = button_rect(self.body.container, i, self.work_scroll) else { continue };
                    let frame = if selected == Some(i) {
                        [0.35, 0.55, 0.35, 1.0]
                    } else if hovered == Some(SlotRef::LoomChoice(i)) {
                        [0.95, 0.95, 0.95, 1.0]
                    } else {
                        [0.3, 0.3, 0.3, 1.0]
                    };
                    ui.rect(px + x, py + y, w, h, frame);
                    // Each choice as `LoomScreen` draws it: the pattern in
                    // the dye's colour on a small flag of the banner's.
                    let layer = voxelcraft::inventory::components::BannerLayer { pattern: p, color: dye };
                    super::patterns::draw_flag(ui, px + x + 4.0, py + y + 1.0, 6.0, 12.0, base, &[layer]);
                }
                self.list_scrollbar(ui, px + 119.0, py + 13.0, 56.0);
                // The result banner, large, right of the list.
                if let Some(r) = result.or(banner) {
                    let p = bp::patterns(&r);
                    let base = bp::banner_color(r.item).unwrap_or(crate::color::DyeColor::White);
                    super::patterns::draw_flag(ui, px + 146.0, py + 6.0, 10.0, 20.0, base, &p.layers);
                }
                if banner.is_some_and(|b| bp::patterns(&b).layers.len() >= bp::MAX_LAYERS) {
                    ui.text_flat(px + 60.0, py + 72.0 - 12.0, "Too many layers", [0.7, 0.2, 0.2, 1.0]);
                }
            }
            Container::Cartography(_) => {
                ui.rect(px + 66.0, py + 12.0, 66.0, 52.0, dark);
                ui.rect(px + 67.0, py + 13.0, 64.0, 50.0, [0.82, 0.76, 0.6, 1.0]);
                let [map, extra, _] = self.session.work;
                let text = match (map.and_then(|m| self.world.maps.describe(&m)), extra.map(|e| e.item)) {
                    (Some((scale, locked)), extra) => match extra {
                        Some(Item::PAPER) if locked => "Locked".to_string(),
                        Some(Item::PAPER) if scale >= voxelcraft::world::maps::MAX_SCALE => {
                            "Fully zoomed out".to_string()
                        }
                        Some(Item::PAPER) => format!("Zoom out: 1:{}", 1u32 << (scale + 1)),
                        Some(Item::MAP) => "Copy".to_string(),
                        Some(i) if i == Item::from_block(Block::GLASS_PANE) && locked => "Already locked".into(),
                        Some(i) if i == Item::from_block(Block::GLASS_PANE) => "Lock".to_string(),
                        _ => format!("Scale 1:{}", 1u32 << scale),
                    },
                    _ => String::new(),
                };
                ui.text_flat(px + 70.0, py + 34.0, &text, [0.2, 0.15, 0.1, 1.0]);
                for i in 0..5 {
                    ui.rect(px + 116.0 + 20.0 + i as f32, py + 33.0 + i as f32, 1.0, 9.0 - 2.0 * i as f32, dark);
                }
            }
            Container::Beacon(pos) => {
                let levels = self.world.beacon(pos).map_or(0, |b| b.levels);
                super::beacon::draw(ui, &self.session, levels, px, py, hovered);
            }
            _ => {}
        }
    }

    fn list_scrollbar(&self, ui: &mut Ui, x: f32, y: f32, h: f32) {
        let (len, shown) = self.work_list();
        let rows = len.div_ceil(COLS).max(1) as f32;
        let thumb = (h * (shown as f32 / rows).min(1.0)).max(6.0);
        let max = (rows - shown as f32).max(1.0);
        let top = y + (h - thumb) * self.work_scroll as f32 / max;
        ui.rect(x, y, 6.0, h, [0.45, 0.45, 0.45, 1.0]);
        ui.rect(x, top, 6.0, thumb, if len > shown * COLS { [0.9, 0.9, 0.9, 1.0] } else { [0.6, 0.6, 0.6, 1.0] });
    }

    /// The block the open workstation screen belongs to is still there and
    /// in reach.
    pub(super) fn workstation_gone(&self, pos: IVec3, far: impl Fn(IVec3) -> bool) -> bool {
        let base = match self.body.container {
            Container::Stonecutter(_) => Block::STONECUTTER,
            Container::Loom(_) => Block::LOOM,
            Container::Cartography(_) => Block::CARTOGRAPHY_TABLE,
            Container::Beacon(_) => voxelcraft::world::beacon::BEACON,
            _ => return false,
        };
        self.world.get_block(pos).is_none_or(|b| b.base() != base) || far(pos)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::DyeColor;
    use voxelcraft::entity::PlayerId;

    fn session() -> PlayerSession {
        PlayerSession::new(PlayerId::HOST, glam::DVec3::ZERO)
    }

    #[test]
    fn the_stonecutter_forgets_its_recipe_when_the_input_changes() {
        let mut s = session();
        let cut = Container::Stonecutter(IVec3::ZERO);
        s.work[0] = Some(Stack::new(Block::STONE, 3));
        sync_work_menu(cut, &mut s);
        let list = cut_list(&s);
        assert!(list.len() > 1);
        s.work_choice = Some(1);
        assert_eq!(cut_result(&s), Some(Stack::new(list[1].0, list[1].1)));
        // More of the same input keeps the choice (`input.is(this.input.getItem())`).
        s.work[0] = Some(Stack::new(Block::STONE, 5));
        sync_work_menu(cut, &mut s);
        assert_eq!(cut_selected(&s), Some(1));
        s.work[0] = Some(Stack::new(Block::COBBLESTONE, 1));
        sync_work_menu(cut, &mut s);
        assert_eq!((s.work_choice, cut_result(&s)), (None, None));
        s.work[0] = None;
        sync_work_menu(cut, &mut s);
        assert!(cut_list(&s).is_empty());
    }

    #[test]
    fn the_loom_picks_a_pattern_items_pattern_and_drops_a_choice_no_longer_offered() {
        let mut s = session();
        let loom = Container::Loom(IVec3::ZERO);
        s.work[0] = Some(Stack::new(bp::banner_item(DyeColor::White), 1));
        s.work[1] = Some(Stack::new(DyeColor::Blue.dye(), 2));
        sync_work_menu(loom, &mut s);
        let (list, selected, result) = loom_view(&s);
        assert_eq!((list.len(), selected, result), (32, None, None));
        s.work_choice = Some(pattern_index(list[3]));
        assert_eq!(loom_view(&s).1, Some(3));
        s.work[2] = Some(Stack::new(bp::item(4), 1));
        sync_work_menu(loom, &mut s);
        let (list, selected, result) = loom_view(&s);
        assert_eq!((list.len(), selected), (1, Some(0)));
        assert_eq!(bp::patterns(&result.unwrap()).layers[0].pattern.name(), "globe");
        s.work[2] = None;
        sync_work_menu(loom, &mut s);
        assert_eq!(loom_view(&s).1, None, "the globe isn't in the plain list");
    }

    #[test]
    fn shift_clicks_route_to_the_right_inputs() {
        let loom = Container::Loom(IVec3::ZERO);
        assert_eq!(input_for(loom, bp::banner_item(DyeColor::Red)), Some(0));
        assert_eq!(input_for(loom, DyeColor::Red.dye()), Some(1));
        assert_eq!(input_for(loom, bp::item(0)), Some(2));
        assert_eq!(input_for(loom, Item::PAPER), None);
        let table = Container::Cartography(IVec3::ZERO);
        assert_eq!(input_for(table, Item::FILLED_MAP), Some(0));
        assert_eq!(input_for(table, Item::PAPER), Some(1));
        assert_eq!(input_for(table, Item::MAP), Some(1));
        assert_eq!(input_for(table, Item::from_block(Block::GLASS_PANE)), Some(1));
        let cut = Container::Stonecutter(IVec3::ZERO);
        assert_eq!(input_for(cut, Item::from_block(Block::STONE)), Some(0));
        assert_eq!(input_for(cut, Item::PAPER), None);
    }
}
