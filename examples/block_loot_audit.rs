//! Cold-path block loot audit: how many Java 26.3 block loot tables VoxelCraft's
//! registered states match, the allow-listed differences and the Java blocks
//! not registered yet, as JSON. `tests/java_block_loot.rs` runs the same audit
//! and fails on any unlisted difference.
#[path = "../tests/block_loot/mod.rs"]
mod block_loot;

use serde_json::json;

fn main() {
    let audit = block_loot::audit();
    let fixture: serde_json::Value = serde_json::from_str(block_loot::FIXTURE).unwrap();
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "reference": fixture["reference"], "server_sha1": fixture["server_sha1"],
            "reference_blocks": fixture["blocks"].as_object().unwrap().len(),
            "registered_states": audit.states,
            "compared_blocks": audit.compared, "matched_blocks": audit.matched,
            "state_scenario_checks": audit.checks, "scenarios_without_a_harvesting_tool": audit.skipped,
            "allowed": audit.allowed.iter().map(|(b, reason)| json!({"block": b, "reason": reason})).collect::<Vec<_>>(),
            "failing": audit.failing, "failures": audit.failures,
            "voxelcraft_only_states": audit.voxelcraft_only,
            "voxelcraft_only_allowed": block_loot::VOXELCRAFT_ONLY.iter().map(|(n, reason)| json!({"name": n, "reason": reason})).collect::<Vec<_>>(),
            "unregistered_count": audit.unregistered.len(), "unregistered": audit.unregistered,
            "scope": "Mining drops per state and tool scenario. Explosion loot (survives_explosion, explosion_decay) is not compared.",
        }))
        .unwrap()
    );
}
