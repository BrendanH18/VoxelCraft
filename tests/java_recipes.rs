//! Every Java 26.3 recipe (crafting, cooking, stonecutting, smithing) against
//! VoxelCraft's recipe data, both ways. Reference recipes whose items aren't
//! registered are listed, not failed; intentional differences are in the
//! allow-lists in `java_recipes/compare.rs`. `examples/recipe_audit.rs`
//! prints the same report as JSON.

#[path = "java_recipes/compare.rs"]
mod compare;

use compare::{Reference, Section};

fn check(name: &str, s: &Section, at_least: usize) {
    eprintln!(
        "{name}: compared {}, matched {}, allow-listed {}, unregistered {}",
        s.compared,
        s.matched,
        s.allowed.len(),
        s.unregistered.len()
    );
    assert!(s.compared >= at_least, "{name}: only {} reference recipes compared", s.compared);
    assert!(s.special.is_empty(), "{name}: reference recipes needing an allow-list decision: {:#?}", s.special);
    assert!(s.failures.is_empty(), "{name}: {} mismatches:\n{}", s.failures.len(), s.failures.join("\n"));
}

#[test]
fn names_resolve_to_distinct_items() {
    let r = Reference::load();
    let collisions = r.names.collisions();
    assert!(collisions.is_empty(), "Java names sharing one VoxelCraft item: {collisions:#?}");
}

#[test]
fn crafting_recipes_match_java() {
    check("crafting", &compare::compare_crafting(&Reference::load()), 700);
}

#[test]
fn cooking_recipes_match_java() {
    check("cooking", &compare::compare_cooking(&Reference::load()), 90);
}

#[test]
fn stonecutting_recipes_match_java() {
    check("stonecutting", &compare::compare_stonecutting(&Reference::load()), 200);
}

#[test]
fn smithing_recipes_match_java() {
    check("smithing", &compare::compare_smithing(&Reference::load()), 9);
}
