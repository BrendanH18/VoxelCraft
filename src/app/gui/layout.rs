//! Java's screen layouts: a centred column of rows (the pause menu), and the
//! two-column options list between a 33-pixel header and footer.

use super::{BUTTON_H, BUTTON_W, GAP, Rect, Size, Widget};

/// Height of an options list row.
pub(super) const ROW: f32 = 25.0;
pub(in crate::app) const HEADER: f32 = 33.0;
pub(in crate::app) const FOOTER: f32 = 33.0;
/// A header title, centred in the header.
pub(super) const TITLE_Y: f32 = 12.0;
/// One options column, and the two together with Java's 10-pixel gap.
pub(in crate::app) const OPTION_W: f32 = 150.0;
pub(in crate::app) const LIST_W: f32 = 310.0;
const COLUMN_GAP: f32 = LIST_W - 2.0 * OPTION_W;

/// Rows of widgets, each row `width` wide and split evenly, centred on screen
/// as one block with the title 20 pixels above it. Gaps shrink in short
/// windows (down to 1 pixel) so everything stays on screen.
pub(in crate::app) fn centred_rows<Id>(rows: Vec<Vec<Widget<Id>>>, width: f32, (sw, sh): Size) -> Vec<Widget<Id>> {
    let n = rows.len() as f32;
    let gap = GAP.min(((sh - n * BUTTON_H - 24.0) / (n + 1.0)).max(1.0));
    let total = n * BUTTON_H + (n - 1.0).max(0.0) * gap;
    let mut y = ((sh - total) / 2.0).floor() + 10.0;
    let left = ((sw - width) / 2.0).floor();
    let mut out = Vec::new();
    for row in rows {
        let k = row.len().max(1) as f32;
        let w = ((width - (k - 1.0) * GAP) / k).floor();
        for (i, mut widget) in row.into_iter().enumerate() {
            widget.rect = [left + i as f32 * (w + GAP), y, w, BUTTON_H];
            out.push(widget);
        }
        y += BUTTON_H + gap;
    }
    out
}

/// Java's options list: 150-pixel widgets in two columns on 25-pixel rows,
/// scrolling between the header and the footer when they don't all fit.
/// Widgets are given with any rect; the list places them.
pub(in crate::app) struct OptionList<Id> {
    widgets: Vec<Widget<Id>>,
    left: f32,
    y: f32,
}

impl<Id> OptionList<Id> {
    pub fn new((sw, _): Size) -> Self {
        Self { widgets: Vec::new(), left: ((sw - LIST_W) / 2.0).floor(), y: HEADER + 4.0 }
    }

    /// Where the list shows: the scroll bar goes at its right edge.
    pub fn rect((sw, sh): Size) -> Rect {
        let left = ((sw - LIST_W) / 2.0).floor();
        [left - 4.0, HEADER, LIST_W + 18.0, (sh - HEADER - FOOTER).max(ROW)]
    }

    fn place(&mut self, mut widget: Widget<Id>, rect: Rect) {
        widget.rect = rect;
        self.widgets.push(widget.scrolling());
    }

    /// Two widgets side by side, or one in the left column.
    pub fn pair(&mut self, left: Widget<Id>, right: Option<Widget<Id>>) {
        self.place(left, [self.left, self.y, OPTION_W, BUTTON_H]);
        if let Some(right) = right {
            self.place(right, [self.left + OPTION_W + COLUMN_GAP, self.y, OPTION_W, BUTTON_H]);
        }
        self.y += ROW;
    }

    /// Widgets two to a row, in order.
    pub fn pairs(&mut self, widgets: Vec<Widget<Id>>) {
        let mut it = widgets.into_iter();
        while let Some(left) = it.next() {
            self.pair(left, it.next());
        }
    }

    /// One widget across both columns.
    pub fn wide(&mut self, widget: Widget<Id>) {
        self.place(widget, [self.left, self.y, LIST_W, BUTTON_H]);
        self.y += ROW;
    }

    /// The widgets, then Done in the footer.
    pub fn done(mut self, done: Widget<Id>, (sw, sh): Size) -> Vec<Widget<Id>> {
        let mut done = done;
        done.rect =
            [((sw - BUTTON_W) / 2.0).floor(), (sh - FOOTER + (FOOTER - BUTTON_H) / 2.0).floor(), BUTTON_W, BUTTON_H];
        self.widgets.push(done);
        self.widgets
    }
}
