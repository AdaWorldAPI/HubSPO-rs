//! HubSpot's full API surface, from the developer docs' own navigation index
//! (D-API-SURFACE).
//!
//! Every page of the API reference embeds the navigation of its version tab.
//! Each operation entry carries its title, its OpenAPI coordinate
//! (`specs/<version>/<spec>.json METHOD /path`) and its page:
//! `{\"title\":\"…\",\"openapi\":\"specs/… GET /…\",\"href\":\"/api-reference/…\"}`.
//! One overview page per version (`latest`, `legacy`, `2027-03-beta`) holds
//! that version's whole list, so three pages give the whole surface.
//!
//! The data sits in a JavaScript string: quotes are escaped (`\"`). Only the
//! three fields above are kept; everything else on the page is dropped.

/// One operation of the documented surface.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Operation {
    /// API version: `legacy`, `2026-09`, `2027-03-beta`, …
    pub version: String,
    /// Reference area: the first `href` segment after the version
    /// (`crm`, `marketing`, `automation`, …).
    pub area: String,
    /// Spec files that list it, without the `specs/<version>/` prefix, in
    /// page order. A generic endpoint (`/crm/objects/{v}/{objectType}`) is
    /// listed by several specs.
    pub specs: Vec<String>,
    pub method: String,
    pub path: String,
    pub title: String,
    /// The operation's reference page, relative to `/docs`.
    pub href: String,
}

/// Why a page could not be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SurfaceError {
    /// An entry's fields are not where the shape says, at this byte offset.
    Malformed { at: usize },
}

const OPENAPI: &str = "\\\"openapi\\\":\\\"";
const TITLE: &str = "\\\"title\\\":\\\"";
const HREF: &str = ",\\\"href\\\":\\\"";
const QUOTE: &str = "\\\"";

const METHODS: &[&str] = &["GET", "POST", "PUT", "PATCH", "DELETE"];

/// Every operation in a page's navigation, in page order, each
/// `(method, path)` once with every spec that lists it. An entry that does
/// not have the expected shape is an error, never a shorter list.
///
/// # Errors
///
/// [`SurfaceError::Malformed`] at the first unreadable entry.
pub fn operations(html: &str) -> Result<Vec<Operation>, SurfaceError> {
    let mut out: Vec<Operation> = Vec::new();
    let mut from = 0;
    while let Some(i) = html[from..].find(OPENAPI) {
        let at = from + i;
        let op = entry(html, at).ok_or(SurfaceError::Malformed { at })?;
        from = at + OPENAPI.len();
        merge(&mut out, op);
    }
    Ok(out)
}

/// Add `op` to `ops`, or its specs to the operation already there.
pub fn merge(ops: &mut Vec<Operation>, op: Operation) {
    match ops
        .iter_mut()
        .find(|o| o.method == op.method && o.path == op.path)
    {
        Some(known) => {
            for spec in op.specs {
                if !known.specs.contains(&spec) {
                    known.specs.push(spec);
                }
            }
        }
        None => ops.push(op),
    }
}

/// Read the entry whose `"openapi"` key starts at `at`.
fn entry(html: &str, at: usize) -> Option<Operation> {
    // The title is the previous key of the same object.
    let open = html[..at].rfind('{')?;
    let title_start = open + html[open..at].find(TITLE)? + TITLE.len();
    let title_end = title_start + html[title_start..at].rfind(QUOTE)?;
    let title = unescape(&html[title_start..title_end]);

    let value_start = at + OPENAPI.len();
    let value_end = value_start + html[value_start..].find(QUOTE)?;
    let value = &html[value_start..value_end];
    let href_start = value_end + QUOTE.len();
    let href_value = html[href_start..].strip_prefix(HREF)?;
    let href_end = href_value.find(QUOTE)?;
    let href = href_value[..href_end].to_string();

    let (file, rest) = value.split_once(' ')?;
    let (method, path) = rest.split_once(' ')?;
    let file = file.strip_prefix("specs/")?;
    let (version, spec) = file.split_once('/')?;
    if !METHODS.contains(&method) || !path.starts_with('/') || !spec.ends_with(".json") {
        return None;
    }
    // `/api-reference/<tab>/<area>/…`: the tab is `latest` for the current
    // version, so the area is read by position, not by version name.
    let area = href.split('/').nth(3)?.to_string();
    Some(Operation {
        version: version.to_string(),
        area,
        specs: vec![spec.to_string()],
        method: method.to_string(),
        path: path.to_string(),
        title,
        href,
    })
}

fn unescape(s: &str) -> String {
    s.replace("\\\\\\\"", "\"").replace("\\\\\\\\", "\\")
}

/// The surface as TSV: `version, area, method, path, specs, title, href`,
/// specs joined by `,`.
/// Tabs and newlines in a title become spaces.
#[must_use]
pub fn to_tsv(ops: &[Operation]) -> String {
    let clean = |s: &str| s.replace(['\t', '\n', '\r'], " ");
    ops.iter()
        .map(|o| {
            format!(
                "{}\t{}\t{}\t{}\t{}\t{}\t{}\n",
                o.version,
                o.area,
                o.method,
                o.path,
                o.specs.join(","),
                clean(&o.title),
                o.href
            )
        })
        .collect()
}

/// `path` with its version segment (`v3`, `2026-09`, `2027-03-beta`, …)
/// replaced by `{v}`, so one operation compares across versions.
#[must_use]
pub fn unversioned(path: &str) -> String {
    path.split('/')
        .map(|seg| if is_version(seg) { "{v}" } else { seg })
        .collect::<Vec<_>>()
        .join("/")
}

fn is_version(seg: &str) -> bool {
    let b = seg.as_bytes();
    let numbered = |s: &[u8]| !s.is_empty() && s.iter().all(u8::is_ascii_digit);
    // v1, v3, v4 …
    if let Some(n) = seg.strip_prefix('v') {
        return numbered(n.as_bytes());
    }
    // 2026-09, 2027-03-beta
    b.len() >= 7 && numbered(&b[..4]) && b[4] == b'-' && numbered(&b[5..7])
}

/// Operations of one `(version, area)`, and how many of them a previous
/// harvest already holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AreaCount {
    pub version: String,
    pub area: String,
    pub operations: usize,
    /// Operations whose `(method, unversioned path)` the harvest holds.
    pub harvested: usize,
    /// Operations whose reference page also exists under the current
    /// version (`latest` tab): the same function, often at a reshaped path.
    pub in_current: usize,
}

/// An operation's reference page with its version tab removed
/// (`/api-reference/legacy/crm/x` → `crm/x`): the same function keeps its
/// page slug across versions even when its path changes shape.
#[must_use]
pub fn page_slug(href: &str) -> String {
    href.split('/').skip(3).collect::<Vec<_>>().join("/")
}

/// The reference tab of an operation's page (`latest`, `legacy`, …).
#[must_use]
pub fn tab(href: &str) -> &str {
    href.split('/').nth(2).unwrap_or("")
}

/// Count `ops` per `(version, area)`, in first-seen order, against the
/// `(method, path)` pairs of a previous harvest. Paths compare unversioned,
/// so a harvest of one dated version covers the same endpoint in another.
#[must_use]
pub fn coverage(ops: &[Operation], harvested: &[(String, String)]) -> Vec<AreaCount> {
    let known: std::collections::BTreeSet<(String, String)> = harvested
        .iter()
        .map(|(m, p)| (m.clone(), unversioned(p)))
        .collect();
    let current: std::collections::BTreeSet<String> = ops
        .iter()
        .filter(|o| tab(&o.href) == "latest")
        .map(|o| page_slug(&o.href))
        .collect();
    let mut out: Vec<AreaCount> = Vec::new();
    for op in ops {
        let hit = known.contains(&(op.method.clone(), unversioned(&op.path)));
        let i = match out
            .iter()
            .position(|c| c.version == op.version && c.area == op.area)
        {
            Some(i) => i,
            None => {
                out.push(AreaCount {
                    version: op.version.clone(),
                    area: op.area.clone(),
                    operations: 0,
                    harvested: 0,
                    in_current: 0,
                });
                out.len() - 1
            }
        };
        out[i].operations += 1;
        out[i].harvested += usize::from(hit);
        out[i].in_current += usize::from(current.contains(&page_slug(&op.href)));
    }
    out
}

/// Read the TSV written by [`to_tsv`] back.
///
/// # Errors
///
/// The 1-based number of the first line without seven fields.
pub fn parse_tsv(tsv: &str) -> Result<Vec<Operation>, usize> {
    tsv.lines()
        .enumerate()
        .filter(|(_, l)| !l.is_empty())
        .map(|(i, l)| {
            let f: Vec<&str> = l.split('\t').collect();
            let [version, area, method, path, specs, title, href] = f[..] else {
                return Err(i + 1);
            };
            Ok(Operation {
                version: version.into(),
                area: area.into(),
                specs: specs.split(',').map(String::from).collect(),
                method: method.into(),
                path: path.into(),
                title: title.into(),
                href: href.into(),
            })
        })
        .collect()
}

/// A spec file's family: its name without directory, `.json` and version
/// suffix (`v3/crm-contacts-v3.json` → `crm-contacts`), so one family
/// groups a function across versions.
#[must_use]
pub fn family(spec: &str) -> String {
    let name = spec.rsplit('/').next().unwrap_or(spec);
    let name = name.strip_suffix(".json").unwrap_or(name);
    match name.rsplit_once("-v") {
        Some((head, tail)) if is_version(&format!("v{tail}")) || is_version(tail) => {
            head.to_string()
        }
        _ => name.to_string(),
    }
}

/// One row of the family → deliverable map: the D-id that owns a spec
/// family, or `UNPLANNED` with the reason in `note`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MapRow {
    pub family: String,
    pub deliverable: String,
    pub note: String,
}

/// The deliverable value for a family no deliverable owns yet.
pub const UNPLANNED: &str = "UNPLANNED";

/// Read the map TSV (`family <TAB> deliverable <TAB> note`).
///
/// # Errors
///
/// The 1-based number of the first line without three fields, with an
/// empty field, or naming a family twice.
pub fn parse_map(tsv: &str) -> Result<Vec<MapRow>, usize> {
    let mut out: Vec<MapRow> = Vec::new();
    for (i, line) in tsv.lines().enumerate().filter(|(_, l)| !l.is_empty()) {
        let f: Vec<&str> = line.split('\t').collect();
        let [family, deliverable, note] = f[..] else {
            return Err(i + 1);
        };
        if [family, deliverable, note].iter().any(|s| s.is_empty())
            || out.iter().any(|r| r.family == family)
        {
            return Err(i + 1);
        }
        out.push(MapRow {
            family: family.into(),
            deliverable: deliverable.into(),
            note: note.into(),
        });
    }
    Ok(out)
}

/// The documented functions one deliverable owns.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DeliverableCount {
    pub deliverable: String,
    /// Spec families mapped to it.
    pub families: usize,
    /// Endpoints of the current version (`latest` tab).
    pub current: usize,
    /// Legacy endpoints whose reference page has no current counterpart.
    pub legacy_only: usize,
    /// Beta endpoints whose reference page has no current counterpart.
    pub beta_only: usize,
}

impl DeliverableCount {
    /// Distinct functions: current plus the legacy-only and beta-only ones.
    #[must_use]
    pub fn functions(&self) -> usize {
        self.current + self.legacy_only + self.beta_only
    }
}

/// Count the surface per deliverable, in map order. An operation belongs to
/// the family of its first spec.
///
/// # Errors
///
/// The first family with operations but no map row.
pub fn by_deliverable(ops: &[Operation], map: &[MapRow]) -> Result<Vec<DeliverableCount>, String> {
    let current: std::collections::BTreeSet<String> = ops
        .iter()
        .filter(|o| tab(&o.href) == "latest")
        .map(|o| page_slug(&o.href))
        .collect();
    let mut out: Vec<DeliverableCount> = Vec::new();
    for row in map {
        if !out.iter().any(|c| c.deliverable == row.deliverable) {
            out.push(DeliverableCount {
                deliverable: row.deliverable.clone(),
                ..DeliverableCount::default()
            });
        }
        if let Some(c) = out.iter_mut().find(|c| c.deliverable == row.deliverable) {
            c.families += 1;
        }
    }
    for op in ops {
        let fam = family(op.specs.first().map_or("", String::as_str));
        let row = map
            .iter()
            .find(|r| r.family == fam)
            .ok_or_else(|| fam.clone())?;
        let Some(c) = out.iter_mut().find(|c| c.deliverable == row.deliverable) else {
            return Err(fam);
        };
        if tab(&op.href) == "latest" {
            c.current += 1;
        } else if !current.contains(&page_slug(&op.href)) {
            if op.version == "legacy" {
                c.legacy_only += 1;
            } else {
                c.beta_only += 1;
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nav(title: &str, openapi: &str, href: &str) -> String {
        format!(r#"{{\"title\":\"{title}\",\"openapi\":\"{openapi}\",\"href\":\"{href}\"}}"#)
    }

    #[test]
    fn entries_parse_in_page_order_each_operation_once() {
        let page = [
            "<script>self.__next_f.push([1,\"[".to_string(),
            nav(
                "Retrieve account details",
                "specs/2026-09/account-account-info-v2026-09.json GET /account-info/2026-09/details",
                "/api-reference/latest/account/account-information/get-account-details",
            ),
            ",".into(),
            nav(
                "Create a contact",
                "specs/legacy/v3/crm-contacts-v3.json POST /crm/v3/objects/contacts",
                "/api-reference/legacy/crm/objects/contacts/create",
            ),
            ",".into(),
            // The same operation listed again under another heading.
            nav(
                "Create",
                "specs/legacy/v3/crm-contacts-v3.json POST /crm/v3/objects/contacts",
                "/api-reference/legacy/crm/x",
            ),
            "]\"])</script>".into(),
        ]
        .concat();
        let ops = operations(&page).unwrap();
        assert_eq!(ops.len(), 2);
        assert_eq!(ops[1].specs, ["v3/crm-contacts-v3.json"]);
        assert_eq!(ops[0].version, "2026-09");
        assert_eq!(ops[0].area, "account");
        assert_eq!(ops[0].specs, ["account-account-info-v2026-09.json"]);
        assert_eq!(ops[0].method, "GET");
        assert_eq!(ops[0].path, "/account-info/2026-09/details");
        assert_eq!(ops[0].title, "Retrieve account details");
        assert_eq!(ops[1].version, "legacy");
        assert_eq!(ops[1].area, "crm");
        assert_eq!(ops[1].title, "Create a contact");
    }

    #[test]
    fn one_endpoint_in_two_specs_is_one_operation_with_both() {
        let page = [
            nav(
                "List",
                "specs/legacy/v3/crm-objects-v3.json GET /crm/v3/objects/{objectType}",
                "/api-reference/legacy/crm/a",
            ),
            nav(
                "List",
                "specs/legacy/v3/crm-custom-objects-v3.json GET /crm/v3/objects/{objectType}",
                "/api-reference/legacy/crm/b",
            ),
            nav(
                "List",
                "specs/legacy/v3/crm-objects-v3.json GET /crm/v3/objects/{objectType}",
                "/api-reference/legacy/crm/c",
            ),
        ]
        .concat();
        let ops = operations(&page).unwrap();
        assert_eq!(ops.len(), 1);
        assert_eq!(
            ops[0].specs,
            ["v3/crm-objects-v3.json", "v3/crm-custom-objects-v3.json"]
        );
        assert_eq!(ops[0].href, "/api-reference/legacy/crm/a");
    }

    #[test]
    fn a_damaged_entry_is_an_error_not_a_shorter_list() {
        let good = nav(
            "A",
            "specs/legacy/v3/a.json GET /a/v3/x",
            "/api-reference/legacy/a/x",
        );
        assert_eq!(operations(&good).map(|o| o.len()), Ok(1));
        let no_method = nav(
            "A",
            "specs/legacy/v3/a.json FETCH /a/v3/x",
            "/api-reference/legacy/a/x",
        );
        assert!(matches!(
            operations(&no_method),
            Err(SurfaceError::Malformed { .. })
        ));
        let no_href = good.replace(r#",\"href\":\"/api-reference/legacy/a/x\""#, "");
        assert!(matches!(
            operations(&no_href),
            Err(SurfaceError::Malformed { .. })
        ));
        assert_eq!(operations("<html>no navigation</html>"), Ok(vec![]));
    }

    #[test]
    fn version_segments_are_replaced_and_nothing_else() {
        assert_eq!(
            unversioned("/crm/v3/objects/contacts"),
            "/crm/{v}/objects/contacts"
        );
        assert_eq!(
            unversioned("/crm/objects/2026-09/contacts/{contactId}"),
            "/crm/objects/{v}/contacts/{contactId}"
        );
        assert_eq!(
            unversioned("/account-info/2027-03-beta/details"),
            "/account-info/{v}/details"
        );
        assert_eq!(unversioned("/events/v/x"), "/events/v/x");
        assert_eq!(unversioned("/vids/2026/x"), "/vids/2026/x");
    }

    fn op(version: &str, area: &str, method: &str, path: &str) -> Operation {
        let tab = if version == "2026-09" {
            "latest"
        } else {
            version
        };
        let page = path.rsplit('/').next().unwrap_or("");
        Operation {
            version: version.into(),
            area: area.into(),
            specs: vec!["s.json".into()],
            method: method.into(),
            path: path.into(),
            title: "t".into(),
            href: format!("/api-reference/{tab}/{area}/{page}"),
        }
    }

    #[test]
    fn coverage_matches_across_versions_by_method_and_unversioned_path() {
        let ops = [
            op("legacy", "crm", "GET", "/crm/v3/objects/contacts"),
            op("legacy", "crm", "POST", "/crm/v3/objects/contacts"),
            op("2026-09", "crm", "GET", "/crm/objects/2026-09/contacts"),
            op("2026-09", "cms", "GET", "/cms/2026-09/pages"),
        ];
        let harvested = [
            (
                "GET".to_string(),
                "/crm/objects/2026-03/contacts".to_string(),
            ),
            ("GET".to_string(), "/crm/v4/objects/contacts".to_string()),
        ];
        let c = coverage(&ops, &harvested);
        let rows: Vec<(&str, &str, usize, usize, usize)> = c
            .iter()
            .map(|r| {
                (
                    r.version.as_str(),
                    r.area.as_str(),
                    r.operations,
                    r.harvested,
                    r.in_current,
                )
            })
            .collect();
        // Both legacy contacts ops share the current page slug `crm/contacts`.
        assert_eq!(
            rows,
            [
                ("legacy", "crm", 2, 1, 2),
                ("2026-09", "crm", 1, 1, 1),
                ("2026-09", "cms", 1, 0, 1)
            ]
        );
        // Silence twin: a legacy page with no current counterpart.
        let alone = [op("legacy", "cms", "GET", "/cms/v3/blogs")];
        assert_eq!(coverage(&alone, &harvested)[0].in_current, 0);
    }

    #[test]
    fn family_drops_directory_suffix_and_version() {
        assert_eq!(family("v3/crm-contacts-v3.json"), "crm-contacts");
        assert_eq!(family("crm-contacts-v2026-09.json"), "crm-contacts");
        assert_eq!(
            family("account-account-info-v2027-03-beta.json"),
            "account-account-info"
        );
        assert_eq!(family("v1/forms.json"), "forms");
        // `-v` inside a name is not a version.
        assert_eq!(
            family("v3/crm-video-conferencing-extension-v3.json"),
            "crm-video-conferencing-extension"
        );
        assert_eq!(
            family("v3/crm-public-app-feature-flags-v3-v3.json"),
            "crm-public-app-feature-flags-v3"
        );
    }

    #[test]
    fn deliverables_count_current_legacy_only_and_beta_only() {
        let mut beta = op("2027-03-beta", "crm", "GET", "/crm/2027-03-beta/new");
        beta.specs = vec!["crm-new-v2027-03-beta.json".into()];
        let mut legacy_same = op("legacy", "crm", "GET", "/crm/v3/objects/contacts");
        legacy_same.specs = vec!["v3/crm-contacts-v3.json".into()];
        let mut legacy_only = op("legacy", "crm", "GET", "/crm/v3/old");
        legacy_only.specs = vec!["v3/crm-contacts-v3.json".into()];
        let mut cur = op("2026-09", "crm", "GET", "/crm/objects/2026-09/contacts");
        cur.specs = vec!["crm-contacts-v2026-09.json".into()];
        let ops = [legacy_same, legacy_only, cur, beta];
        let map =
            parse_map("crm-contacts\tD-API-1\tcrud\ncrm-new\tUNPLANNED\tnew\nunused\tD-API-1\tx\n")
                .unwrap();
        let c = by_deliverable(&ops, &map).unwrap();
        let rows: Vec<(&str, usize, usize, usize, usize)> = c
            .iter()
            .map(|r| {
                (
                    r.deliverable.as_str(),
                    r.families,
                    r.current,
                    r.legacy_only,
                    r.beta_only,
                )
            })
            .collect();
        assert_eq!(rows, [("D-API-1", 2, 1, 1, 0), ("UNPLANNED", 1, 0, 0, 1)]);
        assert_eq!(c[0].functions(), 2);
        // A family without a map row is refused, never dropped.
        assert_eq!(by_deliverable(&ops, &map[..1]), Err("crm-new".to_string()));
    }

    #[test]
    fn map_rows_are_complete_and_unique() {
        assert_eq!(parse_map("a\tD-X-1\n"), Err(1));
        assert_eq!(parse_map("a\t\tnote\n"), Err(1));
        assert_eq!(parse_map("a\tD-X-1\tn\na\tD-X-2\tn\n"), Err(2));
        assert_eq!(
            parse_map("a\tD-X-1\tn\nb\tD-X-1\tn\n").map(|m| m.len()),
            Ok(2)
        );
    }

    /// The committed map covers every family of the committed surface, names
    /// only families that exist, and only D-ids on the status board.
    #[test]
    fn the_committed_map_matches_the_surface_and_the_board() {
        let ops = parse_tsv(include_str!("../../../data/hubspot/api_surface.tsv")).unwrap();
        let map = parse_map(include_str!("../../../data/hubspot/surface_map.tsv")).unwrap();
        let counts = by_deliverable(&ops, &map).expect("every family is mapped");
        assert_eq!(
            counts
                .iter()
                .map(DeliverableCount::functions)
                .sum::<usize>(),
            1518
        );
        let families: std::collections::BTreeSet<String> = ops
            .iter()
            .map(|o| family(o.specs.first().map_or("", String::as_str)))
            .collect();
        for row in &map {
            assert!(
                families.contains(&row.family),
                "stale map row {}",
                row.family
            );
        }
        let board = include_str!("../../../.claude/board/STATUS_BOARD.md");
        for row in &map {
            if row.deliverable != UNPLANNED {
                assert!(
                    board.contains(&format!("| {} |", row.deliverable)),
                    "{} is not on the status board",
                    row.deliverable
                );
            }
        }
    }

    #[test]
    fn tsv_round_trips() {
        let ops = vec![
            op("legacy", "crm", "GET", "/a"),
            op("2026-09", "cms", "POST", "/b"),
        ];
        assert_eq!(parse_tsv(&to_tsv(&ops)), Ok(ops));
        assert_eq!(parse_tsv("legacy\tcrm\tGET\n"), Err(1));
    }

    #[test]
    fn tsv_has_seven_columns_and_no_stray_tabs() {
        let op = Operation {
            version: "legacy".into(),
            area: "crm".into(),
            specs: vec!["v3/a.json".into(), "v3/b.json".into()],
            method: "GET".into(),
            path: "/a".into(),
            title: "two\twords".into(),
            href: "/h".into(),
        };
        assert_eq!(
            to_tsv(&[op]),
            "legacy\tcrm\tGET\t/a\tv3/a.json,v3/b.json\ttwo words\t/h\n"
        );
    }
}
