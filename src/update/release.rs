//! A published release, read from GitHub's release JSON
//! (`GET /repos/{owner}/{repo}/releases/latest`).

use serde_json::Value;

use super::version::Version;

#[derive(Clone, Debug, PartialEq)]
pub struct Asset {
    pub name: String,
    pub size: u64,
    pub url: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Release {
    pub version: Version,
    /// The release body: the in-game "what's new" text.
    pub notes: String,
    /// The release's web page.
    pub page: String,
    pub assets: Vec<Asset>,
}

pub const SUMS: &str = "SHA256SUMS.txt";
pub const SIGNATURE: &str = "SHA256SUMS.txt.sig";

impl Release {
    pub fn parse(json: &str) -> Result<Self, String> {
        let v: Value = serde_json::from_str(json).map_err(|e| format!("unreadable release information: {e}"))?;
        // GitHub's latest-release endpoint omits these, but a folder source might not.
        if v["draft"].as_bool() == Some(true) || v["prerelease"].as_bool() == Some(true) {
            return Err("the latest release is a draft or pre-release".into());
        }
        let tag = v["tag_name"].as_str().ok_or("the release has no tag")?;
        let version = Version::parse(tag).ok_or_else(|| format!("release tag {tag:?} is not a version"))?;
        let assets = v["assets"]
            .as_array()
            .map(|assets| {
                assets
                    .iter()
                    .filter_map(|a| {
                        Some(Asset {
                            name: a["name"].as_str()?.to_string(),
                            size: a["size"].as_u64()?,
                            url: a["browser_download_url"].as_str()?.to_string(),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        Ok(Self {
            version,
            notes: v["body"].as_str().unwrap_or_default().replace("\r\n", "\n"),
            // Opened with the system's URL handler: web pages only.
            page: v["html_url"]
                .as_str()
                .filter(|url| url.starts_with("https://"))
                .unwrap_or(super::RELEASES_PAGE)
                .to_string(),
            assets,
        })
    }

    pub fn asset(&self, name: &str) -> Option<&Asset> {
        self.assets.iter().find(|a| a.name == name)
    }

    /// The release notes as plain lines: headings, emphasis and link
    /// targets removed, list items marked with "-".
    pub fn plain_notes(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for line in self.notes.lines() {
            let line = line.trim_end();
            let trimmed = line.trim_start();
            let text = if let Some(heading) = trimmed.strip_prefix('#') {
                heading.trim_start_matches('#').trim().to_string()
            } else if let Some(item) = trimmed.strip_prefix("* ").or_else(|| trimmed.strip_prefix("- ")) {
                let indent = (line.len() - trimmed.len()).min(8);
                format!("{}- {}", " ".repeat(indent), item)
            } else {
                trimmed.to_string()
            };
            let text = strip_links(&text).replace("**", "").replace('`', "");
            if text.is_empty() && out.last().is_none_or(|l| l.is_empty()) {
                continue;
            }
            out.push(text);
        }
        while out.last().is_some_and(|l| l.is_empty()) {
            out.pop();
        }
        out
    }
}

/// `[text](url)` becomes `text`.
fn strip_links(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(open) = rest.find('[') {
        let Some(close) = rest[open..].find("](").map(|i| open + i) else { break };
        let Some(end) = rest[close..].find(')').map(|i| close + i) else { break };
        out.push_str(&rest[..open]);
        out.push_str(&rest[open + 1..close]);
        rest = &rest[end + 1..];
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const LATEST: &str = r###"{
        "tag_name": "v0.11",
        "html_url": "https://github.com/BrendanH18/VoxelCraft/releases/tag/v0.11",
        "draft": false,
        "prerelease": false,
        "body": "## What's new\r\n\r\n- **Signs** and [banners](https://example.com)\r\n  * nested `item`\r\n\r\n",
        "assets": [
            {"name": "SHA256SUMS.txt", "size": 200, "browser_download_url": "https://github.com/BrendanH18/VoxelCraft/releases/download/v0.11/SHA256SUMS.txt"},
            {"name": "VoxelCraft-0.11.0-windows-x64-Setup.exe", "size": 9000000, "browser_download_url": "https://github.com/BrendanH18/VoxelCraft/releases/download/v0.11/VoxelCraft-0.11.0-windows-x64-Setup.exe"},
            {"name": "broken"}
        ]
    }"###;

    #[test]
    fn reads_github_release() {
        let r = Release::parse(LATEST).unwrap();
        assert_eq!(r.version, Version(0, 11, 0));
        assert_eq!(r.page, "https://github.com/BrendanH18/VoxelCraft/releases/tag/v0.11");
        assert_eq!(r.assets.len(), 2, "assets missing a field are skipped");
        assert_eq!(r.asset("VoxelCraft-0.11.0-windows-x64-Setup.exe").unwrap().size, 9_000_000);
        assert!(r.asset("VoxelCraft-0.11.0-macos-apple-silicon.dmg").is_none());
        assert_eq!(r.plain_notes(), ["What's new", "", "- Signs and banners", "  - nested item"]);
    }

    #[test]
    fn rejects_drafts_and_bad_tags() {
        assert!(Release::parse(&LATEST.replace(r#""draft": false"#, r#""draft": true"#)).is_err());
        assert!(Release::parse(&LATEST.replace(r#""prerelease": false"#, r#""prerelease": true"#)).is_err());
        assert!(Release::parse(&LATEST.replace("\"v0.11\"", "\"nightly\"")).is_err());
        assert!(Release::parse("not json").is_err());
        assert_eq!(strip_links("a [b](c) d [e"), "a b d [e");
        for page in ["file:///Applications/Calculator.app", "javascript:alert(1)", "http://example.com"] {
            let r =
                Release::parse(&LATEST.replace("https://github.com/BrendanH18/VoxelCraft/releases/tag/v0.11", page));
            assert_eq!(r.unwrap().page, super::super::RELEASES_PAGE, "{page}");
        }
    }
}
