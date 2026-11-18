//! Java 26.3 books (v0.15 Track B): the book and quill (`writable_book`,
//! item 1557) and the written book (item 1558), their editing and signing
//! (`ServerGamePacketListenerImpl.handleEditBook`, `BookEditScreen`,
//! `BookSignScreen`), copying (`BookCloningRecipe`) and what a lectern
//! reads from them (`LecternBlockEntity`). Pages are the stack's
//! [`Payload::WritableBook`] or [`Payload::WrittenBook`].

use crate::inventory::Stack;
use crate::inventory::components::{
    Payload, WRITABLE_MAX_PAGES, WRITABLE_PAGE_LENGTH, WRITTEN_MAX_GENERATION, WRITTEN_TITLE_MAX_LENGTH, WrittenBook,
};
use crate::item::{Item, ItemInfo, ItemKind, Sprite};

pub const WRITABLE_BOOK: Item = Item(1557);
pub const WRITTEN_BOOK: Item = Item(1558);

/// `Items.WRITTEN_BOOK`'s `stacksTo(16)`; the book and quill stacks to one.
pub const WRITTEN_STACK: u8 = 16;
/// `BookSignScreen`'s title box: 15 characters.
pub const SIGN_TITLE_LENGTH: usize = 15;
/// `BookEditScreen`: 126 / 9 lines of 114 pixels per page.
pub const PAGE_LINES: usize = 14;
pub const PAGE_WIDTH: f32 = 114.0;
/// `BookCloningRecipe.DEFAULT_BOOK_GENERATION_RANGES`: originals and
/// copies of originals can be copied.
pub const COPYABLE_GENERATIONS: std::ops::RangeInclusive<u8> = 0..=1;

pub(crate) fn item_info(item: Item) -> Option<ItemInfo> {
    match item {
        WRITABLE_BOOK => Some(ItemInfo {
            name: "book and quill",
            kind: ItemKind::Material,
            max_stack: 1,
            sprite: Sprite::WritableBook,
        }),
        WRITTEN_BOOK => Some(ItemInfo {
            name: "written book",
            kind: ItemKind::Material,
            max_stack: WRITTEN_STACK,
            sprite: Sprite::WrittenBook,
        }),
        _ => None,
    }
}

/// `#lectern_books`.
pub fn lectern_book(item: Item) -> bool {
    matches!(item, WRITTEN_BOOK | WRITABLE_BOOK)
}

/// `WrittenBookContent.GENERATION` names (`book.generation.N`).
pub const GENERATIONS: [&str; 4] = ["Original", "Copy of original", "Copy of a copy", "Tattered"];

/// A written book's content.
pub fn written(stack: &Stack) -> Option<&'static WrittenBook> {
    match stack.payload.map(|p| p.get()) {
        Some(Payload::WrittenBook(b)) if stack.item == WRITTEN_BOOK => Some(b),
        _ => None,
    }
}

/// A book and quill's pages (none for a blank one).
pub fn writable_pages(stack: &Stack) -> &'static [String] {
    match stack.payload.map(|p| p.get()) {
        Some(Payload::WritableBook(pages)) if stack.item == WRITABLE_BOOK => pages,
        _ => &[],
    }
}

/// The pages a reader sees: a written book's, or a book and quill's.
pub fn pages(stack: &Stack) -> Vec<String> {
    match written(stack) {
        Some(b) => b.pages.clone(),
        None => writable_pages(stack).to_vec(),
    }
}

/// `LecternBlockEntity.getPageCount`.
pub fn page_count(stack: &Stack) -> usize {
    written(stack).map_or_else(|| writable_pages(stack).len(), |b| b.pages.len())
}

/// Cuts text to Java's `String.length` limit (UTF-16 units).
fn cut(text: &str, max: usize) -> String {
    let mut units = 0;
    text.chars()
        .take_while(|c| {
            units += c.len_utf16();
            units <= max
        })
        .collect()
}

/// `handleEditBook` without a title: the book and quill's new pages, as
/// the packet's codec bounds them (100 pages of 1024 characters).
pub fn edit(stack: Stack, pages: &[String]) -> Option<Stack> {
    if stack.item != WRITABLE_BOOK {
        return None;
    }
    let pages: Vec<String> = pages.iter().take(WRITABLE_MAX_PAGES).map(|p| cut(p, WRITABLE_PAGE_LENGTH)).collect();
    Some(Stack { payload: Payload::WritableBook(pages).intern(), ..stack })
}

/// `signBook`: the book and quill becomes a written book (its other
/// components kept) titled `title` (trimmed, as `BookSignScreen` sends it)
/// by `author`, generation 0. `None` for a blank title, which the sign
/// screen won't send.
pub fn sign(stack: Stack, pages: &[String], title: &str, author: &str) -> Option<Stack> {
    if stack.item != WRITABLE_BOOK {
        return None;
    }
    let title = title.trim();
    if title.is_empty() {
        return None;
    }
    let book = WrittenBook {
        title: cut(title, WRITTEN_TITLE_MAX_LENGTH),
        author: author.to_string(),
        generation: 0,
        pages: pages.iter().take(WRITABLE_MAX_PAGES).map(|p| cut(p, WRITABLE_PAGE_LENGTH)).collect(),
        resolved: true,
    };
    Some(Stack { item: WRITTEN_BOOK, payload: Payload::WrittenBook(book).intern(), ..stack })
}

/// `BookCloningRecipe`: one written book (an original or a copy of one)
/// and one or more books and quill make that many copies a generation
/// later. `None` when the grid isn't that recipe.
pub fn clone_result(occupied: &[Stack]) -> Option<Stack> {
    if occupied.len() < 2 {
        return None;
    }
    let mut source = None;
    let mut copies = 0u8;
    for s in occupied {
        if s.item == WRITTEN_BOOK {
            let b = written(s)?;
            if !COPYABLE_GENERATIONS.contains(&b.generation) || source.is_some() {
                return None;
            }
            source = Some((*s, b));
        } else if s.item == WRITABLE_BOOK {
            copies += 1;
        } else {
            return None;
        }
    }
    let (stack, book) = source?;
    if copies == 0 {
        return None;
    }
    let copy = WrittenBook { generation: (book.generation + 1).min(WRITTEN_MAX_GENERATION), ..book.clone() };
    Some(Stack { count: copies, payload: Payload::WrittenBook(copy).intern(), ..stack })
}

/// `BookCloningRecipe.getRemainingItems`: the original written book stays.
pub fn clone_remainder(occupied: &[Stack], stack: &Stack) -> Option<Stack> {
    (clone_result(occupied).is_some() && stack.item == WRITTEN_BOOK).then_some(Stack { count: 1, ..*stack })
}

/// `LecternBlockEntity.getRedstoneSignal`: the comparator reads how far
/// through the book the open page is, 1..=15.
pub fn lectern_signal(page: usize, pages: usize) -> u8 {
    let progress = if pages > 1 { page as f32 / (pages as f32 - 1.0) } else { 1.0 };
    (progress * 14.0).floor() as u8 + 1
}

/// `LecternBlockEntity.setPage`'s clamp (`Mth.clamp(page, 0, count - 1)`).
pub fn clamp_page(page: i64, pages: usize) -> usize {
    page.max(0).min(pages as i64 - 1).max(0) as usize
}

/// Splits a page into the lines the book screens draw: Java breaks at
/// newlines and wraps words to the page's 114 pixels (`StringSplitter`).
pub fn wrap(page: &str, width: f32, text_width: impl Fn(&str) -> f32) -> Vec<String> {
    let mut lines = Vec::new();
    for paragraph in page.split('\n') {
        let mut line = String::new();
        for word in paragraph.split_inclusive(' ') {
            let candidate = format!("{line}{word}");
            if text_width(candidate.trim_end()) <= width || line.is_empty() {
                line = candidate;
                // A single word longer than the page breaks by character.
                while text_width(line.trim_end()) > width && line.chars().count() > 1 {
                    let mut head = String::new();
                    for c in line.chars() {
                        if text_width(&format!("{head}{c}")) > width {
                            break;
                        }
                        head.push(c);
                    }
                    let rest = line[head.len()..].to_string();
                    lines.push(head);
                    line = rest;
                }
            } else {
                lines.push(line.trim_end().to_string());
                line = word.to_string();
            }
        }
        lines.push(line.trim_end().to_string());
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    fn book(pages: &[&str]) -> Stack {
        edit(Stack::new(WRITABLE_BOOK, 1), &pages.iter().map(|p| p.to_string()).collect::<Vec<_>>()).unwrap()
    }

    #[test]
    fn signing_makes_an_original_titled_by_its_author() {
        let quill = book(&["Hello", "World"]);
        let signed = sign(quill, writable_pages(&quill), "  Diary ", "Alex").unwrap();
        let b = written(&signed).unwrap();
        assert_eq!((b.title.as_str(), b.author.as_str(), b.generation), ("Diary", "Alex", 0));
        assert_eq!(b.pages, ["Hello", "World"]);
        assert!(sign(quill, &[], "   ", "Alex").is_none());
        assert!(sign(signed, &[], "x", "Alex").is_none(), "only a book and quill signs");
    }

    #[test]
    fn copies_go_up_one_generation_and_tattered_books_refuse() {
        let original = sign(book(&["a"]), &["a".into()], "T", "A").unwrap();
        let grid = [original, Stack::new(WRITABLE_BOOK, 1), Stack::new(WRITABLE_BOOK, 1)];
        let copy = clone_result(&grid).unwrap();
        assert_eq!(copy.count, 2);
        assert_eq!(written(&copy).unwrap().generation, 1);
        let copy_of_copy = clone_result(&[Stack { count: 1, ..copy }, Stack::new(WRITABLE_BOOK, 1)]).unwrap();
        assert_eq!(written(&copy_of_copy).unwrap().generation, 2);
        assert!(clone_result(&[copy_of_copy, Stack::new(WRITABLE_BOOK, 1)]).is_none());
        assert!(clone_result(&[original]).is_none());
        assert_eq!(clone_remainder(&grid, &original), Some(original));
    }

    #[test]
    fn lectern_signal_follows_page_progress() {
        assert_eq!(lectern_signal(0, 1), 15);
        assert_eq!(lectern_signal(0, 0), 15);
        assert_eq!(lectern_signal(0, 3), 1);
        assert_eq!(lectern_signal(1, 3), 8);
        assert_eq!(lectern_signal(2, 3), 15);
        assert_eq!(clamp_page(5, 3), 2);
        assert_eq!(clamp_page(-1, 3), 0);
    }

    #[test]
    fn edits_respect_the_packet_limits() {
        let long = "x".repeat(2000);
        let pages: Vec<String> = (0..150).map(|_| long.clone()).collect();
        let b = edit(Stack::new(WRITABLE_BOOK, 1), &pages).unwrap();
        assert_eq!(writable_pages(&b).len(), WRITABLE_MAX_PAGES);
        assert!(writable_pages(&b).iter().all(|p| p.len() == WRITABLE_PAGE_LENGTH));
    }

    #[test]
    fn wrapping_breaks_lines_at_words_and_newlines() {
        let width = |s: &str| s.len() as f32 * 6.0;
        assert_eq!(wrap("aaa bbb\nccc", 40.0, width), ["aaa", "bbb", "ccc"]);
        assert_eq!(wrap("abcdefghij", 30.0, width), ["abcde", "fghij"]);
        assert_eq!(wrap("", 30.0, width), [""]);
    }
}
