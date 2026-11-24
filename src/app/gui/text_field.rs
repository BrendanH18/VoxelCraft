//! An editable one-line text field, like Java's edit box: a caret, a
//! selection, word jumps and the clipboard. Text is any character the font
//! draws (`voxelcraft::font::allowed`): ASCII, Latin-1 and Latin
//! Extended-A. The caret and selection are byte indices that always sit on
//! character boundaries, and the length limit counts characters.

use std::cell::Cell;

use winit::keyboard::KeyCode;

use super::{Clipboard, Key, Rect};
use crate::render::ui::{Color, Ui, WHITE};

const TEXT: Color = [0.88, 0.88, 0.88, 1.0];
const DISABLED_TEXT: Color = [0.44, 0.44, 0.44, 1.0];
const BORDER: Color = [0.63, 0.63, 0.63, 1.0];
const SELECTION: Color = [0.0, 0.0, 1.0, 0.55];

/// Characters a field can hold.
pub(super) fn allowed(c: char) -> bool {
    voxelcraft::font::allowed(c)
}

#[derive(Clone, Debug)]
pub(in crate::app) struct TextField {
    text: String,
    /// Caret, as a byte index on a character boundary.
    cursor: usize,
    /// The other end of the selection; equal to `cursor` when nothing is selected.
    anchor: usize,
    /// Longest text in characters (Java's `maxLength`, in UTF-16 units,
    /// which is the same for every character the font draws).
    max: usize,
    /// Widest text in font pixels ([`voxelcraft::font::width`]), if limited:
    /// an edit that would pass it is refused whole, as Java's sign editor's
    /// `TextFieldHelper` refuses it. Text set from outside may be wider.
    max_width: Option<i32>,
    /// First character shown, moved to keep the caret in view.
    start: Cell<usize>,
    /// Grey text shown while the field is empty.
    hint: String,
}

impl TextField {
    pub fn new(max: usize) -> Self {
        Self {
            text: String::new(),
            cursor: 0,
            anchor: 0,
            max,
            max_width: None,
            start: Cell::new(0),
            hint: String::new(),
        }
    }

    pub fn with_text(text: &str, max: usize) -> Self {
        let mut field = Self::new(max);
        field.set(text);
        field
    }

    /// Limits typing to `width` font pixels (see `max_width`).
    pub fn max_width(mut self, width: i32) -> Self {
        self.max_width = Some(width);
        self
    }

    pub fn hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = hint.into();
        self
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    /// Replaces the text and puts the caret at its end. Only the character
    /// set and length limit apply: a sign line wider than its width keeps
    /// every character.
    pub fn set(&mut self, text: &str) {
        self.text = text.chars().filter(|&c| allowed(c)).take(self.max).collect();
        self.cursor = self.text.len();
        self.anchor = self.cursor;
        self.start.set(0);
    }

    /// The selected range, start first.
    pub fn selection(&self) -> (usize, usize) {
        (self.cursor.min(self.anchor), self.cursor.max(self.anchor))
    }

    pub fn selected(&self) -> &str {
        let (a, b) = self.selection();
        &self.text[a..b]
    }

    #[cfg(test)]
    pub fn cursor(&self) -> usize {
        self.cursor
    }

    pub fn select_all(&mut self) {
        self.anchor = 0;
        self.cursor = self.text.len();
    }

    /// Types `s` over the selection, dropping characters the field doesn't
    /// take and anything past its length limit.
    pub fn insert(&mut self, s: &str) {
        let (a, b) = self.selection();
        let kept = self.text.len() - (b - a);
        let room = self.max.saturating_sub(self.text[..a].chars().count() + self.text[b..].chars().count());
        let s: String = s.chars().filter(|&c| allowed(c)).take(room).collect();
        if let Some(max) = self.max_width {
            let mut next = String::with_capacity(kept + s.len());
            next.push_str(&self.text[..a]);
            next.push_str(&s);
            next.push_str(&self.text[b..]);
            if voxelcraft::font::width(&next) > max {
                return;
            }
        }
        self.delete_selection();
        self.text.insert_str(self.cursor, &s);
        self.cursor += s.len();
        self.anchor = self.cursor;
    }

    fn delete_selection(&mut self) -> bool {
        let (a, b) = self.selection();
        if a == b {
            return false;
        }
        self.text.replace_range(a..b, "");
        self.cursor = a;
        self.anchor = a;
        true
    }

    /// The character boundary before `i`.
    fn prev(&self, i: usize) -> usize {
        self.text[..i].char_indices().next_back().map_or(0, |(j, _)| j)
    }

    /// The character boundary after `i`.
    fn next(&self, i: usize) -> usize {
        self.text[i..].chars().next().map_or(i, |c| i + c.len_utf8())
    }

    fn move_to(&mut self, pos: usize, extend: bool) {
        self.cursor = pos.min(self.text.len());
        if !extend {
            self.anchor = self.cursor;
        }
    }

    /// Start of the word before `from`.
    fn word_left(&self, from: usize) -> usize {
        let b = self.text.as_bytes();
        let mut i = from;
        while i > 0 && b[i - 1] == b' ' {
            i -= 1;
        }
        while i > 0 && b[i - 1] != b' ' {
            i -= 1;
        }
        i
    }

    /// End of the word after `from` (and the spaces after it), as Java jumps.
    fn word_right(&self, from: usize) -> usize {
        let b = self.text.as_bytes();
        let mut i = from;
        while i < b.len() && b[i] != b' ' {
            i += 1;
        }
        while i < b.len() && b[i] == b' ' {
            i += 1;
        }
        i
    }

    /// Edits for a key press; returns whether the field used it.
    pub fn key(&mut self, key: &Key, clipboard: &mut dyn Clipboard) -> bool {
        let word = key.command || key.alt;
        match key.code {
            KeyCode::KeyA if key.command => self.select_all(),
            KeyCode::KeyC if key.command => {
                if !self.selected().is_empty() {
                    clipboard.set(self.selected());
                }
            }
            KeyCode::KeyX if key.command => {
                if !self.selected().is_empty() {
                    clipboard.set(self.selected());
                    self.delete_selection();
                }
            }
            KeyCode::KeyV if key.command => {
                if let Some(text) = clipboard.get() {
                    self.insert(&text);
                }
            }
            KeyCode::Backspace => {
                if !self.delete_selection() && self.cursor > 0 {
                    let to = if word { self.word_left(self.cursor) } else { self.prev(self.cursor) };
                    self.text.replace_range(to..self.cursor, "");
                    self.move_to(to, false);
                }
            }
            KeyCode::Delete => {
                if !self.delete_selection() && self.cursor < self.text.len() {
                    let to = if word { self.word_right(self.cursor) } else { self.next(self.cursor) };
                    self.text.replace_range(self.cursor..to, "");
                }
            }
            KeyCode::ArrowLeft => {
                let (a, b) = self.selection();
                let to = if word {
                    self.word_left(self.cursor)
                } else if !key.shift && a != b {
                    a
                } else {
                    self.prev(self.cursor)
                };
                self.move_to(to, key.shift);
            }
            KeyCode::ArrowRight => {
                let (a, b) = self.selection();
                let to = if word {
                    self.word_right(self.cursor)
                } else if !key.shift && a != b {
                    b
                } else {
                    self.next(self.cursor)
                };
                self.move_to(to, key.shift);
            }
            KeyCode::Home => self.move_to(0, key.shift),
            KeyCode::End => self.move_to(self.text.len(), key.shift),
            _ => match &key.text {
                Some(text) if !key.command => self.insert(text),
                _ => return false,
            },
        }
        true
    }

    /// Width the text gets inside a field `w` wide.
    fn inner(w: f32) -> f32 {
        w - 8.0
    }

    /// The first character to show so the caret stays inside `width`.
    fn visible_start(&self, width: f32) -> usize {
        let mut start = self.start.get().min(self.cursor).min(self.text.len());
        while !self.text.is_char_boundary(start) {
            start -= 1;
        }
        while start < self.cursor && Ui::text_width(&self.text[start..self.cursor]) > width {
            start = self.next(start);
        }
        // Show more on the left once text is deleted from the right.
        while start > 0 && Ui::text_width(&self.text[self.prev(start)..]) <= width {
            start = self.prev(start);
        }
        self.start.set(start);
        start
    }

    /// Puts the caret at the character boundary nearest `x` UI pixels from
    /// the field's left edge, clearing the selection.
    pub fn click(&mut self, x: f32) {
        let mut start = self.start.get().min(self.text.len());
        while !self.text.is_char_boundary(start) {
            start -= 1;
        }
        let x = x - 4.0;
        let at = |k: usize| if k == start { 0.0 } else { Ui::text_width(&self.text[start..k]) + 1.0 };
        let boundaries = self.text[start..].char_indices().map(|(i, _)| start + i).chain([self.text.len()]);
        let pos = boundaries.min_by(|&a, &b| (at(a) - x).abs().total_cmp(&(at(b) - x).abs()));
        self.move_to(pos.unwrap_or(self.text.len()), false);
    }

    /// Java's look: a black box with a grey border, white when focused, and
    /// a blinking caret.
    /// v0.18 Track 2: the text alone, white, from `x` in `w` pixels, with
    /// the caret (the anvil's borderless, always-focused name box).
    pub fn draw_plain(&self, ui: &mut Ui, [x, y, w, _]: Rect, caret: bool) {
        let start = self.visible_start(w);
        let shown = &self.text[start..];
        let mut end = shown.len();
        while end > 0 && Ui::text_width(&shown[..end]) > w {
            end -= 1;
        }
        ui.text(x, y, &shown[..end], WHITE);
        if caret {
            let at = self.cursor.clamp(start, start + end);
            let cx = if at == start { x } else { x + Ui::text_width(&self.text[start..at]) + 1.0 };
            if self.cursor == self.text.len() {
                ui.text(cx, y, "_", WHITE);
            } else {
                ui.rect(cx - 1.0, y - 1.0, 1.0, 10.0, WHITE);
            }
        }
    }

    pub fn draw(&self, ui: &mut Ui, [x, y, w, h]: Rect, focused: bool, caret: bool, enabled: bool) {
        ui.rect(x, y, w, h, if focused { WHITE } else { BORDER });
        ui.rect(x + 1.0, y + 1.0, w - 2.0, h - 2.0, [0.0, 0.0, 0.0, 1.0]);
        let width = Self::inner(w);
        let (tx, ty) = (x + 4.0, y + ((h - 8.0) / 2.0).floor());
        if self.text.is_empty() && !focused {
            ui.text(tx, ty, &super::fit(&self.hint, width), DISABLED_TEXT);
        }
        let start = self.visible_start(width);
        let shown = &self.text[start..];
        let mut end = shown.len();
        while end > 0 && Ui::text_width(&shown[..end]) > width {
            end = shown[..end].char_indices().next_back().map_or(0, |(j, _)| j);
        }
        let shown = &shown[..end];
        ui.text(tx, ty, shown, if enabled { TEXT } else { DISABLED_TEXT });
        let offset = |i: usize| {
            let i = i.clamp(start, start + end);
            if i == start { 0.0 } else { Ui::text_width(&self.text[start..i]) + 1.0 }
        };
        let (a, b) = self.selection();
        if focused && a != b {
            let (sx, ex) = (offset(a), offset(b));
            ui.rect(tx + sx - 1.0, ty - 1.0, ex - sx, 10.0, SELECTION);
        }
        if caret && enabled {
            let cx = tx + offset(self.cursor);
            if self.cursor == self.text.len() {
                ui.text(cx, ty, "_", TEXT);
            } else {
                ui.rect(cx - 1.0, ty - 1.0, 1.0, 10.0, [0.82, 0.82, 0.82, 1.0]);
            }
        }
    }
}
