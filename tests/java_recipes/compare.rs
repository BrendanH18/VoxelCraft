//! Compares VoxelCraft's recipe data with the normalized Java 26.3 recipes in
//! `tests/fixtures/java/26.3/recipes.json`. Shared by `tests/java_recipes.rs`
//! and `examples/recipe_audit.rs`.
//!
//! A reference recipe is compared when its result and at least one
//! alternative of every ingredient slot are registered; alternatives
//! VoxelCraft doesn't register are left out of both sides. Shaped patterns
//! compare equal to their mirror image, as both games match them mirrored.

#![allow(dead_code)]

use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use voxelcraft::{
    crafting::{self, Grid, Layout},
    inventory::Stack,
    item::Item,
    world::{block::Block, furnace},
};

pub const FIXTURE: &str = include_str!("../fixtures/java/26.3/recipes.json");
const REGISTRY: &str = include_str!("../fixtures/java/26.3/registry_names.json");

/// Reference recipes that are registered but intentionally absent or
/// different in VoxelCraft, each with the decision behind it.
pub const JAVA_ALLOWED: &[(&str, &str)] = &[
    ("crafting_dye/leather_helmet_dyed", "dyed leather colours (a data component) don't exist yet"),
    ("crafting_dye/leather_chestplate_dyed", "dyed leather colours (a data component) don't exist yet"),
    ("crafting_dye/leather_leggings_dyed", "dyed leather colours (a data component) don't exist yet"),
    ("crafting_dye/leather_boots_dyed", "dyed leather colours (a data component) don't exist yet"),
    ("crafting_dye/leather_horse_armor_dyed", "dyed leather colours (a data component) don't exist yet"),
    (
        "crafting_special_shielddecoration/shield_decoration",
        "banner patterns arrive with the loom (v0.17); a shield has no pattern to carry yet",
    ),
    (
        "crafting_special_repairitem/repair_item",
        "grid repair of two damaged items isn't implemented; anvils and grindstones repair",
    ),
];

/// Banner duplication copies a patterned banner's layers onto a blank one;
/// banners are plain until the loom (owner decision for v0.12; v0.17).
fn banner_duplicate(key: &str) -> bool {
    key.starts_with("crafting_special_bannerduplicate/")
}

/// VoxelCraft recipes with no Java 26.3 counterpart that are kept, by
/// result name, with the reason.
pub const VOXELCRAFT_ALLOWED: &[(&str, &str)] = &[];

/// Java ids whose VoxelCraft item has another name: Java's `snow` is the
/// layer and `snow_block` the full block; 26.3 renamed the chain.
const ALIASES: &[(&str, &str)] = &[("snow", "snow layer"), ("snow_block", "snow"), ("iron_chain", "chain")];

/// Java names resolved to VoxelCraft items, and back.
pub struct Names {
    by_name: HashMap<String, Item>,
    by_item: HashMap<Item, String>,
}

impl Names {
    pub fn load() -> Self {
        let registry: Value = serde_json::from_str(REGISTRY).unwrap();
        let items: BTreeMap<String, Item> = Item::all_items().map(|i| (i.name().to_string(), i)).collect();
        let mut by_name = HashMap::new();
        for (id, display) in registry["items"].as_object().unwrap() {
            // Non-block items first, so `wheat` is the item, not the crop.
            let spaced = id.replace('_', " ");
            let display = display.as_str().unwrap().to_lowercase();
            if let Some(&(_, ours)) = ALIASES.iter().find(|(java, _)| java == id) {
                by_name.insert(id.clone(), Item::from_name(ours).unwrap());
                continue;
            }
            let found = items
                .get(&spaced)
                .copied()
                .or_else(|| Item::from_name(id))
                .or_else(|| (!display.is_empty()).then(|| Item::from_name(&display)).flatten());
            if let Some(item) = found {
                by_name.insert(id.clone(), item);
            }
        }
        let mut by_item = HashMap::new();
        for (name, &item) in &by_name {
            let entry = by_item.entry(item).or_insert_with(|| name.clone());
            // Two names on one item is a resolution bug; keep the first for messages.
            if entry != name && name < entry {
                *entry = name.clone();
            }
        }
        Self { by_name, by_item }
    }

    pub fn item(&self, name: &str) -> Option<Item> {
        self.by_name.get(name).copied()
    }

    /// The Java name of `item`, or its VoxelCraft name in angle brackets.
    pub fn name(&self, item: Item) -> String {
        self.by_item.get(&item).cloned().unwrap_or_else(|| format!("<{}>", item.name()))
    }

    /// Java names whose VoxelCraft item is named differently, for review.
    pub fn renamed(&self) -> Vec<(String, String)> {
        let mut out: Vec<_> = self
            .by_name
            .iter()
            .filter(|(id, item)| id.replace('_', " ") != item.name())
            .map(|(id, item)| (id.clone(), item.name().to_string()))
            .collect();
        out.sort();
        out
    }

    /// Java names that resolve to the same VoxelCraft item.
    pub fn collisions(&self) -> Vec<(Item, Vec<String>)> {
        let mut groups: BTreeMap<u16, Vec<String>> = BTreeMap::new();
        for (name, item) in &self.by_name {
            groups.entry(item.0).or_default().push(name.clone());
        }
        groups
            .into_iter()
            .filter(|(_, v)| v.len() > 1)
            .map(|(id, mut v)| {
                v.sort();
                (Item(id), v)
            })
            .collect()
    }
}

/// The reference data with tags expanded.
pub struct Reference {
    pub fixture: Value,
    pub names: Names,
}

/// The registered alternatives of one ingredient, and whether every
/// alternative is registered.
pub type Alternatives = BTreeSet<u16>;

impl Reference {
    pub fn load() -> Self {
        Self { fixture: serde_json::from_str(FIXTURE).unwrap(), names: Names::load() }
    }

    pub fn recipes(&self, kind: &str) -> impl Iterator<Item = (&str, &Value)> {
        self.fixture["recipes"][kind].as_object().into_iter().flatten().map(|(k, v)| (k.as_str(), v))
    }

    pub fn kinds(&self) -> Vec<String> {
        self.fixture["recipes"].as_object().unwrap().keys().cloned().collect()
    }

    /// Every alternative name of an ingredient value.
    pub fn names_of(&self, value: &Value) -> Vec<String> {
        match value {
            Value::String(s) if s.starts_with('#') => self.fixture["item_tags"][&s[1..]]
                .as_array()
                .unwrap_or_else(|| panic!("tag {s}"))
                .iter()
                .map(|v| v.as_str().unwrap().to_string())
                .collect(),
            Value::String(s) => vec![s.clone()],
            Value::Array(list) => list.iter().map(|v| v.as_str().unwrap().to_string()).collect(),
            other => panic!("ingredient {other}"),
        }
    }

    /// The registered items of an ingredient; `None` when none is registered.
    pub fn ingredient(&self, value: &Value) -> Option<Alternatives> {
        let set: Alternatives = self.names_of(value).iter().filter_map(|n| self.names.item(n)).map(|i| i.0).collect();
        (!set.is_empty()).then_some(set)
    }

    /// Names in a recipe that VoxelCraft doesn't register (for the report).
    pub fn missing_names(&self, recipe: &Value) -> Vec<String> {
        let mut out = BTreeSet::new();
        let mut visit = |v: &Value| {
            for n in self.names_of(v) {
                if self.names.item(&n).is_none() {
                    out.insert(n);
                }
            }
        };
        for (key, value) in recipe.as_object().unwrap() {
            match key.as_str() {
                "result" if value.is_string() => visit(value),
                "key" => value.as_object().unwrap().values().for_each(&mut visit),
                "ingredients" => value.as_array().unwrap().iter().for_each(&mut visit),
                "result"
                | "count"
                | "components"
                | "pattern"
                | "material_count"
                | "add_material_count_to_result"
                | "cookingtime"
                | "experience" => {}
                // Firework star shapes: shape name -> ingredient.
                _ if value.is_object() => value.as_object().unwrap().values().for_each(&mut visit),
                _ => visit(value),
            }
        }
        out.into_iter().collect()
    }

    /// Java's cooking time once the furnace's speed multiplier applies:
    /// `ceil(cookingtime / speed)` (`AbstractFurnaceBlockEntity.getTotalCookTime`),
    /// with the fast multiplier in the fast-cooking blocks. Campfires have none.
    pub fn effective_ticks(&self, kind: &str, ticks: u32) -> u32 {
        let speed = &self.fixture["cooking_speed"];
        let Some(block) = speed["recipe_blocks"][kind].as_str() else { return ticks };
        let fast = speed["fast_blocks"].as_array().unwrap().iter().any(|b| b == block);
        let multiplier = speed[if fast { "fast" } else { "normal" }].as_f64().unwrap() as f32;
        (ticks as f32 / multiplier).ceil() as u32
    }

    pub fn result(&self, recipe: &Value) -> Option<(Item, u8)> {
        let item = self.names.item(recipe["result"].as_str()?)?;
        Some((item, recipe.get("count").and_then(Value::as_u64).unwrap_or(1) as u8))
    }
}

/// A crafting recipe in comparable form: shaped patterns are stored as the
/// smaller of the pattern and its mirror; shapeless ingredients are sorted.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Debug, Hash)]
pub enum Form {
    Shaped(usize, usize, Vec<Option<Alternatives>>),
    Shapeless(Vec<Alternatives>),
}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Debug, Hash)]
pub struct Crafting {
    pub form: Form,
    pub result: u16,
    pub count: u8,
}

fn shaped(w: usize, h: usize, cells: Vec<Option<Alternatives>>) -> Form {
    let mirrored: Vec<_> =
        (0..h).flat_map(|y| (0..w).rev().map(move |x| (x, y))).map(|(x, y)| cells[y * w + x].clone()).collect();
    Form::Shaped(w, h, cells.min(mirrored))
}

fn shapeless(mut list: Vec<Alternatives>) -> Form {
    list.sort();
    Form::Shapeless(list)
}

/// A reference crafting recipe in comparable form, or why it isn't.
pub enum JavaCrafting {
    Compared(Crafting),
    Unregistered(Vec<String>),
    /// Not a grid recipe VoxelCraft can express (dye components, specials).
    Special,
}

impl Reference {
    pub fn crafting(&self, kind: &str, recipe: &Value) -> JavaCrafting {
        let unregistered = || JavaCrafting::Unregistered(self.missing_names(recipe));
        if !recipe["result"].is_null() && self.result(recipe).is_none() {
            return unregistered();
        }
        let Some((result, count)) = self.result(recipe) else {
            // Map cloning keeps the input item; specials name no result.
            return if self.missing_names(recipe).is_empty() { JavaCrafting::Special } else { unregistered() };
        };
        let form = match kind {
            "crafting_shaped" => {
                let rows: Vec<&str> =
                    recipe["pattern"].as_array().unwrap().iter().map(|r| r.as_str().unwrap()).collect();
                let mut cells = Vec::new();
                for c in rows.iter().flat_map(|r| r.chars()) {
                    if c == ' ' {
                        cells.push(None);
                    } else {
                        match self.ingredient(&recipe["key"][c.to_string()]) {
                            Some(set) => cells.push(Some(set)),
                            None => return unregistered(),
                        }
                    }
                }
                shaped(rows[0].len(), rows.len(), cells)
            }
            "crafting_shapeless" => {
                let mut list = Vec::new();
                for v in recipe["ingredients"].as_array().unwrap() {
                    match self.ingredient(v) {
                        Some(set) => list.push(set),
                        None => return unregistered(),
                    }
                }
                shapeless(list)
            }
            "crafting_transmute" if recipe.get("material_count").is_none() => {
                // Java keeps the input's components; VoxelCraft items carry
                // none that these recipes change, so a shapeless pair matches.
                match (self.ingredient(&recipe["input"]), self.ingredient(&recipe["material"])) {
                    (Some(a), Some(b)) => shapeless(vec![a, b]),
                    _ => return unregistered(),
                }
            }
            "crafting_decorated_pot" => {
                // The four faces around an empty middle, as `decorated_pot_simple`.
                let face = |k: &str| self.ingredient(&recipe[k]);
                match (face("back"), face("left"), face("right"), face("front")) {
                    (Some(b), Some(l), Some(r), Some(f)) => {
                        shaped(3, 3, vec![None, Some(b), None, Some(l), None, Some(r), None, Some(f), None])
                    }
                    _ => return unregistered(),
                }
            }
            _ => {
                if !self.missing_names(recipe).is_empty() {
                    return unregistered();
                }
                return JavaCrafting::Special;
            }
        };
        JavaCrafting::Compared(Crafting { form, result: result.0, count })
    }
}

/// VoxelCraft's crafting recipes in comparable form, with their index.
pub fn voxelcraft_crafting() -> Vec<(usize, Crafting)> {
    let set = |options: &[Item]| options.iter().map(|i| i.0).collect::<Alternatives>();
    crafting::recipes()
        .iter()
        .enumerate()
        .map(|(i, r)| {
            let form = match r.layout() {
                Layout::Shaped(w, h, cells) => shaped(w, h, cells.into_iter().map(|c| c.map(set)).collect()),
                Layout::Shapeless(list) => shapeless(list.iter().map(|o| set(o)).collect()),
            };
            (i, Crafting { form, result: r.result.item.0, count: r.result.count })
        })
        .collect()
}

/// Every grid built from the recipe with one cell switched to each of its
/// alternatives (the rest at their first), and what it should craft.
pub fn grids(c: &Crafting) -> Vec<Grid> {
    let (size, cells): (usize, Vec<(usize, &Alternatives)>) = match &c.form {
        Form::Shaped(w, h, cells) => {
            let size = if *w > 2 || *h > 2 { 3 } else { 2 };
            (
                size,
                cells.iter().enumerate().filter_map(|(i, s)| s.as_ref().map(|s| ((i / w) * size + i % w, s))).collect(),
            )
        }
        Form::Shapeless(list) => (if list.len() > 4 { 3 } else { 2 }, list.iter().enumerate().collect()),
    };
    let base = |grid: &mut Grid| {
        for &(slot, set) in &cells {
            grid.cells[slot] = Some(Stack::new(Item(*set.first().unwrap()), 1));
        }
    };
    let mut out = Vec::new();
    for &(slot, set) in &cells {
        for &alt in set {
            let mut grid = Grid::new(size);
            base(&mut grid);
            grid.cells[slot] = Some(Stack::new(Item(alt), 1));
            out.push(grid);
        }
    }
    out
}

/// Readable form of a crafting recipe for failure messages.
pub fn describe(names: &Names, c: &Crafting) -> String {
    let alt = |s: &Alternatives| {
        let v: Vec<String> = s.iter().map(|&i| names.name(Item(i))).collect();
        if v.len() == 1 { v[0].clone() } else { format!("[{}]", v.join("|")) }
    };
    let body = match &c.form {
        Form::Shaped(w, h, cells) => {
            let cells: Vec<String> = cells.iter().map(|c| c.as_ref().map_or("_".into(), alt)).collect();
            format!("shaped {w}x{h} {}", cells.join(" "))
        }
        Form::Shapeless(list) => format!("shapeless {}", list.iter().map(alt).collect::<Vec<_>>().join(" + ")),
    };
    format!("{} x{} <- {body}", names.name(Item(c.result)), c.count)
}

/// Every item VoxelCraft can hold: non-block items, the creative palette
/// and every item a reference name resolves to.
pub fn every_item(names: &Names) -> BTreeSet<u16> {
    let mut set: BTreeSet<u16> = Item::all_items().map(|i| i.0).collect();
    set.extend(Block::creative_palette().map(|b| Item::from_block(b).0));
    set.extend(Item::creative_palette().map(|i| i.0));
    set.extend(names.by_name.values().map(|i| i.0));
    set
}

/// What a cooking recipe type does to `item` in VoxelCraft.
pub fn cooks(kind: &str, item: Item) -> Option<Item> {
    use furnace::FurnaceKind;
    match kind {
        "smelting" => FurnaceKind::Normal.smelt(item),
        "blasting" => FurnaceKind::Blast.smelt(item),
        "smoking" => FurnaceKind::Smoker.smelt(item),
        "campfire_cooking" => voxelcraft::world::utilities::campfire::cooked(item),
        _ => unreachable!(),
    }
}

/// VoxelCraft's cooking time in ticks for a recipe type.
pub fn cook_ticks(kind: &str) -> u32 {
    use furnace::FurnaceKind;
    let seconds = match kind {
        "smelting" => FurnaceKind::Normal.cook_time(),
        "blasting" => FurnaceKind::Blast.cook_time(),
        "smoking" => FurnaceKind::Smoker.cook_time(),
        "campfire_cooking" => return voxelcraft::world::utilities::campfire::COOK_TICKS as u32,
        _ => unreachable!(),
    };
    (seconds * 20.0).round() as u32
}

/// The outcome of one comparison section.
#[derive(Default, Debug)]
pub struct Section {
    pub compared: usize,
    pub matched: usize,
    pub unregistered: BTreeMap<String, Vec<String>>,
    pub allowed: Vec<(String, String)>,
    pub special: Vec<String>,
    pub failures: Vec<String>,
}

/// Compares every crafting recipe both ways.
pub fn compare_crafting(r: &Reference) -> Section {
    let mut s = Section::default();
    let ours = voxelcraft_crafting();
    let mut ours_by_form: HashMap<&Crafting, Vec<usize>> = HashMap::new();
    for (i, c) in &ours {
        ours_by_form.entry(c).or_default().push(*i);
    }
    let mut java_forms = BTreeSet::new();
    let allowed: BTreeMap<&str, &str> = JAVA_ALLOWED.iter().copied().collect();
    for kind in r.kinds().into_iter().filter(|k| k.starts_with("crafting_")) {
        for (id, recipe) in r.recipes(&kind) {
            let key = format!("{kind}/{id}");
            let c = match r.crafting(&kind, recipe) {
                JavaCrafting::Unregistered(missing) => {
                    s.unregistered.insert(key, missing);
                    continue;
                }
                JavaCrafting::Special => {
                    if banner_duplicate(&key) {
                        s.allowed.push((key, "banner patterns arrive with the loom (v0.17); banners are plain".into()));
                    } else if let Some(reason) = allowed.get(key.as_str()) {
                        s.allowed.push((key, reason.to_string()));
                    } else {
                        s.special.push(key);
                    }
                    continue;
                }
                JavaCrafting::Compared(c) => c,
            };
            s.compared += 1;
            java_forms.insert(c.clone());
            if let Some(reason) = allowed.get(key.as_str()) {
                s.allowed.push((key, reason.to_string()));
                continue;
            }
            if !ours_by_form.contains_key(&c) {
                let same_result: Vec<String> =
                    ours.iter().filter(|(_, o)| o.result == c.result).map(|(_, o)| describe(&r.names, o)).collect();
                s.failures.push(format!(
                    "{key}: missing or different\n    java: {}\n    ours with that result: {same_result:#?}",
                    describe(&r.names, &c)
                ));
                continue;
            }
            // The grids must craft it, whatever other recipes exist.
            let mut ok = true;
            for grid in grids(&c) {
                let got = grid.result();
                if got != Some(Stack::new(Item(c.result), c.count)) {
                    let cells: Vec<String> = grid.cells[..grid.size * grid.size]
                        .iter()
                        .map(|c| c.map_or("_".into(), |s| r.names.name(s.item)))
                        .collect();
                    s.failures.push(format!(
                        "{key}: grid {} crafts {:?} instead",
                        cells.join(" "),
                        got.map(|s| (r.names.name(s.item), s.count))
                    ));
                    ok = false;
                    break;
                }
            }
            if ok {
                s.matched += 1;
            }
        }
    }
    let kept: BTreeMap<&str, &str> = VOXELCRAFT_ALLOWED.iter().copied().collect();
    for (_, c) in &ours {
        if !java_forms.contains(c) {
            let text = describe(&r.names, c);
            if let Some(reason) = kept.get(r.names.name(Item(c.result)).as_str()) {
                s.allowed.push((format!("voxelcraft/{text}"), reason.to_string()));
            } else {
                s.failures.push(format!("invented (no Java recipe): {text}"));
            }
        }
    }
    for (key, _) in JAVA_ALLOWED.iter().filter(|(k, _)| k.starts_with("crafting")) {
        if !s.allowed.iter().any(|(k, _)| k == key) {
            s.failures.push(format!("stale allow-list entry {key}"));
        }
    }
    s
}

/// Compares smelting, blasting, smoking and campfire cooking both ways.
pub fn compare_cooking(r: &Reference) -> Section {
    let mut s = Section::default();
    let everything = every_item(&r.names);
    for kind in ["smelting", "blasting", "smoking", "campfire_cooking"] {
        let mut expected: BTreeMap<u16, u16> = BTreeMap::new();
        for (id, recipe) in r.recipes(kind) {
            let key = format!("{kind}/{id}");
            let (Some((result, _)), Some(inputs)) = (r.result(recipe), r.ingredient(&recipe["ingredient"])) else {
                s.unregistered.insert(key, r.missing_names(recipe));
                continue;
            };
            s.compared += 1;
            let mut ok = true;
            let ticks = r.effective_ticks(kind, recipe["cookingtime"].as_u64().unwrap() as u32);
            if cook_ticks(kind) != ticks {
                s.failures.push(format!("{key}: cooks in {} ticks, Java {ticks}", cook_ticks(kind)));
                ok = false;
            }
            let xp = recipe["experience"].as_f64().unwrap() as f32;
            if kind != "campfire_cooking" && furnace::smelt_xp(result) != xp {
                s.failures.push(format!("{key}: {} experience, Java {xp}", furnace::smelt_xp(result)));
                ok = false;
            }
            for &input in &inputs {
                expected.insert(input, result.0);
                let got = cooks(kind, Item(input));
                if got != Some(result) {
                    s.failures.push(format!(
                        "{key}: {} gives {:?}, Java {}",
                        r.names.name(Item(input)),
                        got.map(|i| r.names.name(i)),
                        r.names.name(result)
                    ));
                    ok = false;
                }
            }
            if ok {
                s.matched += 1;
            }
        }
        for &item in &everything {
            if let Some(out) = cooks(kind, Item(item))
                && expected.get(&item) != Some(&out.0)
            {
                s.failures.push(format!(
                    "invented {kind}: {} -> {} has no Java recipe",
                    r.names.name(Item(item)),
                    r.names.name(out)
                ));
            }
        }
    }
    s
}

/// Compares the stonecutter data both ways.
pub fn compare_stonecutting(r: &Reference) -> Section {
    let mut s = Section::default();
    let ours = crafting::stonecutting::all();
    let mut expected = BTreeSet::new();
    for (id, recipe) in r.recipes("stonecutting") {
        let key = format!("stonecutting/{id}");
        let (Some((result, count)), Some(inputs)) = (r.result(recipe), r.ingredient(&recipe["ingredient"])) else {
            s.unregistered.insert(key, r.missing_names(recipe));
            continue;
        };
        s.compared += 1;
        let mut ok = true;
        for &input in &inputs {
            expected.insert((input, result.0, count));
            if !ours.contains(&(Item(input), result, count)) {
                let got: Vec<_> = ours.iter().filter(|o| o.0 == Item(input) && o.1 == result).map(|o| o.2).collect();
                s.failures.push(format!("{key}: missing (ours gives counts {got:?})"));
                ok = false;
            }
        }
        if ok {
            s.matched += 1;
        }
    }
    let mut seen = BTreeSet::new();
    for &(input, output, count) in &ours {
        if !seen.insert((input.0, output.0, count)) {
            s.failures.push(format!("duplicate stonecutting {} -> {}", r.names.name(input), r.names.name(output)));
        }
        if !expected.contains(&(input.0, output.0, count)) {
            s.failures.push(format!(
                "invented stonecutting: {} -> {} x{count}",
                r.names.name(input),
                r.names.name(output)
            ));
        }
    }
    s
}

/// Compares the smithing transforms both ways; trims are a mechanic
/// VoxelCraft doesn't have.
pub fn compare_smithing(r: &Reference) -> Section {
    use voxelcraft::smithing;
    let mut s = Section::default();
    let mut bases = BTreeSet::new();
    for (id, recipe) in r.recipes("smithing_transform") {
        let key = format!("smithing_transform/{id}");
        let slots = ["template", "base", "addition"].map(|k| r.ingredient(&recipe[k]));
        let (Some((result, _)), [Some(template), Some(base), Some(addition)]) = (r.result(recipe), slots) else {
            s.unregistered.insert(key, r.missing_names(recipe));
            continue;
        };
        s.compared += 1;
        let mut ok = true;
        for &t in &template {
            for &b in &base {
                bases.insert(b);
                for &a in &addition {
                    let got = smithing::upgrade(Stack::new(Item(t), 1), Stack::new(Item(b), 1), Stack::new(Item(a), 1));
                    if got.map(|s| s.item) != Some(result) {
                        s.failures.push(format!("{key}: gives {:?}", got.map(|s| r.names.name(s.item))));
                        ok = false;
                    }
                }
            }
        }
        if ok {
            s.matched += 1;
        }
    }
    for item in every_item(&r.names) {
        if smithing::is_base(Item(item)) && !bases.contains(&item) {
            s.failures.push(format!("invented smithing base {}", r.names.name(Item(item))));
        }
    }
    for (id, recipe) in r.recipes("smithing_trim") {
        let key = format!("smithing_trim/{id}");
        if r.ingredient(&recipe["template"]).is_none() {
            s.unregistered.insert(key, r.missing_names(recipe));
        } else {
            s.special.push(key);
        }
    }
    s
}
