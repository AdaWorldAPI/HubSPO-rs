//! HubSpot's per-object "object definition" reference pages
//! (`developers.hubspot.com/docs/api-reference/latest/crm/objects/<type>/object-definition`)
//! and the activity API guides (`.../crm/activities/<type>/guide.md`).
//!
//! An object-definition page embeds the object's default properties as the
//! properties API returns them (`name`, `label`, `type`, `fieldType`,
//! `groupName`, `hidden`, `hubspotDefined`, `dataSensitivity`, `options`, …),
//! inside the page's script data. That is the source of D-ONT-6's position
//! tables: internal names and types, not display labels.
//!
//! The data sits in a JavaScript string, so it is not plain JSON: quotes are
//! escaped (`\"`), and a string value is either a template literal
//! (`` `text` ``) or an escaped-quote string (`\"text\"`, used when the text
//! itself holds a backtick). [`definitions`] rewrites each embedded object to
//! JSON and parses it with `serde_json`; nothing else of the page is kept.
//!
//! Activities (calls, meetings, tasks, notes, emails) have no
//! object-definition page. Their API guides list the properties the API
//! writes in a markdown table, by internal name only; [`guide_names`] reads
//! those.

use serde::Deserialize;

/// One default property as the page defines it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PropertyDef {
    /// Internal name: the key the API and filters use.
    pub name: String,
    pub label: String,
    /// `string`, `number`, `datetime`, `date`, `bool`, `enumeration`, …
    #[serde(rename = "type")]
    pub kind: String,
    pub field_type: String,
    pub group_name: String,
    #[serde(default)]
    pub hidden: bool,
    #[serde(default)]
    pub hubspot_defined: bool,
    #[serde(default)]
    pub data_sensitivity: Option<String>,
}

/// Why a page could not be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ObjdefError {
    /// The embedded data ends before its object closes.
    Truncated { at: usize },
    /// An embedded object did not parse as a property definition.
    Json { at: usize, error: String },
}

const MARKER: &str = "\\\"json\\\":{";

/// Every property definition embedded in an object-definition page, in page
/// order. A page whose data cannot be read is an error, never a shorter list.
///
/// # Errors
///
/// [`ObjdefError`] at the byte offset of the first unreadable definition.
pub fn definitions(html: &str) -> Result<Vec<PropertyDef>, ObjdefError> {
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(i) = html[from..].find(MARKER) {
        let start = from + i + MARKER.len() - 1;
        let (json, end) = to_json(html, start)?;
        let def = serde_json::from_str(&json).map_err(|e| ObjdefError::Json {
            at: start,
            error: e.to_string(),
        })?;
        out.push(def);
        from = end;
    }
    Ok(out)
}

/// Rewrite the JavaScript-embedded object starting at `start` (`{`) into
/// JSON. Returns the JSON and the offset just past the object.
fn to_json(s: &str, start: usize) -> Result<(String, usize), ObjdefError> {
    let b = s.as_bytes();
    let mut out = String::new();
    let mut depth = 0usize;
    let mut j = start;
    loop {
        let c = *b.get(j).ok_or(ObjdefError::Truncated { at: start })?;
        if c == b'`' {
            // Template literal; `\`` is an escaped backtick inside it.
            let mut text = String::new();
            let mut k = j + 1;
            loop {
                match b.get(k) {
                    None => return Err(ObjdefError::Truncated { at: start }),
                    Some(b'`') => break,
                    Some(b'\\') if b.get(k + 1) == Some(&b'`') => {
                        text.push('`');
                        k += 2;
                    }
                    Some(_) => {
                        let ch = s[k..]
                            .chars()
                            .next()
                            .ok_or(ObjdefError::Truncated { at: start })?;
                        text.push(ch);
                        k += ch.len_utf8();
                    }
                }
            }
            out.push_str(&quote(&text));
            j = k + 1;
        } else if s[j..].starts_with("\\\"") {
            // Escaped-quote string: runs to the next `\"`.
            let rest = &s[j + 2..];
            let end = rest
                .find("\\\"")
                .ok_or(ObjdefError::Truncated { at: start })?;
            out.push_str(&quote(&rest[..end].replace("\\'", "'")));
            j += 2 + end + 2;
        } else {
            match c {
                b'{' | b'[' => depth += 1,
                b'}' | b']' => depth = depth.saturating_sub(1),
                _ => {}
            }
            let ch = s[j..]
                .chars()
                .next()
                .ok_or(ObjdefError::Truncated { at: start })?;
            out.push(ch);
            j += ch.len_utf8();
            if depth == 0 {
                return Ok((out, j));
            }
        }
    }
}

fn quote(text: &str) -> String {
    serde_json::Value::String(text.to_string()).to_string()
}

/// Internal names listed in an activity guide's property tables, in page
/// order, each name once.
///
/// A guide holds other tables too: request parameters (`limit`,
/// `properties`), the associations object (`to`, `types`), path fields
/// (`callId`), enum values. A table counts only if its header's first cell
/// is `Field` and every row names a property (`hs_*` or `hubspot_*`).
#[must_use]
pub fn guide_names(markdown: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut lines = markdown.lines().peekable();
    while let Some(line) = lines.next() {
        if !line.starts_with('|') {
            continue;
        }
        let header_is_field = line
            .trim_start_matches('|')
            .trim_start()
            .starts_with("Field ");
        let mut rows = Vec::new();
        let mut property_table = header_is_field;
        while let Some(next) = lines.next_if(|l| l.starts_with('|')) {
            if next.starts_with("| -") || next.starts_with("|-") || next.starts_with("| :") {
                continue;
            }
            let name = next
                .strip_prefix("| `")
                .and_then(|r| r.split_once("` |"))
                .map(|(n, _)| n);
            match name {
                Some(n) if (n.starts_with("hs_") || n.starts_with("hubspot_")) && is_name(n) => {
                    rows.push(n);
                }
                _ => property_table = false,
            }
        }
        if property_table {
            for n in rows {
                if !out.iter().any(|o| o == n) {
                    out.push(n.to_string());
                }
            }
        }
    }
    out
}

fn is_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAGE: &str = concat!(
        r#"<script>self.__next_f.push([1,"{\"properties\":[{\"name\":`email`,"#,
        r#"\"json\":{\"name\":`email`,\"label\":`Email`,\"type\":`string`,\"fieldType\":`text`,"#,
        r#"\"description\":`A contact's email`,\"groupName\":`contactinformation`,\"options\":[],"#,
        r#"\"hidden\":false,\"hubspotDefined\":true,\"dataSensitivity\":`non_sensitive`}},"#,
        r#"{\"json\":{\"name\":`hs_lead_is_open`,\"label\":`Open leads`,\"type\":`number`,"#,
        r#"\"fieldType\":`number`,\"description\":\"Whether the stage is not `QUALIFIED` or {x}\","#,
        r#"\"groupName\":`lead_information`,\"options\":[{\"label\":`A \`b\``,\"value\":`a`}],"#,
        r#"\"hidden\":true,\"hubspotDefined\":true}}]}"])</script>"#
    );

    #[test]
    fn both_string_forms_and_nested_options_parse() {
        let defs = definitions(PAGE).expect("page parses");
        let names: Vec<&str> = defs.iter().map(|d| d.name.as_str()).collect();
        assert_eq!(names, ["email", "hs_lead_is_open"]);
        assert_eq!(defs[0].kind, "string");
        assert_eq!(defs[0].field_type, "text");
        assert_eq!(defs[0].group_name, "contactinformation");
        assert_eq!(defs[0].data_sensitivity.as_deref(), Some("non_sensitive"));
        assert!(!defs[0].hidden);
        assert_eq!(defs[1].label, "Open leads");
        assert!(defs[1].hidden);
        assert_eq!(defs[1].data_sensitivity, None);
    }

    #[test]
    fn a_damaged_definition_is_an_error_not_a_shorter_list() {
        let cut = &PAGE[..PAGE.find("hs_lead_is_open").unwrap() + 40];
        assert!(matches!(
            definitions(cut),
            Err(ObjdefError::Truncated { .. })
        ));
        let bad = PAGE.replace("\\\"type\\\":`number`", "\\\"type\\\":7");
        assert!(matches!(definitions(&bad), Err(ObjdefError::Json { .. })));
    }

    #[test]
    fn a_page_without_definitions_is_empty() {
        assert_eq!(definitions("<html><p>no data</p></html>"), Ok(vec![]));
    }

    #[test]
    fn guide_property_tables_give_names_once_in_order() {
        let md = "| Field | Description |\n| - | - |\n\
            | `hs_timestamp` | Required. |\n| `hs_call_body` | The `body`. |\n\
            \ntext\n\
            | Field | Fieldtype | Description |\n| - | - | - |\n\
            | `hubspot_owner_id` | x | y |\n| `hs_timestamp` | again | z |\n\
            \n| Field | Description |\n| - | - |\n| `to` | record |\n| `types` | t |\n\
            \n| Field | Description |\n| - | - |\n| `hs_x` | a |\n| `callId` | b |\n\
            \n| Parameter | Description |\n| - | - |\n| `hs_limit` | n |\n";
        assert_eq!(
            guide_names(md),
            ["hs_timestamp", "hs_call_body", "hubspot_owner_id"]
        );
    }
}
