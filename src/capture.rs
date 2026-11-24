//! Parsing for the scripted-capture set-up flags (`--gui-scale`,
//! `--creative-tab`, `--gamerule`, `--weather`). The game applies them in
//! `app`; these helpers hold the rules so they are tested without a window.
//! See `docs/development.md#screenshots-and-scripted-scenes`.

/// The largest GUI scale `--gui-scale` accepts, as `options.txt` does
/// (the game still lowers it to what fits the window, as Java does).
pub const MAX_GUI_SCALE: u32 = 64;

/// `--gui-scale auto|N`: 0 is Auto (the largest scale that fits), else 1..=64.
pub fn parse_gui_scale(text: &str) -> Result<u32, String> {
    if text == "auto" {
        return Ok(0);
    }
    text.parse::<u32>()
        .ok()
        .filter(|&n| n <= MAX_GUI_SCALE)
        .ok_or_else(|| format!("--gui-scale needs auto or 0..{MAX_GUI_SCALE} (0 is auto), got {text}"))
}

/// Java 26.3's creative tabs (`CreativeModeTabs`' registry ids), in the
/// order the screen shows them: the top row, then the bottom row. Today the
/// creative screen has one palette, so every tab shows it; track 4 of v0.18
/// adds the tabs and checks this list against its fixture.
pub const CREATIVE_TABS: [&str; 14] = [
    "building_blocks",
    "colored_blocks",
    "natural_blocks",
    "functional_blocks",
    "redstone_blocks",
    "hotbar",
    "search",
    "tools_and_utilities",
    "combat",
    "food_and_drinks",
    "ingredients",
    "spawn_eggs",
    "op_blocks",
    "inventory",
];

/// `--creative-tab <id>`: one of [`CREATIVE_TABS`], with or without the
/// `minecraft:` namespace.
pub fn parse_creative_tab(text: &str) -> Result<&'static str, String> {
    let id = text.strip_prefix("minecraft:").unwrap_or(text);
    CREATIVE_TABS
        .iter()
        .copied()
        .find(|tab| *tab == id)
        .ok_or_else(|| format!("--creative-tab: expected one of {}, got {text}", CREATIVE_TABS.join(", ")))
}

/// `--gamerule name=value`: a rule the game enforces and a value it accepts,
/// checked against a default rule set. Returns the name and value as given.
pub fn parse_gamerule(text: &str) -> Result<(String, String), String> {
    let (name, value) = text.split_once('=').ok_or_else(|| format!("--gamerule needs name=value (got {text})"))?;
    let (name, value) = (name.trim(), value.trim());
    crate::rules::GameRules::default().set(name, value).map_err(|e| format!("--gamerule: {e}"))?;
    Ok((name.to_string(), value.to_string()))
}

/// `--weather clear|rain|thunder`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Weather {
    Clear,
    Rain,
    Thunder,
}

pub fn parse_weather(text: &str) -> Result<Weather, String> {
    match text {
        "clear" => Ok(Weather::Clear),
        "rain" => Ok(Weather::Rain),
        "thunder" => Ok(Weather::Thunder),
        _ => Err(format!("--weather needs clear, rain or thunder (got {text})")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gui_scale_takes_auto_or_a_number() {
        assert_eq!(parse_gui_scale("auto"), Ok(0));
        assert_eq!(parse_gui_scale("0"), Ok(0));
        assert_eq!(parse_gui_scale("3"), Ok(3));
        assert_eq!(parse_gui_scale("64"), Ok(64));
        for bad in ["65", "-1", "large", "", "2.5"] {
            assert!(parse_gui_scale(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn creative_tabs_are_java_ids() {
        assert_eq!(parse_creative_tab("search"), Ok("search"));
        assert_eq!(parse_creative_tab("minecraft:spawn_eggs"), Ok("spawn_eggs"));
        assert!(parse_creative_tab("blocks").is_err());
        let unique: std::collections::BTreeSet<_> = CREATIVE_TABS.iter().collect();
        assert_eq!(unique.len(), CREATIVE_TABS.len());
    }

    #[test]
    fn gamerules_are_checked_against_the_rule_table() {
        assert_eq!(parse_gamerule("advance_time=false"), Ok(("advance_time".into(), "false".into())));
        assert_eq!(parse_gamerule(" advance_weather = false "), Ok(("advance_weather".into(), "false".into())));
        assert!(parse_gamerule("advance_time").is_err(), "needs a value");
        assert!(parse_gamerule("advance_time=maybe").is_err(), "wrong type");
        assert!(parse_gamerule("no_such_rule=true").is_err());
    }

    #[test]
    fn weather_names() {
        assert_eq!(parse_weather("clear"), Ok(Weather::Clear));
        assert_eq!(parse_weather("rain"), Ok(Weather::Rain));
        assert_eq!(parse_weather("thunder"), Ok(Weather::Thunder));
        assert!(parse_weather("snow").is_err());
    }
}
