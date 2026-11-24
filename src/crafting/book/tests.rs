use super::*;
use crate::world::furnace::FurnaceKind;

#[test]
fn ids_are_sorted_unique_and_round_trip() {
    assert_eq!(RecipeId::count(), 1739);
    assert!(data::LEARNABLE.windows(2).all(|w| w[0].id < w[1].id));
    for id in RecipeId::all() {
        assert_eq!(RecipeId::from_name(id.name()), Some(id));
        assert_eq!(RecipeId::from_name(&format!("minecraft:{}", id.name())), Some(id));
        assert!(!is_special(id.name()));
    }
    assert!(is_special("minecraft:firework_rocket") && is_special("repair_item"));
    assert_eq!(RecipeId::from_name("firework_rocket"), None);
}

#[test]
fn every_shown_category_belongs_to_one_book_and_tab() {
    for r in recipes() {
        let book = r.book().unwrap_or_else(|| panic!("{} has no book", r.id.name()));
        assert!(book.tabs().iter().any(|t| t.category == Some(r.category)), "{}", r.id.name());
        assert_eq!(r.kind.is_crafting(), book == BookType::Crafting, "{}", r.id.name());
    }
    // Most of Java's crafting and furnace recipes are shown.
    let crafting = recipes().iter().filter(|r| r.kind.is_crafting()).count();
    let cooking = recipes().len() - crafting;
    assert!(crafting > 900 && cooking > 90, "{crafting} crafting, {cooking} cooking");
}

/// The book's recipe data and the game's crafting agree: each shown
/// recipe's ingredients, at their first alternatives, craft its result
/// through `Grid::result` (and cook in its furnace kind).
#[test]
fn shown_recipes_craft_their_result_through_the_shared_grid() {
    let mut failures = Vec::new();
    for r in recipes() {
        let got = if r.kind.is_crafting() {
            let size = if r.fits(2) { 2 } else { 3 };
            let mut grid = Grid::new(size);
            place::arrange(
                size,
                size,
                if r.kind == Kind::Shaped { r.width } else { size },
                if r.kind == Kind::Shaped { r.height } else { size },
                &r.slots,
                |ingredient, cell| {
                    if let Some(i) = ingredient {
                        grid.cells[cell] = Some(Stack::new(r.ingredients[i][0], 1));
                    }
                },
            );
            assert!(r.matches_grid(&grid), "{} doesn't match its own placement", r.id.name());
            grid.result().map(|s| s.item)
        } else {
            let kind = match r.kind {
                Kind::Smelting => FurnaceKind::Normal,
                Kind::Blasting => FurnaceKind::Blast,
                _ => FurnaceKind::Smoker,
            };
            kind.smelt(r.ingredients[0][0])
        };
        if got != Some(r.result.item) {
            failures.push(format!("{}: {:?} instead of {}", r.id.name(), got.map(Item::name), r.result.item.name()));
        }
    }
    assert!(failures.is_empty(), "{} recipes disagree:\n{}", failures.len(), failures.join("\n"));
}

#[test]
fn grid_recipe_finds_the_learnable_recipe_and_skips_specials() {
    let planks = |log: &str| {
        let mut grid = Grid::new(2);
        grid.cells[3] = Some(Stack::new(Item::from_name(log).unwrap(), 1));
        grid_recipe(&grid).map(RecipeId::name)
    };
    assert_eq!(planks("oak log"), Some("oak_planks"));
    assert_eq!(planks("birch log"), Some("birch_planks"));
    // Paper and one gunpowder is the learnable `firework_rocket_simple`;
    // a second gunpowder makes it the special `firework_rocket`.
    let mut rocket = Grid::new(2);
    rocket.cells[0] = Some(Stack::new(Item::PAPER, 1));
    rocket.cells[1] = Some(Stack::new(Item::GUNPOWDER, 1));
    assert_eq!(grid_recipe(&rocket).map(RecipeId::name), Some("firework_rocket_simple"));
    rocket.cells[2] = Some(Stack::new(Item::GUNPOWDER, 1));
    assert!(rocket.result().is_some());
    assert_eq!(grid_recipe(&rocket), None);
    assert_eq!(grid_recipe(&Grid::new(3)), None);
}

#[test]
fn arrange_centres_small_patterns_like_place_recipe_helper() {
    let cells = |gw, gh, rw, rh, n: usize| {
        let entries: Vec<Option<usize>> = (0..n).map(Some).collect();
        let mut out = Vec::new();
        place::arrange(gw, gh, rw, rh, &entries, |_, cell| out.push(cell));
        out
    };
    // A 1x2 stick pattern in a 3x3 grid sits in the middle column.
    assert_eq!(cells(3, 3, 1, 2, 2), vec![1, 4]);
    // A 2x2 pattern in 3x3 stays top left; in 2x2 it fills the grid.
    assert_eq!(cells(3, 3, 2, 2, 4), vec![0, 1, 3, 4]);
    assert_eq!(cells(2, 2, 2, 2, 4), vec![0, 1, 2, 3]);
    // A 1x1 pattern in 3x3 is centred both ways; shapeless fills in order.
    assert_eq!(cells(3, 3, 1, 1, 1), vec![4]);
    assert_eq!(cells(3, 3, 3, 3, 2), vec![0, 1]);
}
