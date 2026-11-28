//! Java 26.3 bundles (`Items.BUNDLE`, `Items.DYED_BUNDLE`, `BundleItem`,
//! `BundleContents`): the plain bundle and its sixteen dyed forms, item ids
//! 1568..=1584 (the plain bundle, then `DyeColor` order; v0.17 build-5000
//! reservation).
//!
//! A bundle holds an ordered list of stacks (the stack's
//! [`Payload::Bundle`]) whose total weight may not exceed one. Index 0 is the
//! front: Java's `Mutable.tryInsert` adds (or moves a merged stack) there and
//! `removeOne` takes it from there. Weights are exact fractions, as Java's
//! `org.apache.commons.lang3.math.Fraction`. Checked against
//! `tests/fixtures/java/26.3/bundles.json` in `tests/java_bundles.rs`.

use crate::color::DyeColor;
use crate::inventory::Stack;
use crate::inventory::components::Payload;
use crate::item::{Item, ItemInfo, ItemKind, Sprite};
use std::cmp::Ordering;

/// The plain bundle; the sixteen dyed bundles follow in `DyeColor` order.
pub const ITEM_FIRST: u16 = 1568;
/// One past the black bundle.
pub const ITEM_END: u16 = ITEM_FIRST + 17;
const _: () = assert!(ITEM_END == 1585);

/// `Items.BUNDLE`'s and `Items.DYED_BUNDLE`'s `stacksTo(1)`.
pub const MAX_STACK: u8 = 1;
/// The plain bundle.
pub const BUNDLE: Item = Item(ITEM_FIRST);

/// `BundleContents.BUNDLE_IN_BUNDLE_WEIGHT`, added to a bundle's own
/// contents when it sits in another bundle.
pub const BUNDLE_IN_BUNDLE_WEIGHT: Fraction = Fraction::new(1, 16);
/// `BundleContents.BEEHIVE_WEIGHT`: a hive or nest item holding bees.
pub const BEEHIVE_WEIGHT: Fraction = Fraction::ONE;

const NAMES: [&str; 17] = [
    "bundle",
    "white bundle",
    "orange bundle",
    "magenta bundle",
    "light blue bundle",
    "yellow bundle",
    "lime bundle",
    "pink bundle",
    "gray bundle",
    "light gray bundle",
    "cyan bundle",
    "purple bundle",
    "blue bundle",
    "brown bundle",
    "green bundle",
    "red bundle",
    "black bundle",
];

/// The bundle of colour `c`.
pub const fn dyed(c: DyeColor) -> Item {
    Item(ITEM_FIRST + 1 + c as u16)
}

/// Whether `item` is one of the seventeen bundles (Java's `#bundles`).
pub fn is_bundle(item: Item) -> bool {
    (ITEM_FIRST..ITEM_END).contains(&item.0)
}

/// A bundle's colour: `Some(None)` for the plain bundle, `None` for
/// anything that isn't a bundle.
pub fn colour(item: Item) -> Option<Option<DyeColor>> {
    let i = item.0.checked_sub(ITEM_FIRST).filter(|&i| i < 17)?;
    Some(i.checked_sub(1).map(|c| DyeColor::ALL[c as usize]))
}

/// Every bundle in the `bundles` tag's order: plain, then `DyeColor` order.
pub fn all() -> impl Iterator<Item = Item> {
    (ITEM_FIRST..ITEM_END).map(Item)
}

/// The plain bundle's undyed leather.
const LEATHER: [u8; 3] = [150, 96, 58];

pub(crate) fn item_info(item: Item) -> Option<ItemInfo> {
    let c = colour(item)?;
    Some(ItemInfo {
        name: NAMES[(item.0 - ITEM_FIRST) as usize],
        kind: ItemKind::Material,
        max_stack: MAX_STACK,
        sprite: Sprite::Bundle(c.map_or(LEATHER, DyeColor::rgb)),
    })
}

/// An exact non-negative fraction in lowest terms (Java's `Fraction`, as the
/// bundle code uses it: weights never go below zero).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Fraction {
    num: u64,
    den: u64,
}

const fn gcd(mut a: u64, mut b: u64) -> u64 {
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a
}

impl Fraction {
    pub const ZERO: Self = Self { num: 0, den: 1 };
    pub const ONE: Self = Self { num: 1, den: 1 };

    /// `num / den` in lowest terms. `den` must not be zero.
    pub const fn new(num: u64, den: u64) -> Self {
        let g = gcd(num, den);
        let g = if g == 0 { 1 } else { g };
        Self { num: num / g, den: den / g }
    }

    pub const fn numerator(self) -> u64 {
        self.num
    }

    pub const fn denominator(self) -> u64 {
        self.den
    }

    /// `Fraction.add`; `None` where Java's would overflow.
    pub fn checked_add(self, o: Self) -> Option<Self> {
        let g = gcd(self.den, o.den);
        let den = (self.den / g).checked_mul(o.den)?;
        let num = self.num.checked_mul(o.den / g)?.checked_add(o.num.checked_mul(self.den / g)?)?;
        Some(Self::new(num, den))
    }

    /// `self - o`, floored at zero.
    pub fn saturating_sub(self, o: Self) -> Self {
        if self <= o {
            return Self::ZERO;
        }
        let den = self.den as u128 * o.den as u128;
        let num = self.num as u128 * o.den as u128 - o.num as u128 * self.den as u128;
        let g = gcd_u128(num, den);
        Self { num: (num / g) as u64, den: (den / g) as u64 }
    }

    /// `multiplyBy(Fraction.getFraction(n, 1))`.
    pub fn checked_mul(self, n: u64) -> Option<Self> {
        let g = gcd(n, self.den).max(1);
        Some(Self::new(self.num.checked_mul(n / g)?, self.den / g))
    }

    /// `divideBy(o).intValue()`, for `o > 0`.
    pub fn div_floor(self, o: Self) -> u64 {
        ((self.num as u128 * o.den as u128) / (self.den as u128 * o.num as u128)) as u64
    }

    /// `floatValue()`.
    pub fn as_f32(self) -> f32 {
        self.num as f32 / self.den as f32
    }
}

fn gcd_u128(mut a: u128, mut b: u128) -> u128 {
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a.max(1)
}

impl PartialOrd for Fraction {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Fraction {
    fn cmp(&self, other: &Self) -> Ordering {
        (self.num as u128 * other.den as u128).cmp(&(other.num as u128 * self.den as u128))
    }
}

/// `BundleContents.canItemBeInBundle`: anything but a shulker box
/// (`BlockItem.canFitInsideContainerItems`).
pub fn can_fit(stack: &Stack) -> bool {
    crate::content::shulker_box::can_fit_inside_container(stack)
}

/// The stacks a stack holds as a bundle (Java reads the component, which
/// every bundle carries, empty by default).
fn held(stack: &Stack) -> Option<&'static [Stack]> {
    match stack.payload() {
        Some(Payload::Bundle(items)) => Some(items),
        _ => is_bundle(stack.item).then_some(&[]),
    }
}

/// `BundleContents.getWeight`: one item's weight. A bundle weighs 1/16 plus
/// its contents, a hive item holding bees weighs one, anything else one
/// over its maximum stack size. `None` where Java's is an error (overflow).
pub fn weight(stack: &Stack) -> Option<Fraction> {
    if let Some(items) = held(stack) {
        return contents_weight(items)?.checked_add(BUNDLE_IN_BUNDLE_WEIGHT);
    }
    if stack.hive.is_some_and(|h| h.bees.iter().any(Option::is_some)) {
        return Some(BEEHIVE_WEIGHT);
    }
    Some(Fraction::new(1, stack.max().max(1) as u64))
}

/// `BundleContents.computeContentWeight`: the summed weight of every item.
pub fn contents_weight(items: &[Stack]) -> Option<Fraction> {
    items.iter().try_fold(Fraction::ZERO, |sum, s| sum.checked_add(weight(s)?.checked_mul(s.count as u64)?))
}

/// A bundle's fullness (`BundleItem.getWeightSafe`): its contents' weight,
/// or one when that can't be computed.
pub fn fullness(stack: &Stack) -> Fraction {
    contents_weight(stack.bundle_contents()).unwrap_or(Fraction::ONE)
}

/// `BundleItem.isBarVisible` and `getBarWidth`: the fullness bar's width in
/// pixels (1 to 13), `None` for an empty bundle or a non-bundle.
pub fn bar_width(stack: &Stack) -> Option<u32> {
    if !is_bundle(stack.item) {
        return None;
    }
    let w = fullness(stack);
    // `Mth.mulAndTruncate(weight, 12)`: numerator * 12 / denominator.
    (w > Fraction::ZERO).then(|| (1 + (w.numerator() as u128 * 12 / w.denominator() as u128) as u32).min(13))
}

/// `BundleItem.getBarColor`: red once full, else blue (ARGB from Java's
/// `colorFromFloat(1, 1, .33, .33)` and `(1, .44, .53, 1)`).
pub fn bar_rgb(stack: &Stack) -> [f32; 3] {
    if fullness(stack) >= Fraction::ONE { [1.0, 0.33, 0.33] } else { [0.44, 0.53, 1.0] }
}

/// `Mutable.getMaxAmountToAdd`: how many items of `item_weight` still fit.
fn max_amount_to_add(contents: &[Stack], item_weight: Fraction) -> Option<u64> {
    let remaining = Fraction::ONE.saturating_sub(contents_weight(contents)?);
    Some(remaining.div_floor(item_weight))
}

/// `Mutable.tryInsert`: moves as many of `offered` as fit into `contents`,
/// merging with an equal stack (moved to the front) or adding a new front
/// stack. Returns how many moved; `offered` keeps the rest.
pub fn try_insert(contents: &mut Vec<Stack>, offered: &mut Option<Stack>) -> u8 {
    let Some(stack) = *offered else { return 0 };
    if !can_fit(&stack) {
        return 0;
    }
    let Some(room) = weight(&stack).and_then(|w| max_amount_to_add(contents, w)) else {
        return 0;
    };
    let amount = (stack.count as u64).min(room) as u8;
    if amount == 0 {
        return 0;
    }
    // `findStackIndex`: only stackable items merge.
    let merge = if stack.max() > 1 { contents.iter().position(|s| s.stacks_with(&stack)) } else { None };
    let moved = match merge {
        Some(i) => {
            let s = contents.remove(i);
            Stack { count: s.count + amount, ..s }
        }
        None => Stack { count: amount, ..stack },
    };
    contents.insert(0, moved);
    *offered = (stack.count > amount).then_some(Stack { count: stack.count - amount, ..stack });
    amount
}

/// `Mutable.removeOne` with nothing selected: takes the front stack.
pub fn remove_one(contents: &mut Vec<Stack>) -> Option<Stack> {
    remove_selected(contents, None)
}

/// `Mutable.removeOne`: takes the selected stack (`selected`, an index from
/// the front), or the front one when nothing in range is selected.
pub fn remove_selected(contents: &mut Vec<Stack>, selected: Option<usize>) -> Option<Stack> {
    if contents.is_empty() {
        return None;
    }
    let i = selected.filter(|&i| i < contents.len()).unwrap_or(0);
    Some(contents.remove(i))
}

/// `BundleItem.MAX_SHOWN_GRID_ITEMS`: the contents tooltip's twelve cells,
/// four across (`MAX_SHOWN_GRID_ITEMS_X`).
pub const SHOWN_MAX: usize = 12;
/// `MAX_SHOWN_GRID_ITEMS_X`.
pub const SHOWN_COLUMNS: usize = 4;

/// `BundleContents.getNumberOfItemsToShow` for a bundle of `stacks`
/// stacks: up to twelve (eleven when more don't fit, the twelfth cell
/// showing "+n"), trimmed so every row but none is full.
pub fn shown_count(stacks: usize) -> usize {
    let available = if stacks > SHOWN_MAX { SHOWN_MAX - 1 } else { SHOWN_MAX };
    let partial = stacks % SHOWN_COLUMNS;
    let empty = if partial == 0 { 0 } else { SHOWN_COLUMNS - partial };
    stacks.min(available - empty)
}

/// `ScrollWheelHandler.getNextScrollWheelSelection`: the selection after
/// one wheel step over a bundle showing `limit` stacks. A step down (`wheel`
/// negative) moves to the next stack, up to the previous one, both wrapping;
/// from no selection, down picks the front and up the last shown.
pub fn next_selection(wheel: f64, current: Option<usize>, limit: usize) -> Option<usize> {
    if limit == 0 {
        return None;
    }
    let mut i = current.map_or(-1, |c| c as i64) - wheel.signum() as i64;
    i = i.max(-1);
    let limit = limit as i64;
    while i < 0 {
        i += limit;
    }
    while i >= limit {
        i -= limit;
    }
    Some(i as usize)
}

/// What a bundle click did, for feedback (Java's insert, insert-fail and
/// remove-one sounds).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Click {
    /// Items went in (`BUNDLE_INSERT`).
    Inserted,
    /// Nothing fitted (`BUNDLE_INSERT_FAIL`).
    InsertFailed,
    /// A stack came out (`BUNDLE_REMOVE_ONE`).
    Removed,
    /// Handled with no change (an empty bundle emptied again).
    Nothing,
}

impl Click {
    /// The sound Java plays for this click, if any.
    pub fn cue(self) -> Option<Cue> {
        match self {
            Click::Inserted => Some(Cue::Insert),
            Click::InsertFailed => Some(Cue::InsertFail),
            Click::Removed => Some(Cue::RemoveOne),
            Click::Nothing => None,
        }
    }

    /// Whether the clicked bundle keeps its selected stack after this
    /// click: Java leaves the selection alone only when a stack went in
    /// (or failed to) from the cursor; taking one out or any other click
    /// on it clears it (`removeOne`, `toggleSelectedItem(-1)`).
    pub fn keeps_selection(self) -> bool {
        matches!(self, Click::Inserted | Click::InsertFailed)
    }
}

/// v1.0 gap track 6: the four bundle sounds (`SoundEvents.BUNDLE_*`).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Cue {
    /// `item.bundle.insert` ("Item packed"): volume 0.8, pitch 0.8-1.2.
    Insert,
    /// `item.bundle.insert_fail` ("Bundle full"): volume 1, pitch 1.
    InsertFail,
    /// `item.bundle.remove_one` ("Item unpacked"): volume 0.8, pitch 0.8-1.2.
    RemoveOne,
    /// `item.bundle.drop_contents` ("Bundle empties"): volume 0.8, pitch
    /// 0.8-1.2, at the player's block.
    DropContents,
}

impl Cue {
    pub const ALL: [Cue; 4] = [Cue::Insert, Cue::InsertFail, Cue::RemoveOne, Cue::DropContents];

    /// The Java sound event.
    pub fn event(self) -> &'static str {
        match self {
            Cue::Insert => "item.bundle.insert",
            Cue::InsertFail => "item.bundle.insert_fail",
            Cue::RemoveOne => "item.bundle.remove_one",
            Cue::DropContents => "item.bundle.drop_contents",
        }
    }

    /// Java's volume and pitch range (`BundleItem.play*Sound`).
    pub fn volume_pitch(self) -> (f32, (f32, f32)) {
        match self {
            Cue::InsertFail => (1.0, (1.0, 1.0)),
            _ => (0.8, (0.8, 1.2)),
        }
    }
}

thread_local! {
    /// The selected stack of the bundle a click lands on, and what the last
    /// bundle click did, for [`clicking`].
    static CLICK: std::cell::Cell<(Option<usize>, Option<Click>)> = const { std::cell::Cell::new((None, None)) };
}

/// Runs `f`, a container click, with `selected` as the clicked bundle's
/// selected stack (the one the hovered bundle's tooltip highlights), and
/// returns what a bundle override did during it. Every screen reaches the
/// override through [`crate::inventory::click_slot`], so the adapters wrap
/// their clicks here rather than threading the selection and the sound
/// through each screen's slot rules.
pub fn clicking<R>(selected: Option<usize>, f: impl FnOnce() -> R) -> (R, Option<Click>) {
    let outer = CLICK.replace((selected, None));
    let r = f();
    let (_, click) = CLICK.replace(outer);
    (r, click)
}

fn record(click: Click) {
    CLICK.set((None, Some(click)));
}

fn clicked_selection() -> Option<usize> {
    CLICK.get().0
}

fn set_contents(bundle: &mut Stack, contents: Vec<Stack>) {
    *bundle = bundle.with_payload(Payload::Bundle(contents));
}

/// Java's bundle overrides of a container click, run before the ordinary
/// click as `AbstractContainerMenu.tryItemClickBehaviourOverride` does:
/// first the held bundle's `overrideStackedOnOther`, then the clicked
/// bundle's `overrideOtherStackedOnMe`. `None` leaves the click to the
/// ordinary pick-up, place and swap rules.
///
/// - Holding a bundle, left click a stack: as much of it as fits goes in.
/// - Holding a bundle, right click an empty slot: the front stack comes out
///   into the slot.
/// - Holding a stack, left click a bundle: as much as fits goes in.
/// - Holding nothing, right click a bundle: the front stack comes out onto
///   the cursor.
pub fn click(slot: &mut Option<Stack>, cursor: &mut Option<Stack>, right: bool) -> Option<Click> {
    let result = held_or_clicked(slot, cursor, right);
    if let Some(click) = result {
        record(click);
    }
    result
}

fn held_or_clicked(slot: &mut Option<Stack>, cursor: &mut Option<Stack>, right: bool) -> Option<Click> {
    if let Some(mut bundle) = cursor.filter(|c| is_bundle(c.item)) {
        let mut contents = bundle.bundle_contents().to_vec();
        let result = match (*slot, right) {
            (Some(other), false) => {
                // `tryTransfer`: take what fits from the slot, then insert it.
                let room = weight(&other).and_then(|w| max_amount_to_add(&contents, w)).unwrap_or(0);
                let take = (other.count as u64).min(room) as u8;
                let mut taken = (take > 0 && can_fit(&other)).then_some(Stack { count: take, ..other });
                let moved = try_insert(&mut contents, &mut taken);
                // Whatever `tryInsert` refused stays in the slot.
                let left = other.count - moved;
                *slot = (left > 0).then_some(Stack { count: left, ..other });
                if moved > 0 { Click::Inserted } else { Click::InsertFailed }
            }
            (None, true) => match remove_one(&mut contents) {
                Some(out) => {
                    // `Slot.safeInsert` into the empty slot, the rest back in.
                    let fits = out.count.min(out.max());
                    *slot = Some(Stack { count: fits, ..out });
                    let mut rest = (out.count > fits).then_some(Stack { count: out.count - fits, ..out });
                    if rest.is_some() {
                        try_insert(&mut contents, &mut rest);
                        Click::Nothing
                    } else {
                        Click::Removed
                    }
                }
                None => Click::Nothing,
            },
            _ => return clicked_bundle(slot, cursor, right),
        };
        set_contents(&mut bundle, contents);
        *cursor = Some(bundle);
        return Some(result);
    }
    clicked_bundle(slot, cursor, right)
}

/// `BundleItem.overrideOtherStackedOnMe`, for a bundle in the clicked slot.
fn clicked_bundle(slot: &mut Option<Stack>, cursor: &mut Option<Stack>, right: bool) -> Option<Click> {
    let mut bundle = slot.filter(|s| is_bundle(s.item))?;
    let mut contents = bundle.bundle_contents().to_vec();
    let result = match (cursor.is_some(), right) {
        (true, false) => {
            if try_insert(&mut contents, cursor) > 0 {
                Click::Inserted
            } else {
                Click::InsertFailed
            }
        }
        (false, true) => match remove_selected(&mut contents, clicked_selection()) {
            Some(out) => {
                *cursor = Some(out);
                Click::Removed
            }
            None => Click::Nothing,
        },
        _ => return None,
    };
    set_contents(&mut bundle, contents);
    *slot = Some(bundle);
    Some(result)
}

/// `BundleItem.removeOneItemFromBundle`: takes the front stack out of
/// `bundle` (a held bundle has nothing selected), or `None` when it holds
/// nothing or isn't a bundle.
pub fn take_one(bundle: &mut Stack) -> Option<Stack> {
    if !is_bundle(bundle.item) {
        return None;
    }
    let mut contents = bundle.bundle_contents().to_vec();
    let out = remove_one(&mut contents)?;
    set_contents(bundle, contents);
    Some(out)
}

/// `Item.onDestroyed`: what an item lying on the ground spills when it
/// burns or is blown up (`ItemUtils.onContainerDestroyed`): a bundle's
/// stacks front first (`BundleItem.onDestroyed`), or a block item's
/// `container` contents in slot order, empty slots skipped
/// (`BlockItem.onDestroyed`, v1.0 gap track 6b: shulker boxes). Nothing for
/// any other item.
pub fn destroyed_contents(stack: &Stack) -> Vec<Stack> {
    if is_bundle(stack.item) {
        return stack.bundle_contents().to_vec();
    }
    match stack.payload() {
        Some(crate::inventory::components::Payload::Container(slots)) if stack.item.block().is_some() => {
            slots.iter().flatten().copied().collect()
        }
        _ => Vec::new(),
    }
}

/// The sixteen `crafting_transmute` recipes (`<colour>_bundle.json`): any
/// bundle and one dye make that colour's bundle with every component of the
/// input kept (`TransmuteRecipe.createWithOriginalComponents`). `None` when
/// the grid isn't one bundle and one dye; `Some(None)` when it is but Java's
/// `matches` refuses it: the result would equal the input (that colour
/// already).
pub fn transmute(occupied: &[Stack]) -> Option<Option<Stack>> {
    let [a, b] = occupied else { return None };
    let (bundle, dye) = if is_bundle(a.item) { (a, b) } else { (b, a) };
    if !is_bundle(bundle.item) {
        return None;
    }
    let c = dye.item.dye_color()?;
    let result = Stack { item: dyed(c), count: 1, ..*bundle };
    Some((!result.stacks_with(bundle)).then_some(result))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fractions_reduce_compare_and_floor_like_commons_lang() {
        assert_eq!(Fraction::new(4, 64), Fraction::new(1, 16));
        let sum = Fraction::new(1, 16).checked_add(Fraction::new(3, 64)).unwrap();
        assert_eq!((sum.numerator(), sum.denominator()), (7, 64));
        assert_eq!(Fraction::ONE.saturating_sub(sum), Fraction::new(57, 64));
        assert_eq!(Fraction::new(57, 64).div_floor(Fraction::new(1, 16)), 14);
        assert_eq!(Fraction::new(1, 16).saturating_sub(Fraction::ONE), Fraction::ZERO);
        assert!(Fraction::new(63, 64) < Fraction::ONE);
        assert_eq!(Fraction::new(1, 64).checked_mul(64), Some(Fraction::ONE));
    }

    #[test]
    fn the_tooltip_shows_whole_rows_and_the_wheel_wraps() {
        // `getNumberOfItemsToShow` for every stack count up to 64.
        let java = |n: usize| {
            let available = if n > 12 { 11 } else { 12 };
            let partial = n % 4;
            n.min(available - if partial == 0 { 0 } else { 4 - partial })
        };
        for n in 0..64 {
            assert_eq!(shown_count(n), java(n), "{n} stacks");
        }
        assert_eq!([1, 2, 5, 12, 13, 20].map(shown_count), [1, 2, 5, 12, 8, 11]);
        // `getNextScrollWheelSelection`: down from none picks the front, up
        // the last shown; both wrap.
        assert_eq!(next_selection(-1.0, None, 5), Some(0));
        assert_eq!(next_selection(1.0, None, 5), Some(4));
        assert_eq!(next_selection(-1.0, Some(4), 5), Some(0));
        assert_eq!(next_selection(1.0, Some(0), 5), Some(4));
        assert_eq!(next_selection(-1.0, Some(1), 5), Some(2));
        assert_eq!(next_selection(1.0, Some(3), 0), None);
    }

    #[test]
    fn a_selected_stack_comes_out_first_and_clicks_report_their_cue() {
        use crate::item::Item;
        let held = [Stack::new(Item::STICK, 3), Stack::new(Item::DIAMOND, 1), Stack::new(Item::APPLE, 2)];
        let mut contents = held.to_vec();
        assert_eq!(remove_selected(&mut contents, Some(1)), Some(held[1]));
        assert_eq!(contents, [held[0], held[2]]);
        assert_eq!(remove_selected(&mut contents, Some(7)), Some(held[0]), "out of range takes the front");
        // The clicked bundle's selection reaches `removeOne` through `clicking`.
        let bundle = Stack::new(BUNDLE, 1).with_payload(Payload::Bundle(held.to_vec()));
        let (mut slot, mut cursor) = (Some(bundle), None);
        let ((), click) = clicking(Some(2), || {
            crate::inventory::click_slot(&mut slot, &mut cursor, true);
        });
        assert_eq!(click, Some(Click::Removed));
        assert_eq!(click.and_then(Click::cue), Some(Cue::RemoveOne));
        assert_eq!(cursor, Some(held[2]));
        assert_eq!(slot.unwrap().bundle_contents(), &held[..2]);
        // Outside a scope nothing is selected: the front comes out.
        cursor = None;
        let ((), click) = clicking(None, || crate::inventory::click_slot(&mut slot, &mut cursor, true));
        assert_eq!((click, cursor), (Some(Click::Removed), Some(held[0])));
        // A full stack offered to a full bundle fails, with Java's sound.
        let full = Stack::new(BUNDLE, 1).with_payload(Payload::Bundle(vec![Stack::new(Item::STICK, 64)]));
        let (mut slot, mut cursor) = (Some(full), Some(Stack::new(Item::APPLE, 1)));
        let ((), click) = clicking(None, || crate::inventory::click_slot(&mut slot, &mut cursor, false));
        assert_eq!(click.and_then(Click::cue), Some(Cue::InsertFail));
        assert!(click.unwrap().keeps_selection());
        // An ordinary click reports nothing.
        let (mut slot, mut cursor) = (Some(Stack::new(Item::STICK, 1)), None);
        let ((), click) = clicking(None, || crate::inventory::click_slot(&mut slot, &mut cursor, false));
        assert_eq!(click, None);
    }

    #[test]
    fn using_and_destroying_a_bundle_takes_from_the_front() {
        use crate::item::Item;
        let held = [Stack::new(Item::STICK, 3), Stack::new(Item::DIAMOND, 1)];
        let mut bundle = Stack::new(dyed(DyeColor::Red), 1).with_payload(Payload::Bundle(held.to_vec()));
        assert_eq!(destroyed_contents(&bundle), held);
        assert_eq!(take_one(&mut bundle), Some(held[0]));
        assert_eq!(take_one(&mut bundle), Some(held[1]));
        assert_eq!(take_one(&mut bundle), None);
        assert_eq!(bundle, Stack::new(dyed(DyeColor::Red), 1), "an emptied bundle equals a fresh one");
        let mut stick = Stack::new(Item::STICK, 1);
        assert_eq!(take_one(&mut stick), None);
        assert!(destroyed_contents(&stick).is_empty());
    }
}
