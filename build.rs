/// Embed Windows icon/version resources only when building the desktop client.
fn main() {
    embed_assets();
    embed_mining_tags();
    println!("cargo:rerun-if-env-changed=VOXELCRAFT_BUILD_REVISION");
    if let Ok(revision) = std::env::var("VOXELCRAFT_BUILD_REVISION") {
        println!("cargo:rustc-env=VOXELCRAFT_BUILD_REVISION={revision}");
    }
    println!("cargo:rerun-if-changed=packaging/icons/VoxelCraft.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows")
        && std::env::var_os("CARGO_FEATURE_CLIENT").is_some()
    {
        winresource::WindowsResource::new()
            .set_icon("packaging/icons/VoxelCraft.ico")
            .set("FileDescription", "VoxelCraft")
            .set("ProductName", "VoxelCraft")
            .set("OriginalFilename", "voxelcraft.exe")
            .compile()
            .expect("compile Windows icon and version resources");
    }
}

/// Emit static reference names at build time; gameplay never parses fixture JSON.
fn embed_mining_tags() {
    use std::{fs, path::PathBuf};
    const PATH: &str = "tests/fixtures/java/26.3/mining.json";
    println!("cargo:rerun-if-changed={PATH}");
    let fixture: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(PATH).expect("read reference mining tags"))
            .expect("parse reference mining tags");
    let mut names: Vec<String> = fixture["ice_support_blocks"]
        .as_array()
        .expect("ice support tag")
        .iter()
        .map(|v| v.as_str().expect("reference block ID").replace('_', " "))
        .collect();
    names.sort_unstable();
    assert!(names.windows(2).all(|pair| pair[0] != pair[1]), "duplicate reference mining tag name");
    let mut generated = String::from("const ICE_SUPPORT_NAMES: &[&str] = &[\n");
    for name in names {
        generated.push_str(&format!("{name:?},\n"));
    }
    generated.push_str("];\n");
    fs::write(PathBuf::from(std::env::var("OUT_DIR").expect("build output")).join("mining_tags.rs"), generated)
        .expect("write static mining tags");
}

/// Generate the embedded resource catalogue from its authoritative manifest.
/// A missing resource fails every platform's build with its stable asset key.
fn embed_assets() {
    use std::{fs, path::PathBuf};
    println!("cargo:rerun-if-changed=assets/manifest.json");
    let manifest: serde_json::Value =
        serde_json::from_str(&fs::read_to_string("assets/manifest.json").expect("read assets/manifest.json"))
            .expect("parse asset manifest");
    let mut generated = String::from("pub static RESOURCES: &[EmbeddedResource] = &[\n");
    let root = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("manifest directory"));
    let mut keys = std::collections::BTreeSet::new();
    for entry in manifest["resources"].as_array().expect("manifest resources array") {
        let key = entry["key"].as_str().expect("asset key");
        let path = entry["path"].as_str().expect("asset path");
        assert!(keys.insert(key), "duplicate asset key {key}");
        assert!(path.starts_with("assets/") && !path.split('/').any(|p| p == ".."), "asset {key}: invalid path {path}");
        assert!(entry["license"] == "MIT OR Apache-2.0", "asset {key}: review its license before inclusion");
        let full = root.join(path);
        assert!(full.is_file(), "asset {key}: missing resource {path}");
        println!("cargo:rerun-if-changed={path}");
        generated.push_str(&format!(
            "EmbeddedResource {{ key: {key:?}, bytes: include_bytes!({:?}) }},\n",
            full.to_str().expect("UTF-8 asset path")
        ));
    }
    generated.push_str("];\n");
    fs::write(PathBuf::from(std::env::var("OUT_DIR").expect("build output")).join("assets.rs"), generated)
        .expect("write asset catalogue");
}
