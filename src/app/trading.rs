//! Host and controller merchants use the same atomic transaction as CLI agents.
use super::hud::draw_stack;
use super::{Container, Game};
use crate::render::ui::{Ui, WHITE};
impl Game {
    pub(super) fn use_villager(&mut self) -> bool {
        if self.sneak_building() {
            return false;
        }
        let Some(id) = self.mobs.entities.target_merchant(
            &self.world,
            self.player.eye(),
            self.player.forward().as_dvec3(),
            super::REACH,
        ) else {
            return false;
        };
        if self
            .mobs
            .entities
            .merchant(id)
            .and_then(|m| m.villager.as_ref())
            .is_none_or(|v| v.offers.iter().all(Option::is_none))
        {
            return true;
        }
        if self.puppet {
            self.puppet_merchant = Some(id);
        } else if !self.inventory_open {
            self.container = Container::Trading(id);
            self.toggle_inventory();
        }
        true
    }
    pub(super) fn buy_trade(&mut self, index: usize) {
        let Container::Trading(id) = self.container else { return };
        if !self.mobs.entities.merchant_in_reach(id, self.player.eye()) {
            return;
        }
        match self.mobs.entities.trade(id, index, &mut self.inventory) {
            Ok(xp) => {
                self.mobs.entities.spawn_xp(self.player.pos, xp);
                self.audio.ui_click();
            }
            Err(message) => self.show_popup(message),
        }
    }
    pub(super) fn trading_ui(&self, ui: &mut Ui, id: u64, px: f32, py: f32) {
        let Some(v) = self.mobs.entities.merchant(id).and_then(|m| m.villager.as_ref()) else { return };
        // Java titles the screen "<Profession> - <Level>".
        let ink = [0.25, 0.25, 0.25, 1.0];
        let mut x = px + 8.0;
        for part in [v.profession.name(), " - ", v.level_name()] {
            ui.text_flat(x, py + 6.0, part, ink);
            x += Ui::text_width(part);
        }
        for (i, o) in v.offers.iter().enumerate() {
            let Some(o) = o else { continue };
            let x = px + 8.0 + (i % 2) as f32 * 80.0;
            let y = py + 22.0 + (i / 2) as f32 * 20.0;
            draw_stack(ui, x, y, v.priced(*o), true, self.dial_of(&self.player));
            if let Some(s) = o.second {
                draw_stack(ui, x + 20.0, y, s, true, self.dial_of(&self.player));
            }
            ui.text_flat(
                x + 40.0,
                y + 5.0,
                if o.stocked() { ">" } else { "X" },
                if o.stocked() { [0.15, 0.4, 0.12, 1.0] } else { [0.7, 0.1, 0.1, 1.0] },
            );
        }
        let (lo, hi) = match v.level {
            1 => (0, 10),
            2 => (10, 70),
            3 => (70, 150),
            4 => (150, 250),
            _ => (250, 250),
        };
        let progress = if hi == lo { 1.0 } else { (v.xp.saturating_sub(lo) as f32 / (hi - lo) as f32).min(1.0) };
        ui.rect(px + 8.0, py + 24.0 + v.level as f32 * 20.0, 160.0, 2.0, [0.2, 0.2, 0.2, 1.0]);
        ui.rect(px + 8.0, py + 24.0 + v.level as f32 * 20.0, 160.0 * progress, 2.0, [0.2, 0.65, 0.15, 1.0]);
    }
    pub(super) fn pad_trade_details(&self, ui: &mut Ui, id: u64, index: usize, x: f32, y: f32) {
        let Some(v) = self.mobs.entities.merchant(id).and_then(|m| m.villager.as_ref()) else { return };
        ui.text(x, y, v.profession.name(), WHITE);
        ui.text(x + 100.0, y, v.level_name(), WHITE);
        if let Some(o) = v.offers.get(index).copied().flatten() {
            let dial = self.dial_of(&self.player);
            draw_stack(ui, x, y + 12.0, v.priced(o), true, dial);
            ui.text(x + 22.0, y + 16.0, o.cost.item.name(), WHITE);
            let sx = x + 22.0 + Ui::text_width(o.cost.item.name()) + 10.0;
            if let Some(s) = o.second {
                draw_stack(ui, sx, y + 12.0, s, true, dial);
                ui.text(sx + 20.0, y + 16.0, s.item.name(), WHITE);
            }
            ui.text(
                x,
                y + 36.0,
                if o.stocked() { "A/X/Y: trade to inventory" } else { "Out of stock - needs to work" },
                WHITE,
            );
        }
    }
}
