//! Aggregates the selected specs into one [`HarvestReport`]: per API what was
//! read, and per object type which verbs HubSpot exposes for it across every
//! API that names it.
//!
//! A selected spec that is absent from the cache is listed in
//! [`HarvestReport::missing`], never skipped silently: a report that quietly
//! covers fewer APIs reads exactly like a smaller HubSpot.

use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;

use crate::catalog::{ApiEntry, Catalog, VersionEntry};
use crate::openapi::{Operation, SpecSummary, Verb};

/// One harvested API.
#[derive(Debug, Clone, Serialize)]
pub struct ApiReport {
    pub group: String,
    pub name: String,
    pub slug: String,
    pub version: String,
    pub stage: String,
    pub title: String,
    pub operations: Vec<Operation>,
    pub enums: BTreeMap<String, Vec<String>>,
}

/// Everything HubSpot exposes for one object type.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct ObjectReport {
    /// Slugs of the APIs whose paths name this type.
    pub apis: BTreeSet<String>,
    pub verbs: BTreeSet<Verb>,
}

/// The whole harvest.
#[derive(Debug, Clone, Serialize)]
pub struct HarvestReport {
    /// APIs in the catalog.
    pub catalog_apis: usize,
    /// APIs with a selectable version (see `catalog` for the policy).
    pub selected: usize,
    pub apis: Vec<ApiReport>,
    pub object_types: BTreeMap<String, ObjectReport>,
    /// HubSpot names some object types only by numeric type id in paths
    /// (`/crm/objects/2026-03/0-3` is Deals). Each id maps to the APIs that
    /// use it, so the name is read from the data, never from a hand table.
    pub object_type_ids: BTreeMap<String, BTreeSet<String>>,
    /// `slug@version` of selected specs that could not be read, with why.
    pub missing: Vec<String>,
    /// Operations on an `/objects/` path whose shape the classifier did not
    /// know. A rising count is a new HubSpot path shape to classify.
    pub unclassified_object_operations: usize,
    /// Operations outside `/objects/` (pipelines, properties, CMS, …). Not
    /// classified into verbs here; counted so they are not mistaken for gaps.
    pub operations_outside_objects: usize,
}

/// True for HubSpot's numeric object type ids such as `0-3` or `2-1234`.
pub fn is_type_id(segment: &str) -> bool {
    segment.split_once('-').is_some_and(|(a, b)| {
        !a.is_empty()
            && !b.is_empty()
            && a.chars().all(|c| c.is_ascii_digit())
            && b.chars().all(|c| c.is_ascii_digit())
    })
}

/// Builds the report. `load` returns the OpenAPI text for a selected version,
/// or an error message when it is unavailable.
pub fn build<F>(catalog: &Catalog, mut load: F) -> HarvestReport
where
    F: FnMut(&ApiEntry, &VersionEntry) -> Result<String, String>,
{
    let mut apis = Vec::new();
    let mut missing = Vec::new();
    let mut selected = 0;
    for api in &catalog.results {
        let Some(version) = api.select() else {
            continue;
        };
        selected += 1;
        let tag = format!("{}@{}", api.slug(), version.version);
        let summary = load(api, version)
            .and_then(|text| SpecSummary::from_json(&text).map_err(|e| e.to_string()));
        match summary {
            Ok(s) => apis.push(ApiReport {
                group: api.group.clone(),
                name: api.name.clone(),
                slug: api.slug(),
                version: version.version.clone(),
                stage: version.stage.clone(),
                title: s.title,
                operations: s.operations,
                enums: s.enums,
            }),
            Err(why) => missing.push(format!("{tag}: {why}")),
        }
    }

    let mut object_types: BTreeMap<String, ObjectReport> = BTreeMap::new();
    let mut object_type_ids: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut unclassified_object_operations = 0;
    let mut operations_outside_objects = 0;
    for api in &apis {
        for op in &api.operations {
            let on_objects = op.path.split('/').any(|s| s == "objects");
            if !on_objects {
                operations_outside_objects += 1;
            } else if op.verb == Verb::Other {
                unclassified_object_operations += 1;
            }
            if let Some(ty) = &op.object_type {
                if is_type_id(ty) {
                    object_type_ids
                        .entry(ty.clone())
                        .or_default()
                        .insert(api.slug.clone());
                }
                let entry = object_types.entry(ty.clone()).or_default();
                entry.apis.insert(api.slug.clone());
                if op.verb != Verb::Other {
                    entry.verbs.insert(op.verb);
                }
            }
        }
    }

    HarvestReport {
        catalog_apis: catalog.results.len(),
        selected,
        apis,
        object_types,
        object_type_ids,
        missing,
        unclassified_object_operations,
        operations_outside_objects,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CATALOG: &str = r#"{"results": [
      {"name": "Widgets", "group": "CRM", "versions": [
        {"version": "3", "stage": "STABLE", "openApi": "u1"},
        {"version": "2026-03", "stage": "STABLE", "openApi": "u2"}]},
      {"name": "Gadgets", "group": "CRM", "versions": [
        {"version": "2026-03", "stage": "STABLE", "openApi": "u3"}]},
      {"name": "Previewed", "group": "CRM", "versions": [
        {"version": "2026-09", "stage": "DEVELOPER_PREVIEW", "openApi": "u4"}]}
    ]}"#;

    const WIDGETS: &str = r#"{"info": {"title": "Widgets", "version": "2026-03"},
      "paths": {
        "/crm/objects/2026-03/widgets": {"get": {}, "post": {}},
        "/crm/objects/2026-03/widgets/merge": {"post": {}},
        "/crm/objects/2026-03/widgets/oddity": {"post": {}},
        "/crm/objects/2026-03/0-7/batch/read": {"post": {}},
        "/crm/v3/pipelines/widgets": {"get": {}}
      }}"#;

    #[test]
    fn selected_specs_are_aggregated_per_object_type() {
        let catalog = Catalog::parse(CATALOG).unwrap();
        let mut asked = Vec::new();
        let report = build(&catalog, |api, v| {
            asked.push(format!("{}@{}", api.slug(), v.version));
            match api.name.as_str() {
                "Widgets" => Ok(WIDGETS.to_string()),
                _ => Err("not in cache".to_string()),
            }
        });
        // The preview-only API is never asked for; the dated stable release is.
        assert_eq!(asked, vec!["crm-widgets@2026-03", "crm-gadgets@2026-03"]);
        assert_eq!(report.catalog_apis, 3);
        assert_eq!(report.selected, 2);
        assert_eq!(report.apis.len(), 1);
        assert_eq!(report.missing, vec!["crm-gadgets@2026-03: not in cache"]);
        let widgets = &report.object_types["widgets"];
        assert_eq!(
            widgets.verbs.iter().copied().collect::<Vec<_>>(),
            vec![Verb::List, Verb::Create, Verb::Merge]
        );
        assert_eq!(report.unclassified_object_operations, 1, "the /oddity POST");
        assert_eq!(report.operations_outside_objects, 1, "the pipelines GET");
        assert_eq!(
            report.object_type_ids["0-7"].iter().collect::<Vec<_>>(),
            vec!["crm-widgets"]
        );
        assert_eq!(report.object_type_ids.len(), 1, "names are not type ids");
    }

    #[test]
    fn type_ids_are_recognised_and_names_are_not() {
        for id in ["0-3", "2-1234", "0-410"] {
            assert!(is_type_id(id), "{id}");
        }
        for name in ["deals", "line_items", "0-", "-3", "a-3", "0-3x", "2026-03x"] {
            assert!(!is_type_id(name), "{name}");
        }
    }

    #[test]
    fn an_unparseable_spec_is_reported_missing_not_dropped() {
        let catalog = Catalog::parse(CATALOG).unwrap();
        let report = build(&catalog, |_, _| Ok("{not json".to_string()));
        assert!(report.apis.is_empty());
        assert_eq!(report.missing.len(), 2, "{:?}", report.missing);
        assert!(
            report.missing[0].contains("invalid JSON"),
            "{:?}",
            report.missing
        );
    }
}
