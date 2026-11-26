//! make-fixture.sh's probe, copied into a build's `examples/` only while the fixture run builds
//! it: prints each block, item or mob name with the id that build gives it, or `-`.
use voxelcraft::entity::MobKind;
use voxelcraft::item::Item;
use voxelcraft::world::block::Block;
fn main() {
    let mut args = std::env::args().skip(1);
    let kind = args.next().expect("block|item|mob");
    for name in args {
        let id = match kind.as_str() {
            "block" => Block::from_name(&name).map(|b| b.0.to_string()),
            "item" => Item::from_name(&name).map(|i| i.0.to_string()),
            _ => MobKind::from_name(&name).map(|_| "1".to_string()),
        };
        println!("{name} {}", id.unwrap_or_else(|| "-".into()));
    }
}
