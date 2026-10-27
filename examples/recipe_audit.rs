//! Cold-path recipe audit: the `tests/java_recipes.rs` comparison as JSON, with
//! counts, every reference recipe whose items VoxelCraft doesn't register
//! (and the names it lacks), the allow-listed differences and any mismatches.
#[path = "../tests/java_recipes/compare.rs"]
mod compare;

use serde_json::json;

fn section(s: &compare::Section) -> serde_json::Value {
    json!({
        "compared": s.compared, "matched": s.matched,
        "allow_listed": s.allowed.iter().map(|(k, why)| json!({"recipe": k, "reason": why})).collect::<Vec<_>>(),
        "needs_decision": s.special, "mismatches": s.failures,
        "unregistered_count": s.unregistered.len(), "unregistered": s.unregistered,
    })
}

fn main() {
    let r = compare::Reference::load();
    let report = json!({
        "reference": r.fixture["reference"], "server_sha1": r.fixture["server_sha1"],
        "reference_counts": r.fixture["counts"],
        "renamed": r.names.renamed(),
        "name_collisions": r.names.collisions().iter().map(|(_, names)| names).collect::<Vec<_>>(),
        "crafting": section(&compare::compare_crafting(&r)),
        "cooking": section(&compare::compare_cooking(&r)),
        "stonecutting": section(&compare::compare_stonecutting(&r)),
        "smithing": section(&compare::compare_smithing(&r)),
        "scope": "Brewing (recipe/brewing) is not compared. Unregistered recipes are missing content, not parity.",
    });
    println!("{}", serde_json::to_string_pretty(&report).unwrap());
}
