//! Java's Play Multiplayer screen (`JoinMultiplayerScreen`) for VoxelCraft's
//! LAN games, with Direct Connection (`DirectJoinServerScreen`) and the
//! connecting screen (`ConnectScreen`). VoxelCraft has no saved server list,
//! so Add Server, Edit and Delete are left out; the list shows the games
//! found on the local network, as Java's LAN section does. With four
//! buttons instead of seven, they are 150 wide in two rows (Java's three
//! and four share the same 308-pixel width). VoxelCraft needs
//! a profile name to join, so the header has a Local Profile field where
//! Java's has none.

use std::net::SocketAddr;
use std::time::Instant;

use crossbeam_channel::Receiver;

use super::{Action, Title};
use crate::app::gui::{BUTTON_H, GREY, Look, Notice, Rect, Screen, Size, Step, TextField, Widget, fit};
use crate::render::ui::{Color, Ui, WHITE};

const HEADER: f32 = 49.0;
const FOOTER: f32 = 60.0;
/// Java's server list rows.
const ROW_W: f32 = 305.0;
const ROW_H: f32 = 36.0;
const PROFILE_MAX: usize = 24;
const ADDRESS_MAX: usize = 128;
const DOUBLE_CLICK_MS: u128 = 250;
const LINE_GREY: Color = [0.5, 0.5, 0.5, 1.0];
const DARK_GREY: Color = [0.19, 0.19, 0.19, 1.0];
const RED: Color = [1.0, 0.33, 0.33, 1.0];
const PROFILE_LABEL: &str = "Local Profile";

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(in crate::app) enum Id {
    Profile,
    Server(usize),
    Join,
    Direct,
    Refresh,
    Back,
}

pub(in crate::app) struct Multiplayer {
    discovery: Result<voxelcraft::lan::Discovery, String>,
    profile: TextField,
    selected: Option<SocketAddr>,
    last_click: Option<(usize, Instant)>,
}

impl Multiplayer {
    pub fn new(title: &Title) -> Self {
        Self {
            discovery: Self::scan(),
            profile: TextField::with_text(&title.profile, PROFILE_MAX),
            selected: None,
            last_click: None,
        }
    }

    fn scan() -> Result<voxelcraft::lan::Discovery, String> {
        voxelcraft::lan::Discovery::new().map_err(|e| e.to_string())
    }

    fn servers(&self) -> &[voxelcraft::lan::Advertisement] {
        self.discovery.as_ref().map_or(&[], |d| &d.servers)
    }

    fn list_rect((sw, sh): Size) -> Rect {
        [((sw - ROW_W) / 2.0).floor(), HEADER, ROW_W + 14.0, (sh - HEADER - FOOTER).max(ROW_H)]
    }

    fn row(i: usize, size: Size) -> Rect {
        let [x, top, ..] = Self::list_rect(size);
        [x, top + 4.0 + i as f32 * ROW_H, ROW_W, ROW_H]
    }

    fn join_selected(&mut self, title: &mut Title) -> Step<Title> {
        let Some(address) = self.selected.filter(|a| self.servers().iter().any(|s| s.address == *a)) else {
            return Step::Stay;
        };
        title.profile = self.profile.text().to_string();
        connect(title, address.to_string())
    }
}

/// Starts connecting to `address` behind Java's connecting screen.
pub(super) fn connect(title: &mut Title, address: String) -> Step<Title> {
    if !voxelcraft::control::valid_name(&title.profile) {
        return Step::push(Notice::new(
            "Failed to connect to the server",
            vec!["Your Local Profile name must be 1 to 24 letters, digits or underscores.".into()],
        ));
    }
    let path = title.saves_dir.parent().unwrap_or(&title.saves_dir).join("lan-profile.txt");
    let _ = std::fs::create_dir_all(path.parent().unwrap());
    if let Err(e) = std::fs::write(&path, &title.profile) {
        log::warn!("could not save the LAN profile to {}: {e}", path.display());
    }
    let (tx, rx) = crossbeam_channel::bounded(1);
    let profile = title.profile.clone();
    std::thread::spawn(move || {
        use std::net::ToSocketAddrs;
        let result = (|| {
            let addresses = address.to_socket_addrs().map_err(|e| e.to_string())?;
            let mut last = "No address found".to_string();
            for address in addresses.take(4) {
                match voxelcraft::lan::Connection::connect(address, &profile) {
                    Ok(c) => return Ok(c),
                    Err(e) => last = e.to_string(),
                }
            }
            Err(last)
        })();
        let _ = tx.send(result);
    });
    Step::push(Connecting { rx, profile: title.profile.clone() })
}

impl Screen<Title> for Multiplayer {
    type Id = Id;

    fn title(&self, _: &Title) -> String {
        "Play Multiplayer".into()
    }

    fn widgets(&self, _: &Title, size: Size) -> Vec<Widget<Id>> {
        let (sw, sh) = size;
        let cx = (sw / 2.0).floor();
        let label = Ui::text_width(PROFILE_LABEL) + 4.0;
        let left = (cx - (label + 150.0) / 2.0).floor();
        let mut out = vec![
            Widget::field(Id::Profile, [left + label, 21.0, 150.0, BUTTON_H])
                .tooltip("The name other players see: 1 to 24 letters, digits or underscores."),
        ];
        for (i, server) in self.servers().iter().enumerate() {
            let mut row = Widget::custom(Id::Server(i), Self::row(i, size)).scrolling();
            if let Some(v) = server.version.as_deref().filter(|v| *v != voxelcraft::lan::GAME_VERSION) {
                row = row.tooltip(format!("Incompatible version! This game needs VoxelCraft {v}."));
            }
            out.push(row);
        }
        let selected = self.selected.is_some_and(|a| self.servers().iter().any(|s| s.address == a));
        let valid = voxelcraft::control::valid_name(self.profile.text());
        let (row1, row2) = ((sh - FOOTER + 8.0).floor(), (sh - FOOTER + 32.0).floor());
        out.extend([
            Widget::button(Id::Join, [cx - 152.0, row1, 150.0, BUTTON_H], "Join Server").enabled(selected && valid),
            Widget::button(Id::Direct, [cx + 2.0, row1, 150.0, BUTTON_H], "Direct Connection").enabled(valid),
            Widget::button(Id::Refresh, [cx - 152.0, row2, 150.0, BUTTON_H], "Refresh"),
            Widget::button(Id::Back, [cx + 2.0, row2, 150.0, BUTTON_H], "Back"),
        ]);
        out
    }

    fn activate(&mut self, title: &mut Title, id: Id) -> Step<Title> {
        match id {
            Id::Profile => Step::Stay,
            Id::Server(i) => {
                self.selected = self.servers().get(i).map(|s| s.address);
                self.join_selected(title)
            }
            Id::Join => self.join_selected(title),
            Id::Direct => {
                title.profile = self.profile.text().to_string();
                Step::push(DirectConnect::new(&title.address))
            }
            Id::Refresh => {
                // The old socket holds the discovery port until it's dropped.
                self.discovery = Err(String::new());
                self.discovery = Self::scan();
                self.selected = None;
                Step::Stay
            }
            Id::Back => Step::Pop,
        }
    }

    fn click(&mut self, title: &mut Title, id: Id) -> Step<Title> {
        let Id::Server(i) = id else { return self.activate(title, id) };
        let double = self.last_click.is_some_and(|(j, t)| j == i && t.elapsed().as_millis() < DOUBLE_CLICK_MS);
        self.selected = self.servers().get(i).map(|s| s.address);
        self.last_click = Some((i, Instant::now()));
        if double { self.join_selected(title) } else { Step::Stay }
    }

    fn focus(&mut self, _: &mut Title, id: Id) {
        if let Id::Server(i) = id {
            self.selected = self.servers().get(i).map(|s| s.address);
        }
    }

    fn back(&mut self, title: &mut Title) -> Step<Title> {
        title.profile = self.profile.text().to_string();
        Step::Pop
    }

    fn field(&mut self, id: Id) -> Option<&mut TextField> {
        (id == Id::Profile).then_some(&mut self.profile)
    }

    fn field_ref(&self, id: Id) -> Option<&TextField> {
        (id == Id::Profile).then_some(&self.profile)
    }

    fn tick(&mut self, _: &mut Title) -> Step<Title> {
        if let Ok(discovery) = &mut self.discovery {
            discovery.poll();
        }
        Step::Stay
    }

    fn list(&self, _: &Title, size: Size) -> Option<Rect> {
        Some(Self::list_rect(size))
    }

    fn scroll_step(&self) -> f32 {
        ROW_H / 2.0
    }

    fn title_y(&self, _: Size, _: f32) -> f32 {
        8.0
    }

    fn draw(&self, _: &Title, ui: &mut Ui, size: Size, scroll: f32) {
        let cx = (size.0 / 2.0).floor();
        let label = Ui::text_width(PROFILE_LABEL) + 4.0;
        ui.text((cx - (label + 150.0) / 2.0).floor(), 27.0, PROFILE_LABEL, GREY);
        // Java's LAN scanning line, under the games found.
        let [_, top, _, h] = Self::list_rect(size);
        let y = Self::row(self.servers().len(), size)[1] - scroll + 6.0;
        if y + 20.0 > top + h {
            return;
        }
        let centred = |ui: &mut Ui, y: f32, text: &str, color| {
            ui.text((cx - Ui::text_width(text) / 2.0).floor(), y, text, color);
        };
        match &self.discovery {
            Ok(_) => {
                centred(ui, y, "Scanning for games on your local network", WHITE);
                // Java's `LoadingDotsText`.
                let frame = (std::time::UNIX_EPOCH.elapsed().map_or(0, |d| d.as_millis()) / 300 % 4) as usize;
                centred(ui, y + 11.0, ["O o o", "o O o", "o o O", "o O o"][frame], LINE_GREY);
            }
            Err(e) => centred(ui, y, &fit(&format!("Can't look for LAN games: {e}"), size.0 - 16.0), RED),
        }
    }

    fn draw_custom(&self, _: &Title, ui: &mut Ui, id: Id, [x, y, w, h]: Rect, look: Look) {
        let Id::Server(i) = id else { return };
        let Some(server) = self.servers().get(i) else { return };
        if self.selected == Some(server.address) {
            let outline = if look.focused { WHITE } else { LINE_GREY };
            ui.rect(x, y, w, h, outline);
            ui.rect(x + 1.0, y + 1.0, w - 2.0, h - 2.0, [0.0, 0.0, 0.0, 1.0]);
        }
        let (tx, ty) = (x + 2.0 + 35.0, y + 2.0);
        crate::app::gui::bevel(ui, [x + 2.0, ty, 32.0, 32.0], [0.24, 0.24, 0.24, 1.0], false);
        let room = x + w - 4.0 - tx;
        ui.text(tx, ty + 1.0, "LAN World", WHITE);
        ui.text(tx, ty + 12.0, &fit(&server.name, room), LINE_GREY);
        let detail = match (server.version.as_deref(), server.players) {
            (Some(v), _) if v != voxelcraft::lan::GAME_VERSION => (format!("Needs VoxelCraft {v}"), RED),
            (_, Some((n, max))) => (format!("{} - {n}/{max} players", server.address), DARK_GREY),
            _ => (server.address.to_string(), DARK_GREY),
        };
        ui.text(tx, ty + 23.0, &fit(&detail.0, room), detail.1);
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(in crate::app) enum DirectId {
    Address,
    Join,
    Cancel,
}

/// Java's Direct Connection screen: a server address, Join Server, Cancel.
pub(in crate::app) struct DirectConnect {
    address: TextField,
}

impl DirectConnect {
    pub fn new(address: &str) -> Self {
        Self { address: TextField::with_text(address, ADDRESS_MAX) }
    }

    /// Java's rows (title 20, label 100, field 116, buttons at
    /// `height / 4 + 108` and `+ 132`), raised to fit short windows.
    fn rows((_, sh): Size) -> (f32, f32) {
        let join = (sh / 4.0 + 108.0).min(sh - 48.0).floor();
        (116.0f32.min(join - 28.0), join)
    }
}

impl Screen<Title> for DirectConnect {
    type Id = DirectId;

    fn title(&self, _: &Title) -> String {
        "Direct Connection".into()
    }

    fn widgets(&self, _: &Title, size: Size) -> Vec<Widget<DirectId>> {
        let x = (size.0 / 2.0 - 100.0).floor();
        let (field, join) = Self::rows(size);
        let typed = !self.address.text().trim().is_empty();
        vec![
            Widget::field(DirectId::Address, [x, field, 200.0, BUTTON_H]),
            Widget::button(DirectId::Join, [x, join, 200.0, BUTTON_H], "Join Server").enabled(typed),
            Widget::button(DirectId::Cancel, [x, join + 24.0, 200.0, BUTTON_H], "Cancel"),
        ]
    }

    fn activate(&mut self, title: &mut Title, id: DirectId) -> Step<Title> {
        match id {
            DirectId::Address | DirectId::Join => {
                let address = self.address.text().trim().to_string();
                if address.is_empty() {
                    return Step::Stay;
                }
                title.address = address.clone();
                Step::Pop.then(connect(title, address))
            }
            DirectId::Cancel => Step::Pop,
        }
    }

    fn field(&mut self, id: DirectId) -> Option<&mut TextField> {
        (id == DirectId::Address).then_some(&mut self.address)
    }

    fn field_ref(&self, id: DirectId) -> Option<&TextField> {
        (id == DirectId::Address).then_some(&self.address)
    }

    fn initial_focus(&self) -> Option<DirectId> {
        Some(DirectId::Address)
    }

    fn title_y(&self, size: Size, _: f32) -> f32 {
        20.0f32.min(Self::rows(size).0 - 36.0)
    }

    fn draw(&self, _: &Title, ui: &mut Ui, size: Size, _: f32) {
        let x = (size.0 / 2.0 - 100.0).floor();
        ui.text(x + 1.0, Self::rows(size).0 - 16.0, "Server Address", [0.63, 0.63, 0.63, 1.0]);
    }
}

type Connected = Result<voxelcraft::lan::Connection, String>;

/// Java's connecting screen: the status and Cancel.
pub(in crate::app) struct Connecting {
    rx: Receiver<Connected>,
    profile: String,
}

impl Screen<Title> for Connecting {
    type Id = ();

    fn title(&self, _: &Title) -> String {
        "Connecting to the server...".into()
    }

    fn widgets(&self, _: &Title, (sw, sh): Size) -> Vec<Widget<()>> {
        let y = (sh / 4.0 + 132.0).min(sh - 24.0).floor();
        vec![Widget::button((), [(sw / 2.0 - 100.0).floor(), y, 200.0, BUTTON_H], "Cancel")]
    }

    /// Cancel: the connection is dropped if it arrives later.
    fn activate(&mut self, _: &mut Title, _: ()) -> Step<Title> {
        Step::Pop
    }

    fn tick(&mut self, _: &mut Title) -> Step<Title> {
        match self.rx.try_recv() {
            Ok(Ok(connection)) => Step::Pop.then(Step::Out(Action::Join(connection, self.profile.clone()))),
            Ok(Err(e)) => Step::replace(Notice::new("Failed to connect to the server", vec![e])),
            Err(crossbeam_channel::TryRecvError::Empty) => Step::Stay,
            Err(crossbeam_channel::TryRecvError::Disconnected) => Step::replace(Notice::new(
                "Failed to connect to the server",
                vec!["The connection attempt stopped unexpectedly.".into()],
            )),
        }
    }

    fn title_y(&self, (_, sh): Size, _: f32) -> f32 {
        (sh / 2.0 - 50.0).floor().max(8.0)
    }
}
