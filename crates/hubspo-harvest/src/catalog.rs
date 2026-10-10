//! HubSpot's public API catalog (`/public/api/spec/v1/specs`) and the policy
//! that picks one OpenAPI document per API.
//!
//! HubSpot labels versions two ways at once: the older integer releases
//! (`"3"`, `"4"`) and the date releases introduced in 2025 (`"2025-09"`,
//! `"2026-03"`). Comparing the labels as strings is wrong — `"3"` sorts after
//! `"2026-03"` — so they are parsed into [`Version`] first.
//!
//! Selection policy, in order (pre-registered before running it on the real
//! catalog):
//!
//! 1. the newest dated release with stage `STABLE`;
//! 2. otherwise the newest numeric release with stage `STABLE`;
//! 3. otherwise the newest dated release with stage `LATEST`;
//! 4. otherwise nothing. `DEVELOPER_PREVIEW` is never selected: it is not a
//!    contract HubSpot has committed to.
//!
//! Beta labels (`"2027-03-beta"`) do not parse, so they are never selected
//! either, whatever their stage. On the 2026-10-10 catalog that excludes,
//! among others, Automation V4 (workflows), Forms, Lead Scoring and
//! Reporting: all four exist only as beta or preview. Their concepts are
//! evidenced from elsewhere, never from an uncommitted contract.

use serde::Deserialize;

/// The catalog document.
#[derive(Debug, Clone, Deserialize)]
pub struct Catalog {
    pub results: Vec<ApiEntry>,
}

/// One API in the catalog, e.g. group `CRM`, name `Contacts`.
#[derive(Debug, Clone, Deserialize)]
pub struct ApiEntry {
    pub name: String,
    pub group: String,
    pub versions: Vec<VersionEntry>,
}

/// One published version of an API.
#[derive(Debug, Clone, Deserialize)]
pub struct VersionEntry {
    pub version: String,
    pub stage: String,
    #[serde(rename = "openApi")]
    pub open_api: String,
}

/// A parsed version label.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Version {
    /// An integer release such as `"3"`.
    Numeric(u32),
    /// A date release such as `"2026-03"`.
    Dated { year: u16, month: u8 },
}

impl Version {
    /// Parses `"3"` or `"2026-03"`. Anything else is `None` and is never
    /// selected.
    pub fn parse(label: &str) -> Option<Self> {
        if let Some((y, m)) = label.split_once('-') {
            let year: u16 = y.parse().ok()?;
            let month: u8 = m.parse().ok()?;
            if y.len() == 4 && (1..=12).contains(&month) {
                return Some(Version::Dated { year, month });
            }
            return None;
        }
        label.parse().ok().map(Version::Numeric)
    }

    fn is_dated(self) -> bool {
        matches!(self, Version::Dated { .. })
    }
}

impl Catalog {
    pub fn parse(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }
}

impl ApiEntry {
    /// The version the harvester reads, per the module's selection policy.
    pub fn select(&self) -> Option<&VersionEntry> {
        let parsed: Vec<(Version, &VersionEntry)> = self
            .versions
            .iter()
            .filter_map(|v| Version::parse(&v.version).map(|p| (p, v)))
            .collect();
        let newest = |dated: bool, stage: &str| {
            parsed
                .iter()
                .filter(|(p, v)| p.is_dated() == dated && v.stage == stage)
                .max_by_key(|(p, _)| *p)
                .map(|(_, v)| *v)
        };
        newest(true, "STABLE")
            .or_else(|| newest(false, "STABLE"))
            .or_else(|| newest(true, "LATEST"))
    }

    /// A file-system-safe name for this API: `crm-line-items`.
    pub fn slug(&self) -> String {
        let raw = format!("{}-{}", self.group, self.name).to_lowercase();
        let mut out = String::with_capacity(raw.len());
        for c in raw.chars() {
            if c.is_ascii_alphanumeric() {
                out.push(c);
            } else if !out.ends_with('-') {
                out.push('-');
            }
        }
        out.trim_matches('-').to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(versions: &[(&str, &str)]) -> ApiEntry {
        ApiEntry {
            name: "Contacts".into(),
            group: "CRM".into(),
            versions: versions
                .iter()
                .map(|(v, s)| VersionEntry {
                    version: (*v).into(),
                    stage: (*s).into(),
                    open_api: format!("https://example.invalid/{v}"),
                })
                .collect(),
        }
    }

    fn picked(versions: &[(&str, &str)]) -> Option<String> {
        entry(versions).select().map(|v| v.version.clone())
    }

    #[test]
    fn version_labels_parse_and_order_by_kind_then_value() {
        assert_eq!(Version::parse("3"), Some(Version::Numeric(3)));
        assert_eq!(
            Version::parse("2026-03"),
            Some(Version::Dated {
                year: 2026,
                month: 3
            })
        );
        for bad in ["", "v3", "2026-13", "26-03", "2026-xx", "2027-03-beta"] {
            assert_eq!(Version::parse(bad), None, "{bad}");
        }
        assert!(Version::parse("2025-09") < Version::parse("2026-03"));
        assert!(Version::parse("3") < Version::parse("4"));
    }

    #[test]
    fn newest_dated_stable_wins_over_a_string_larger_numeric_label() {
        // The real Contacts shape. String order would pick "3".
        let v = [
            ("3", "STABLE"),
            ("2026-03", "STABLE"),
            ("2025-09", "STABLE"),
            ("2026-09", "LATEST"),
        ];
        assert_eq!(picked(&v).as_deref(), Some("2026-03"));
    }

    #[test]
    fn numeric_stable_is_used_when_no_dated_release_is_stable() {
        // The real Lists shape: the dated releases are LATEST / preview only.
        let v = [
            ("2026-03", "LATEST"),
            ("3", "STABLE"),
            ("2026-09", "DEVELOPER_PREVIEW"),
        ];
        assert_eq!(picked(&v).as_deref(), Some("3"));
    }

    #[test]
    fn latest_is_a_fallback_and_preview_is_never_picked() {
        assert_eq!(
            picked(&[("2026-09", "LATEST"), ("2026-12", "DEVELOPER_PREVIEW")]).as_deref(),
            Some("2026-09")
        );
        assert_eq!(picked(&[("2026-09", "DEVELOPER_PREVIEW")]), None);
        // A beta label is not a release, even when HubSpot stages it LATEST.
        assert_eq!(picked(&[("2027-03-beta", "LATEST")]), None);
        assert_eq!(picked(&[("beta", "STABLE")]), None);
    }

    #[test]
    fn slugs_are_lowercase_and_hyphenated() {
        let mut e = entry(&[]);
        e.name = "Price Books Crm Reads Api".into();
        assert_eq!(e.slug(), "crm-price-books-crm-reads-api");
        e.group = "Communication Preferences".into();
        e.name = "Subscriptions".into();
        assert_eq!(e.slug(), "communication-preferences-subscriptions");
    }
}
