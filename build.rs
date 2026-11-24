/// Embed Windows icon/version resources only when building the desktop client.
fn main() {
    embed_assets();
    embed_mining_tags();
    embed_recipe_book();
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
    let list = |value: &serde_json::Value| -> Vec<String> {
        let mut names: Vec<String> = value
            .as_array()
            .expect("reference tag")
            .iter()
            .map(|v| v.as_str().expect("reference block ID").replace('_', " "))
            .collect();
        names.sort_unstable();
        assert!(names.windows(2).all(|pair| pair[0] != pair[1]), "duplicate reference mining tag name");
        names
    };
    let rules = &fixture["tool_rules"];
    let mut generated = String::new();
    for (constant, value) in [
        ("ICE_SUPPORT_NAMES", &fixture["ice_support_blocks"]),
        ("MINEABLE_PICKAXE", &rules["mineable"]["pickaxe"]),
        ("MINEABLE_AXE", &rules["mineable"]["axe"]),
        ("MINEABLE_SHOVEL", &rules["mineable"]["shovel"]),
        ("MINEABLE_HOE", &rules["mineable"]["hoe"]),
        ("SWORD_EFFICIENT", &rules["sword_efficient"]),
        ("SWORD_INSTANTLY_MINES", &rules["sword_instantly_mines"]),
        ("SHEARS_EXTREME", &rules["shears"]["extreme"]),
        ("SHEARS_MAJOR", &rules["shears"]["major"]),
        ("SHEARS_MINOR", &rules["shears"]["minor"]),
        ("TOOL_RULE_BLOCKS", &rules["all_blocks"]),
    ] {
        generated.push_str(&format!("const {constant}: &[&str] = &[\n"));
        for name in list(value) {
            generated.push_str(&format!("{name:?},\n"));
        }
        generated.push_str("];\n");
    }
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

/// v0.18 Track 5: the recipe book's static table, from the Java 26.3
/// recipe shapes (`recipes.json`), book facts and unlock criteria
/// (`recipe_book.json`) and English item names (`registry_names.json`).
/// Item names stay Java IDs here; `crafting::book` resolves them once.
fn embed_recipe_book() {
    use serde_json::Value;
    use std::collections::BTreeMap;
    use std::{fs, path::PathBuf};
    const RECIPES: &str = "tests/fixtures/java/26.3/recipes.json";
    const BOOK: &str = "tests/fixtures/java/26.3/recipe_book.json";
    const NAMES: &str = "tests/fixtures/java/26.3/registry_names.json";
    const EMPTY: u16 = u16::MAX;

    #[derive(Default)]
    struct Tables {
        names: Vec<String>,
        name_ids: BTreeMap<String, u16>,
        alts: Vec<Vec<u16>>,
        alt_ids: BTreeMap<Vec<u16>, u16>,
    }
    impl Tables {
        fn name(&mut self, id: &str) -> u16 {
            if let Some(&i) = self.name_ids.get(id) {
                return i;
            }
            let i = u16::try_from(self.names.len()).expect("name table fits u16");
            self.names.push(id.to_string());
            self.name_ids.insert(id.to_string(), i);
            i
        }
        /// One ingredient (an ID, a `#tag` or a list of IDs) as an `ALTS` index.
        fn ingredient(&mut self, value: &Value, tags: &Value) -> u16 {
            let list: Vec<String> = match value {
                Value::String(s) if s.starts_with('#') => expand(tags, &s[1..]),
                Value::String(s) => vec![s.clone()],
                Value::Array(list) => list.iter().map(|v| v.as_str().expect("ingredient id").to_string()).collect(),
                other => panic!("ingredient {other}"),
            };
            let ids: Vec<u16> = list.iter().map(|n| self.name(n)).collect();
            if let Some(&i) = self.alt_ids.get(&ids) {
                return i;
            }
            let i = u16::try_from(self.alts.len()).expect("alternative table fits u16");
            self.alts.push(ids.clone());
            self.alt_ids.insert(ids, i);
            i
        }
    }
    fn expand(tags: &Value, tag: &str) -> Vec<String> {
        tags[tag]
            .as_array()
            .unwrap_or_else(|| panic!("tag #{tag}"))
            .iter()
            .map(|v| v.as_str().expect("tag entry").to_string())
            .collect()
    }

    for path in [RECIPES, BOOK, NAMES] {
        println!("cargo:rerun-if-changed={path}");
    }
    let read = |path: &str| -> Value {
        serde_json::from_str(&fs::read_to_string(path).unwrap_or_else(|_| panic!("read {path}")))
            .unwrap_or_else(|_| panic!("parse {path}"))
    };
    let (recipes, book, registry) = (read(RECIPES), read(BOOK), read(NAMES));
    let english = registry["items"].as_object().expect("registry items");
    let (recipe_tags, book_tags) = (&recipes["item_tags"], &book["item_tags"]);
    let mut t = Tables::default();
    let mut rows = String::new();
    for (id, fact) in book["learnable"].as_object().expect("learnable recipes") {
        let kind = fact["type"].as_str().expect("recipe type");
        let recipe = &recipes["recipes"][kind][id];
        assert!(recipe.is_object(), "{id}: no {kind} shape in recipes.json");
        let mut ing = |key: &str| t.ingredient(&recipe[key], recipe_tags);
        let (variant, width, height, slots): (&str, usize, usize, Vec<u16>) = match kind {
            "crafting_shaped" => {
                let pattern: Vec<&str> =
                    recipe["pattern"].as_array().unwrap().iter().map(|r| r.as_str().unwrap()).collect();
                let mut slots = Vec::new();
                for c in pattern.iter().flat_map(|row| row.chars()) {
                    slots.push(if c == ' ' { EMPTY } else { t.ingredient(&recipe["key"][c.to_string()], recipe_tags) });
                }
                ("Shaped", pattern[0].len(), pattern.len(), slots)
            }
            "crafting_shapeless" => {
                let mut slots = Vec::new();
                for v in recipe["ingredients"].as_array().unwrap() {
                    slots.push(t.ingredient(v, recipe_tags));
                }
                ("Shapeless", 0, 0, slots)
            }
            "crafting_transmute" => ("Transmute", 0, 0, vec![ing("input"), ing("material")]),
            "crafting_dye" => ("Dye", 0, 0, vec![ing("target"), ing("dye")]),
            "crafting_imbue" => {
                let (m, s) = (ing("material"), ing("source"));
                ("Shaped", 3, 3, vec![m, m, m, m, s, m, m, m, m])
            }
            "smelting" => ("Smelting", 0, 0, vec![ing("ingredient")]),
            "blasting" => ("Blasting", 0, 0, vec![ing("ingredient")]),
            "smoking" => ("Smoking", 0, 0, vec![ing("ingredient")]),
            "campfire_cooking" => ("Campfire", 0, 0, vec![ing("ingredient")]),
            "stonecutting" => ("Stonecutting", 0, 0, vec![ing("ingredient")]),
            "smithing_transform" | "smithing_trim" => {
                ("Smithing", 0, 0, vec![ing("template"), ing("base"), ing("addition")])
            }
            other => panic!("{id}: unexpected learnable type {other}"),
        };
        // An emptied transmute result keeps its input; a trim keeps the armor.
        let result = match recipe["result"].as_str() {
            Some(r) => t.name(r),
            None => t.alts[slots[usize::from(kind == "smithing_trim")] as usize][0],
        };
        let count = recipe.get("count").and_then(Value::as_u64).unwrap_or(1);
        let category: String =
            fact["category"].as_str().expect("category").split('_').map(|w| w[..1].to_uppercase() + &w[1..]).collect();
        let group = fact.get("group").and_then(Value::as_str).map_or("None".to_string(), |g| format!("Some({g:?})"));
        let notify = fact.get("notify").and_then(Value::as_bool).unwrap_or(true);
        let unlock = &book["unlocks"][id];
        let mut items: Vec<u16> = Vec::new();
        for item in unlock["items"].as_array().into_iter().flatten() {
            let item = item.as_str().unwrap();
            let expanded = match item.strip_prefix('#') {
                Some(tag) => expand(book_tags, tag),
                None => vec![item.to_string()],
            };
            for i in expanded {
                let i = t.name(&i);
                if !items.contains(&i) {
                    items.push(i);
                }
            }
        }
        let enter: Vec<&str> =
            unlock["enter_blocks"].as_array().into_iter().flatten().map(|b| b.as_str().unwrap()).collect();
        assert!(enter.iter().all(|b| *b == "water"), "{id}: enter_block other than water");
        rows.push_str(&format!(
            "Raw {{ id: {id:?}, kind: Kind::{variant}, category: Category::{category}, group: {group}, \
             notify: {notify}, width: {width}, height: {height}, slots: &{slots:?}, result: {result}, \
             count: {count}, unlock: RawUnlock {{ items: &{items:?}, tick: {}, occupied_min: {}, enter_water: {} }} }},\n",
            unlock["tick"] == true,
            unlock["occupied_min"].as_u64().unwrap_or(0),
            !enter.is_empty(),
        ));
    }
    let mut generated = String::from(
        "/// Java item IDs and English names the book names.\npub(super) static NAMES: &[(&str, &str)] = &[\n",
    );
    for n in &t.names {
        let display = english.get(n).and_then(Value::as_str).unwrap_or("");
        generated.push_str(&format!("({n:?}, {display:?}),\n"));
    }
    generated.push_str(
        "];\n/// Ingredient alternatives, as indices into `NAMES`.\npub(super) static ALTS: &[&[u16]] = &[\n",
    );
    for a in &t.alts {
        generated.push_str(&format!("&{a:?},\n"));
    }
    generated.push_str("];\n/// Every learnable Java 26.3 recipe, by ID.\npub(super) static LEARNABLE: &[Raw] = &[\n");
    generated.push_str(&rows);
    generated.push_str("];\n/// The special (never learnable) recipes.\npub(super) static SPECIAL: &[&str] = &[\n");
    for s in book["special"].as_array().expect("special recipes") {
        generated.push_str(&format!("{:?},\n", s.as_str().unwrap()));
    }
    generated.push_str("];\n");
    fs::write(PathBuf::from(std::env::var("OUT_DIR").expect("build output")).join("recipe_book.rs"), generated)
        .expect("write recipe book table");
}
