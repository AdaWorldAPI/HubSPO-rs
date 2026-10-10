//! Reads one HubSpot OpenAPI document into a [`SpecSummary`]: which object
//! types it serves, which operations each supports (classified into
//! [`Verb`]s), and the string enums its schemas declare (e.g. the search
//! `Filter.operator` set).
//!
//! The verbs are the METHOD axis of the mint gate: a concept that HubSpot
//! creates, reads, searches and merges through its own endpoints has a
//! service family operating on it. They are classified from the path shape,
//! not from `operationId` strings, which HubSpot formats inconsistently.

use std::collections::BTreeMap;

use serde::Serialize;
use serde_json::Value;

const HTTP_METHODS: [&str; 5] = ["get", "post", "put", "patch", "delete"];

/// What an operation does to its object type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Verb {
    List,
    Create,
    Read,
    Update,
    Archive,
    BatchCreate,
    BatchRead,
    BatchUpdate,
    BatchUpsert,
    BatchArchive,
    Search,
    Merge,
    GdprDelete,
    ReadAssociations,
    Associate,
    Dissociate,
    /// A path shape this classifier does not know. Counted, never guessed.
    Other,
}

/// One HTTP operation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Operation {
    pub method: String,
    pub path: String,
    /// The object type named in the path (`contacts`), or `None` when the
    /// path is generic (`{objectType}`) or not an `/objects/` path.
    pub object_type: Option<String>,
    pub verb: Verb,
}

/// The parts of one OpenAPI document the harvest uses.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SpecSummary {
    pub title: String,
    pub version: String,
    pub operations: Vec<Operation>,
    /// `"Schema.property"` → declared string enum values, sorted by key.
    pub enums: BTreeMap<String, Vec<String>>,
}

/// Why a document could not be summarised.
#[derive(Debug)]
pub enum SpecError {
    Json(serde_json::Error),
    Missing(&'static str),
}

impl std::fmt::Display for SpecError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SpecError::Json(e) => write!(f, "invalid JSON: {e}"),
            SpecError::Missing(what) => write!(f, "OpenAPI document has no `{what}`"),
        }
    }
}

impl std::error::Error for SpecError {}

impl SpecSummary {
    pub fn from_json(json: &str) -> Result<Self, SpecError> {
        let doc: Value = serde_json::from_str(json).map_err(SpecError::Json)?;
        let info = doc.get("info").ok_or(SpecError::Missing("info"))?;
        let paths = doc
            .get("paths")
            .and_then(Value::as_object)
            .ok_or(SpecError::Missing("paths"))?;

        let mut operations = Vec::new();
        for (path, item) in paths {
            let Some(item) = item.as_object() else {
                continue;
            };
            for method in HTTP_METHODS {
                if item.contains_key(method) {
                    let (object_type, verb) = classify(method, path);
                    operations.push(Operation {
                        method: method.to_uppercase(),
                        path: path.clone(),
                        object_type,
                        verb,
                    });
                }
            }
        }
        operations.sort_by(|a, b| (&a.path, &a.method).cmp(&(&b.path, &b.method)));

        let mut enums = BTreeMap::new();
        if let Some(schemas) = doc
            .pointer("/components/schemas")
            .and_then(Value::as_object)
        {
            for (schema, body) in schemas {
                let Some(props) = body.get("properties").and_then(Value::as_object) else {
                    continue;
                };
                for (prop, spec) in props {
                    let values = spec
                        .get("enum")
                        .or_else(|| spec.pointer("/items/enum"))
                        .and_then(Value::as_array);
                    if let Some(values) = values {
                        let mut values: Vec<String> = values
                            .iter()
                            .filter_map(|v| v.as_str().map(str::to_string))
                            .collect();
                        values.sort();
                        enums.insert(format!("{schema}.{prop}"), values);
                    }
                }
            }
        }

        let text = |key| {
            info.get(key)
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string()
        };
        Ok(SpecSummary {
            title: text("title"),
            version: text("version"),
            operations,
            enums,
        })
    }
}

/// True for the version segment that sits between `objects` and the type:
/// `v3`, `v4` (older releases) or `2026-03` (date releases).
fn is_version_segment(seg: &str) -> bool {
    let numeric = seg
        .strip_prefix('v')
        .is_some_and(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()));
    let dated = seg.len() == 7
        && seg.as_bytes()[4] == b'-'
        && seg[..4].chars().all(|c| c.is_ascii_digit())
        && seg[5..].chars().all(|c| c.is_ascii_digit());
    numeric || dated
}

fn is_param(seg: &str) -> bool {
    seg.starts_with('{') && seg.ends_with('}')
}

/// Classifies an operation by its method and the path shape after
/// `/objects/[version/]<type>`.
pub fn classify(method: &str, path: &str) -> (Option<String>, Verb) {
    let segs: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    let Some(objects) = segs.iter().position(|s| *s == "objects") else {
        return (None, Verb::Other);
    };
    let mut i = objects + 1;
    // `/crm/objects/2026-03/contacts`: the version follows `objects`.
    if segs.get(i).is_some_and(|s| is_version_segment(s)) {
        i += 1;
    }
    let Some(type_seg) = segs.get(i) else {
        return (None, Verb::Other);
    };
    let object_type = (!is_param(type_seg)).then(|| type_seg.to_string());
    let tail = &segs[i + 1..];
    let method = method.to_ascii_lowercase();

    let verb = match (method.as_str(), tail) {
        ("get", []) => Verb::List,
        ("post", []) => Verb::Create,
        ("get", [id]) if is_param(id) => Verb::Read,
        ("patch" | "put", [id]) if is_param(id) => Verb::Update,
        ("delete", [id]) if is_param(id) => Verb::Archive,
        ("post", ["batch", op]) => match *op {
            "create" => Verb::BatchCreate,
            "read" => Verb::BatchRead,
            "update" => Verb::BatchUpdate,
            "upsert" => Verb::BatchUpsert,
            "archive" => Verb::BatchArchive,
            _ => Verb::Other,
        },
        ("post", ["search"]) => Verb::Search,
        ("post", ["merge"]) => Verb::Merge,
        ("post", ["gdpr-delete"]) => Verb::GdprDelete,
        (m, [id, "associations", rest @ ..]) if is_param(id) => match (m, rest.len()) {
            ("get", 1) => Verb::ReadAssociations,
            ("put", _) => Verb::Associate,
            ("delete", _) => Verb::Dissociate,
            _ => Verb::Other,
        },
        _ => Verb::Other,
    };
    (object_type, verb)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_path_generations_name_the_same_object_type() {
        for path in ["/crm/v3/objects/contacts", "/crm/objects/2026-03/contacts"] {
            assert_eq!(classify("GET", path), (Some("contacts".into()), Verb::List));
        }
    }

    #[test]
    fn every_known_shape_gets_its_verb() {
        let base = "/crm/objects/2026-03/deals";
        let cases = [
            ("POST", base.to_string(), Verb::Create),
            ("GET", format!("{base}/{{dealId}}"), Verb::Read),
            ("PATCH", format!("{base}/{{dealId}}"), Verb::Update),
            ("DELETE", format!("{base}/{{dealId}}"), Verb::Archive),
            ("POST", format!("{base}/batch/create"), Verb::BatchCreate),
            ("POST", format!("{base}/batch/read"), Verb::BatchRead),
            ("POST", format!("{base}/batch/update"), Verb::BatchUpdate),
            ("POST", format!("{base}/batch/upsert"), Verb::BatchUpsert),
            ("POST", format!("{base}/batch/archive"), Verb::BatchArchive),
            ("POST", format!("{base}/search"), Verb::Search),
            ("POST", format!("{base}/merge"), Verb::Merge),
            ("POST", format!("{base}/gdpr-delete"), Verb::GdprDelete),
            (
                "GET",
                format!("{base}/{{dealId}}/associations/{{toObjectType}}"),
                Verb::ReadAssociations,
            ),
            (
                "PUT",
                format!("{base}/{{dealId}}/associations/{{toObjectType}}/{{toObjectId}}"),
                Verb::Associate,
            ),
            (
                "DELETE",
                format!("{base}/{{dealId}}/associations/{{toObjectType}}/{{toObjectId}}"),
                Verb::Dissociate,
            ),
        ];
        for (method, path, want) in cases {
            assert_eq!(
                classify(method, &path),
                (Some("deals".into()), want),
                "{method} {path}"
            );
        }
    }

    #[test]
    fn generic_and_unknown_shapes_are_not_guessed() {
        // Generic association path: verb known, object type deliberately absent.
        assert_eq!(
            classify(
                "PUT",
                "/crm/objects/2026-03/{objectType}/{objectId}/associations/{toObjectType}/{toObjectId}"
            ),
            (None, Verb::Associate)
        );
        // Not an /objects/ path at all.
        assert_eq!(
            classify("GET", "/crm/v3/pipelines/{objectType}"),
            (None, Verb::Other)
        );
        // An /objects/ path whose tail is unknown.
        assert_eq!(
            classify("POST", "/crm/v3/objects/contacts/batch/frobnicate"),
            (Some("contacts".into()), Verb::Other)
        );
        assert_eq!(
            classify("GET", "/crm/v3/objects/contacts/search"),
            (Some("contacts".into()), Verb::Other),
            "a literal segment is not an id; only POST /search is Search"
        );
    }

    #[test]
    fn a_synthetic_document_is_summarised() {
        let doc = r#"{
          "openapi": "3.0.1",
          "info": {"title": "Widgets", "version": "2026-03"},
          "paths": {
            "/crm/objects/2026-03/widgets": {"get": {}, "post": {}, "parameters": []},
            "/crm/objects/2026-03/widgets/search": {"post": {}}
          },
          "components": {"schemas": {
            "Filter": {"properties": {
              "operator": {"type": "string", "enum": ["NEQ", "EQ"]},
              "propertyName": {"type": "string"}
            }},
            "Spec": {"properties": {
              "kinds": {"type": "array", "items": {"enum": ["B", "A"]}}
            }}
          }}
        }"#;
        let s = SpecSummary::from_json(doc).expect("valid");
        assert_eq!(s.title, "Widgets");
        assert_eq!(s.version, "2026-03");
        let verbs: Vec<Verb> = s.operations.iter().map(|o| o.verb).collect();
        assert_eq!(verbs, vec![Verb::List, Verb::Create, Verb::Search]);
        assert_eq!(s.enums.len(), 2, "{:?}", s.enums);
        assert_eq!(s.enums["Filter.operator"], vec!["EQ", "NEQ"]);
        assert_eq!(s.enums["Spec.kinds"], vec!["A", "B"]);
    }

    #[test]
    fn a_document_without_paths_is_an_error_not_an_empty_summary() {
        let err = SpecSummary::from_json(r#"{"info": {"title": "x"}}"#).unwrap_err();
        assert!(matches!(err, SpecError::Missing("paths")), "{err}");
    }
}
