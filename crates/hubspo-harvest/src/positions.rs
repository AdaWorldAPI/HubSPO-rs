//! The standard property position table (D-ONT-6): which field position each
//! standard property of an object type holds, and the Rust it generates into
//! `hubspo-generated`.
//!
//! The table is a committed TSV, `object <TAB> position <TAB> section <TAB>
//! label`, one row per property. Positions are **append-only**: a merge adds
//! a label it has not seen at the next free position of its object and never
//! moves or removes a row, so a position stored in a field mask keeps its
//! meaning across harvests. A label that disappears from HubSpot's pages
//! keeps its position.
//!
//! Provisional (operator decision 2026-10-20): the rows come from HubSpot's
//! public default-property pages (`kb`), so a property is identified by its
//! display label, not by HubSpot's internal name. A label HubSpot renames
//! arrives as a new row. The portal harvest (`ISS-ORACLE-PORTAL`) adds the
//! internal names and types to the same positions.

use std::fmt::Write as _;

/// Every object type the table may hold, in the order the generated code
/// lists them. A row naming another object is refused.
pub const OBJECTS: &[&str] = &[
    "contact",
    "company",
    "deal",
    "ticket",
    "lead",
    "product",
    "line_item",
    "call",
    "meeting",
    "task",
    "note",
    "email",
    "communication",
    "postal_mail",
];

/// The widest table a `WideFieldMask` can address: positions are `u8`.
pub const MAX_POSITIONS: usize = 256;

/// One row of the table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub object: String,
    pub position: usize,
    pub section: String,
    pub label: String,
}

/// Why a table or a merge was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TableError {
    /// A line does not have four tab-separated fields.
    Malformed { line: usize },
    /// The position is not a number.
    BadPosition { line: usize },
    /// The object is not in [`OBJECTS`].
    UnknownObject(String),
    /// An object's positions are not exactly `0..n` in file order.
    NotDense {
        object: String,
        expected: usize,
        got: usize,
    },
    /// A label appears twice for one object.
    DuplicateLabel { object: String, label: String },
    /// A label or section holds a tab or a newline.
    BadText { object: String, label: String },
    /// More than [`MAX_POSITIONS`] properties for one object.
    TooWide { object: String },
}

/// Read the table, checking every invariant the generated code relies on.
///
/// # Errors
///
/// The first [`TableError`] found.
pub fn parse(tsv: &str) -> Result<Vec<Row>, TableError> {
    let mut rows = Vec::new();
    for (i, line) in tsv.lines().enumerate() {
        if line.is_empty() {
            continue;
        }
        let f: Vec<&str> = line.split('\t').collect();
        let [object, position, section, label] = f[..] else {
            return Err(TableError::Malformed { line: i + 1 });
        };
        let position = position
            .parse()
            .map_err(|_| TableError::BadPosition { line: i + 1 })?;
        rows.push(Row {
            object: object.to_string(),
            position,
            section: section.to_string(),
            label: label.to_string(),
        });
    }
    check(&rows)?;
    Ok(rows)
}

fn check(rows: &[Row]) -> Result<(), TableError> {
    for &object in OBJECTS {
        let mine: Vec<&Row> = rows.iter().filter(|r| r.object == object).collect();
        if mine.len() > MAX_POSITIONS {
            return Err(TableError::TooWide {
                object: object.to_string(),
            });
        }
        for (expected, r) in mine.iter().enumerate() {
            if r.position != expected {
                return Err(TableError::NotDense {
                    object: object.to_string(),
                    expected,
                    got: r.position,
                });
            }
            if mine[..expected].iter().any(|p| p.label == r.label) {
                return Err(TableError::DuplicateLabel {
                    object: object.to_string(),
                    label: r.label.clone(),
                });
            }
        }
    }
    for r in rows {
        if !OBJECTS.contains(&r.object.as_str()) {
            return Err(TableError::UnknownObject(r.object.clone()));
        }
        if [&r.label, &r.section]
            .iter()
            .any(|t| t.contains(['\t', '\n', '\r']))
            || r.label.is_empty()
        {
            return Err(TableError::BadText {
                object: r.object.clone(),
                label: r.label.clone(),
            });
        }
    }
    Ok(())
}

/// Add `(section, label)` pairs to `object`, append-only: a label the object
/// already has keeps its row untouched; a new one takes the next position.
/// Repeated labels in `found` are added once. Rows are kept grouped by
/// object in [`OBJECTS`] order.
///
/// # Errors
///
/// [`TableError::UnknownObject`], or any invariant the result would break.
pub fn merge(
    rows: &[Row],
    object: &str,
    found: &[(String, String)],
) -> Result<Vec<Row>, TableError> {
    if !OBJECTS.contains(&object) {
        return Err(TableError::UnknownObject(object.to_string()));
    }
    let mut out = rows.to_vec();
    let mut next = rows.iter().filter(|r| r.object == object).count();
    for (section, label) in found {
        let known = out.iter().any(|r| r.object == object && r.label == *label);
        if !known {
            out.push(Row {
                object: object.to_string(),
                position: next,
                section: section.clone(),
                label: label.clone(),
            });
            next += 1;
        }
    }
    // Stable sort: file order inside an object is position order.
    out.sort_by_key(|r| OBJECTS.iter().position(|o| *o == r.object));
    check(&out)?;
    Ok(out)
}

/// The table as TSV, the inverse of [`parse`].
#[must_use]
pub fn to_tsv(rows: &[Row]) -> String {
    let mut s = String::new();
    for r in rows {
        let _ = writeln!(
            s,
            "{}\t{}\t{}\t{}",
            r.object, r.position, r.section, r.label
        );
    }
    s
}

/// The generated Rust for `hubspo-generated`: one `&[StandardProperty]` per
/// object type plus the index [`OBJECTS`] in the same order.
#[must_use]
pub fn emit(rows: &[Row]) -> String {
    let mut s = String::from(
        "// @generated by `hubspo-harvest emit` from `data/hubspot/standard_properties.tsv`.\n\
         // Do not edit: change the table and regenerate.\n\n",
    );
    s.push_str(
        "/// One standard property of a HubSpot object type: its field position,\n\
         /// the page section it is listed under, and its display label.\n\
         ///\n\
         /// Provisional: taken from HubSpot's public default-property pages, which\n\
         /// give labels only. Internal names and types arrive with the portal\n\
         /// harvest (`ISS-ORACLE-PORTAL`) at the same positions.\n\
         #[derive(Debug, Clone, Copy, PartialEq, Eq)]\n\
         pub struct StandardProperty {\n\
         \x20   /// Field position: the bit in the object's `WideFieldMask`.\n\
         \x20   pub position: u8,\n\
         \x20   /// Heading on the source page.\n\
         \x20   pub section: &'static str,\n\
         \x20   /// Display label.\n\
         \x20   pub label: &'static str,\n\
         }\n",
    );
    for &object in OBJECTS {
        let _ = write!(
            s,
            "\n/// Standard properties of `{object}`, by position.\npub const {}: &[StandardProperty] = &[\n",
            object.to_ascii_uppercase()
        );
        for r in rows.iter().filter(|r| r.object == object) {
            let _ = writeln!(
                s,
                "    StandardProperty {{ position: {}, section: {:?}, label: {:?} }},",
                r.position, r.section, r.label
            );
        }
        s.push_str("];\n");
    }
    s.push_str("\n/// Every object type's table, in a fixed order.\npub const OBJECTS: &[(&str, &[StandardProperty])] = &[\n");
    for &object in OBJECTS {
        let _ = writeln!(s, "    ({object:?}, {}),", object.to_ascii_uppercase());
    }
    s.push_str("];\n");
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn found(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs
            .iter()
            .map(|(s, l)| (s.to_string(), l.to_string()))
            .collect()
    }

    fn labels(rows: &[Row], object: &str) -> Vec<(usize, String)> {
        rows.iter()
            .filter(|r| r.object == object)
            .map(|r| (r.position, r.label.clone()))
            .collect()
    }

    #[test]
    fn a_merge_appends_new_labels_and_never_moves_old_ones() {
        let first = merge(&[], "deal", &found(&[("A", "Amount"), ("A", "Stage")])).unwrap();
        // HubSpot reorders the page, drops "Stage" and adds "Close date".
        let second = merge(
            &first,
            "deal",
            &found(&[("B", "Close date"), ("A", "Amount")]),
        )
        .unwrap();
        assert_eq!(
            labels(&second, "deal"),
            vec![
                (0, "Amount".to_string()),
                (1, "Stage".to_string()),
                (2, "Close date".to_string())
            ]
        );
        // The kept row keeps its section, not the page's new one.
        assert_eq!(second[0].section, "A");
    }

    #[test]
    fn a_repeated_label_on_one_page_is_added_once() {
        let rows = merge(&[], "call", &found(&[("C", "Type"), ("C", "Type")])).unwrap();
        assert_eq!(labels(&rows, "call"), vec![(0, "Type".to_string())]);
    }

    #[test]
    fn rows_stay_grouped_by_object_whatever_the_merge_order() {
        let a = merge(&[], "deal", &found(&[("", "Amount")])).unwrap();
        let b = merge(&a, "contact", &found(&[("", "Email")])).unwrap();
        let c = merge(&b, "deal", &found(&[("", "Stage")])).unwrap();
        let objects: Vec<&str> = c.iter().map(|r| r.object.as_str()).collect();
        assert_eq!(objects, ["contact", "deal", "deal"]);
        assert_eq!(parse(&to_tsv(&c)).unwrap(), c);
    }

    #[test]
    fn parse_refuses_what_the_generated_code_cannot_hold() {
        assert_eq!(
            parse("deal\t1\ts\tAmount\n"),
            Err(TableError::NotDense {
                object: "deal".into(),
                expected: 0,
                got: 1
            })
        );
        assert_eq!(
            parse("deal\t0\ts\tAmount\ndeal\t1\ts\tAmount\n"),
            Err(TableError::DuplicateLabel {
                object: "deal".into(),
                label: "Amount".into()
            })
        );
        assert_eq!(
            parse("quote\t0\ts\tAmount\n"),
            Err(TableError::UnknownObject("quote".into()))
        );
        assert_eq!(
            parse("deal\t0\tAmount\n"),
            Err(TableError::Malformed { line: 1 })
        );
        assert_eq!(
            parse("deal\tx\ts\tAmount\n"),
            Err(TableError::BadPosition { line: 1 })
        );
        let wide: String = (0..=MAX_POSITIONS)
            .map(|i| format!("deal\t{i}\ts\tP{i}\n"))
            .collect();
        assert_eq!(
            parse(&wide),
            Err(TableError::TooWide {
                object: "deal".into()
            })
        );
        let fits: String = (0..MAX_POSITIONS)
            .map(|i| format!("deal\t{i}\ts\tP{i}\n"))
            .collect();
        assert_eq!(parse(&fits).map(|r| r.len()), Ok(MAX_POSITIONS));
    }

    #[test]
    fn emit_lists_every_object_even_when_empty() {
        let rows = merge(&[], "deal", &found(&[("Deal info", "Amount \"net\"")])).unwrap();
        let rs = emit(&rows);
        assert!(rs.starts_with("// @generated"));
        assert!(rs.contains(
            "pub const DEAL: &[StandardProperty] = &[\n    StandardProperty { position: 0, section: \"Deal info\", label: \"Amount \\\"net\\\"\" },\n];"
        ));
        assert!(rs.contains("pub const CONTACT: &[StandardProperty] = &[\n];"));
        assert_eq!(rs.matches("    (\"").count(), OBJECTS.len());
    }

    /// The committed generated file is exactly what the committed table
    /// generates. Regenerate with
    /// `cargo run -p hubspo-harvest -- emit data/hubspot/standard_properties.tsv
    ///  > crates/hubspo-generated/src/standard_properties.rs`.
    #[test]
    fn the_committed_generated_file_matches_the_table() {
        let tsv = include_str!("../../../data/hubspot/standard_properties.tsv");
        let rows = parse(tsv).expect("committed table is valid");
        assert_eq!(
            emit(&rows),
            include_str!("../../hubspo-generated/src/standard_properties.rs"),
            "regenerate hubspo-generated/src/standard_properties.rs"
        );
        assert_eq!(to_tsv(&rows), tsv, "the table is not in canonical form");
    }
}
