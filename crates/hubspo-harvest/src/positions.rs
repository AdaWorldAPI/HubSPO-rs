//! The standard property position table (D-ONT-6): which field position each
//! standard property of an object type holds, and the Rust it generates into
//! `hubspo-generated`.
//!
//! The table is a committed TSV, one row per property:
//! `object <TAB> position <TAB> name <TAB> type <TAB> field_type <TAB> group
//! <TAB> sensitivity <TAB> hidden <TAB> label`. A property is identified by
//! its internal `name`, the key the API and filters use.
//!
//! Positions are **append-only**: a merge adds a name it has not seen at the
//! next free position of its object and never moves or removes a row, so a
//! position stored in a field mask keeps its meaning across harvests. A
//! property that disappears from HubSpot's pages keeps its position. A merge
//! refreshes the descriptive columns of a known name, but never blanks one.
//!
//! Sources (operator decision 2026-10-20: provisional, from public docs):
//! HubSpot's object-definition pages give every column ([`crate::objdef`]);
//! the activity guides give names only, so those rows carry empty
//! descriptive columns until a portal harvest (`ISS-ORACLE-PORTAL`) fills
//! them in.

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
    "appointment",
    "cart",
    "commerce_payment",
    "commerce_subscription",
    "discount",
    "feedback_submission",
    "fee",
    "goal",
    "invoice",
    "order",
];

/// The widest table the generated code can hold: positions are `u16`.
///
/// A `WideFieldMask` addresses only positions `0..256` (they are `u8`), and
/// HubSpot's contact object alone has more standard properties than that.
/// The table records every property; which positions a mask can reach is
/// the schema's concern (`hubspo-schema`), not this table's.
pub const MAX_POSITIONS: usize = 1 << 16;

/// The descriptive columns of one property. Empty strings mean "unknown".
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Property {
    pub name: String,
    pub kind: String,
    pub field_type: String,
    pub group: String,
    pub sensitivity: String,
    pub hidden: bool,
    pub label: String,
}

/// One row of the table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub object: String,
    pub position: usize,
    pub property: Property,
}

/// Why a table or a merge was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TableError {
    /// A line does not have nine tab-separated fields.
    Malformed { line: usize },
    /// The position is not a number, or `hidden` is not `true`/`false`.
    BadValue { line: usize },
    /// The object is not in [`OBJECTS`].
    UnknownObject(String),
    /// An object's positions are not exactly `0..n` in file order.
    NotDense {
        object: String,
        expected: usize,
        got: usize,
    },
    /// A name appears twice for one object.
    DuplicateName { object: String, name: String },
    /// A name is not `[a-z0-9_]+`, or a text column holds a tab or newline.
    BadText { object: String, name: String },
    /// More than [`MAX_POSITIONS`] properties for one object.
    TooWide { object: String },
}

/// Is `name` an internal property name: non-empty `[a-z0-9_]`?
#[must_use]
pub fn is_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
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
        let [
            object,
            position,
            name,
            kind,
            field_type,
            group,
            sensitivity,
            hidden,
            label,
        ] = f[..]
        else {
            return Err(TableError::Malformed { line: i + 1 });
        };
        let bad = || TableError::BadValue { line: i + 1 };
        let position = position.parse().map_err(|_| bad())?;
        let hidden = match hidden {
            "true" => true,
            "false" => false,
            _ => return Err(bad()),
        };
        rows.push(Row {
            object: object.to_string(),
            position,
            property: Property {
                name: name.to_string(),
                kind: kind.to_string(),
                field_type: field_type.to_string(),
                group: group.to_string(),
                sensitivity: sensitivity.to_string(),
                hidden,
                label: label.to_string(),
            },
        });
    }
    check(&rows)?;
    Ok(rows)
}

fn check(rows: &[Row]) -> Result<(), TableError> {
    for r in rows {
        if !OBJECTS.contains(&r.object.as_str()) {
            return Err(TableError::UnknownObject(r.object.clone()));
        }
        let p = &r.property;
        let texts = [&p.kind, &p.field_type, &p.group, &p.sensitivity, &p.label];
        if !is_name(&p.name) || texts.iter().any(|t| t.contains(['\t', '\n', '\r'])) {
            return Err(TableError::BadText {
                object: r.object.clone(),
                name: p.name.clone(),
            });
        }
    }
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
            if mine[..expected]
                .iter()
                .any(|p| p.property.name == r.property.name)
            {
                return Err(TableError::DuplicateName {
                    object: object.to_string(),
                    name: r.property.name.clone(),
                });
            }
        }
    }
    Ok(())
}

/// Add properties to `object`, append-only: a name the object already has
/// keeps its position, and its descriptive columns take the non-empty values
/// of `found` (a `hidden` of `true` is kept once seen); a new name takes the
/// next position. Repeated names in `found` are added once. Rows are kept
/// grouped by object in [`OBJECTS`] order.
///
/// # Errors
///
/// [`TableError::UnknownObject`], or any invariant the result would break.
pub fn merge(rows: &[Row], object: &str, found: &[Property]) -> Result<Vec<Row>, TableError> {
    if !OBJECTS.contains(&object) {
        return Err(TableError::UnknownObject(object.to_string()));
    }
    let mut out = rows.to_vec();
    let mut next = rows.iter().filter(|r| r.object == object).count();
    for p in found {
        if let Some(r) = out
            .iter_mut()
            .find(|r| r.object == object && r.property.name == p.name)
        {
            let q = &mut r.property;
            for (old, new) in [
                (&mut q.kind, &p.kind),
                (&mut q.field_type, &p.field_type),
                (&mut q.group, &p.group),
                (&mut q.sensitivity, &p.sensitivity),
                (&mut q.label, &p.label),
            ] {
                if !new.is_empty() {
                    old.clone_from(new);
                }
            }
            q.hidden |= p.hidden;
        } else {
            out.push(Row {
                object: object.to_string(),
                position: next,
                property: p.clone(),
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
        let p = &r.property;
        let _ = writeln!(
            s,
            "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
            r.object,
            r.position,
            p.name,
            p.kind,
            p.field_type,
            p.group,
            p.sensitivity,
            p.hidden,
            p.label
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
        "/// One standard property of a HubSpot object type, by field position.\n\
         ///\n\
         /// Provisional: taken from HubSpot's public object-definition pages and\n\
         /// activity guides. An empty string means the source did not say; the\n\
         /// activity guides give names only.\n\
         #[derive(Debug, Clone, Copy, PartialEq, Eq)]\n\
         pub struct StandardProperty {\n\
         \x20   /// Field position: the bit in the object's `WideFieldMask`, if below 256.\n\
         \x20   pub position: u16,\n\
         \x20   /// Internal name: the key the API and filters use.\n\
         \x20   pub name: &'static str,\n\
         \x20   /// Value type: `string`, `number`, `datetime`, `enumeration`, ...\n\
         \x20   pub kind: &'static str,\n\
         \x20   /// Form control: `text`, `select`, `date`, ...\n\
         \x20   pub field_type: &'static str,\n\
         \x20   /// Property group.\n\
         \x20   pub group: &'static str,\n\
         \x20   /// HubSpot's data-sensitivity class, e.g. `non_sensitive`.\n\
         \x20   pub sensitivity: &'static str,\n\
         \x20   /// Hidden from HubSpot's own forms and record views.\n\
         \x20   pub hidden: bool,\n\
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
            let p = &r.property;
            let _ = writeln!(
                s,
                "    StandardProperty {{ position: {}, name: {:?}, kind: {:?}, field_type: {:?}, group: {:?}, sensitivity: {:?}, hidden: {}, label: {:?} }},",
                r.position, p.name, p.kind, p.field_type, p.group, p.sensitivity, p.hidden, p.label
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

    fn prop(name: &str, kind: &str, label: &str) -> Property {
        Property {
            name: name.into(),
            kind: kind.into(),
            label: label.into(),
            ..Property::default()
        }
    }

    fn names(rows: &[Row], object: &str) -> Vec<(usize, String)> {
        rows.iter()
            .filter(|r| r.object == object)
            .map(|r| (r.position, r.property.name.clone()))
            .collect()
    }

    fn line(object: &str, pos: &str, name: &str) -> String {
        format!("{object}\t{pos}\t{name}\tstring\ttext\tg\tnon_sensitive\tfalse\tL\n")
    }

    #[test]
    fn a_merge_appends_new_names_and_never_moves_old_ones() {
        let first = merge(
            &[],
            "deal",
            &[
                prop("amount", "number", "Amount"),
                prop("dealstage", "", ""),
            ],
        )
        .unwrap();
        // HubSpot reorders the page, drops `dealstage` and adds `closedate`.
        let second = merge(
            &first,
            "deal",
            &[
                prop("closedate", "datetime", "Close date"),
                prop("amount", "", ""),
            ],
        )
        .unwrap();
        assert_eq!(
            names(&second, "deal"),
            vec![
                (0, "amount".to_string()),
                (1, "dealstage".to_string()),
                (2, "closedate".to_string())
            ]
        );
        // A source without the descriptive columns does not blank them.
        assert_eq!(second[0].property.kind, "number");
        assert_eq!(second[0].property.label, "Amount");
    }

    #[test]
    fn a_merge_fills_in_a_names_only_row() {
        let guide = merge(&[], "call", &[prop("hs_call_body", "", "")]).unwrap();
        let mut full = prop("hs_call_body", "string", "Call notes");
        full.hidden = true;
        let rows = merge(&guide, "call", &[full.clone()]).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].property, full);
    }

    #[test]
    fn a_repeated_name_on_one_page_is_added_once() {
        let rows = merge(&[], "call", &[prop("hs_x", "", ""), prop("hs_x", "", "")]).unwrap();
        assert_eq!(names(&rows, "call"), vec![(0, "hs_x".to_string())]);
    }

    #[test]
    fn rows_stay_grouped_by_object_whatever_the_merge_order() {
        let a = merge(&[], "deal", &[prop("amount", "", "")]).unwrap();
        let b = merge(&a, "contact", &[prop("email", "", "")]).unwrap();
        let c = merge(&b, "deal", &[prop("dealstage", "", "")]).unwrap();
        let objects: Vec<&str> = c.iter().map(|r| r.object.as_str()).collect();
        assert_eq!(objects, ["contact", "deal", "deal"]);
        assert_eq!(parse(&to_tsv(&c)).unwrap(), c);
    }

    #[test]
    fn parse_refuses_what_the_generated_code_cannot_hold() {
        assert_eq!(
            parse(&line("deal", "1", "amount")),
            Err(TableError::NotDense {
                object: "deal".into(),
                expected: 0,
                got: 1
            })
        );
        assert_eq!(
            parse(&(line("deal", "0", "amount") + &line("deal", "1", "amount"))),
            Err(TableError::DuplicateName {
                object: "deal".into(),
                name: "amount".into()
            })
        );
        assert_eq!(
            parse(&line("quote", "0", "amount")),
            Err(TableError::UnknownObject("quote".into()))
        );
        assert_eq!(
            parse(&line("deal", "0", "Amount")),
            Err(TableError::BadText {
                object: "deal".into(),
                name: "Amount".into()
            })
        );
        assert_eq!(
            parse("deal\t0\tamount\n"),
            Err(TableError::Malformed { line: 1 })
        );
        assert_eq!(
            parse(&line("deal", "x", "amount")),
            Err(TableError::BadValue { line: 1 })
        );
        assert_eq!(
            parse(&line("deal", "0", "amount").replace("false", "no")),
            Err(TableError::BadValue { line: 1 })
        );
        let wide: String = (0..=MAX_POSITIONS)
            .map(|i| line("deal", &i.to_string(), &format!("p{i}")))
            .collect();
        assert_eq!(
            parse(&wide),
            Err(TableError::TooWide {
                object: "deal".into()
            })
        );
        let fits: String = (0..MAX_POSITIONS)
            .map(|i| line("deal", &i.to_string(), &format!("p{i}")))
            .collect();
        assert_eq!(parse(&fits).map(|r| r.len()), Ok(MAX_POSITIONS));
    }

    #[test]
    fn emit_lists_every_object_even_when_empty() {
        let rows = merge(&[], "deal", &[prop("amount", "number", "Amount \"net\"")]).unwrap();
        let rs = emit(&rows);
        assert!(rs.starts_with("// @generated"));
        assert!(rs.contains(
            "pub const DEAL: &[StandardProperty] = &[\n    StandardProperty { position: 0, name: \"amount\", kind: \"number\", field_type: \"\", group: \"\", sensitivity: \"\", hidden: false, label: \"Amount \\\"net\\\"\" },\n];"
        ));
        assert!(rs.contains("pub const CONTACT: &[StandardProperty] = &[\n];"));
        assert_eq!(rs.matches("    (\"").count(), OBJECTS.len());
    }

    /// The committed generated file is exactly what the committed table
    /// generates. Regenerate with `scripts/fetch-hubspot-properties.sh`.
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
