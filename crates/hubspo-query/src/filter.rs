//! D-QRY-1: HubSpot filter groups compiled to one Quack [`Filter`].
//!
//! HubSpot's search filter is a list of *filter groups*: filters inside a
//! group are ANDed, groups are ORed. Values arrive as strings, as in
//! HubSpot's API. [`compile`] binds every property name and literal once
//! through a [`Binder`] (the catalog in `hubspo-store`) and returns a
//! numeric filter; nothing textual survives it. The caller lowers it with
//! `lance_graph_quack::lower` and runs it on mask-risc.
//!
//! ```text
//!   live ∧ (g₁ ∨ g₂ ∨ …),   gᵢ = f₁ ∧ f₂ ∧ …
//! ```
//!
//! ## Operators
//!
//! | operator | coded field (enumeration) | integer field |
//! |---|---|---|
//! | `EQ` / `NEQ` | code compare | `=` / `<>` |
//! | `LT` `LTE` `GT` `GTE` | refused | ordered compare |
//! | `BETWEEN` | refused | `value ≤ x ≤ highValue` |
//! | `IN` / `NOT_IN` | code set | integer set |
//! | `HAS_PROPERTY` / `NOT_HAS_PROPERTY` | validity plane | validity plane |
//! | `CONTAINS_TOKEN` / `NOT_CONTAINS_TOKEN` | refused: text search is the Tantivy index (H-ADR-6) | refused |
//!
//! ## Missing values
//!
//! A filter that reads a nullable property is SQL three-valued
//! (`Filter::sql_where`): a record with no value is kept by no comparison,
//! including `NEQ` and `NOT_IN`. Only `HAS_PROPERTY` / `NOT_HAS_PROPERTY` ask
//! about the missing value itself. Whether HubSpot keeps records with no
//! value under `NEQ` / `NOT_IN` is an open question for the value oracle
//! (`ISS-ORACLE-PORTAL`); the answer would change only this module.
//!
//! ## Literals
//!
//! - Coded fields: the literal must be one of the field's labels. An unknown
//!   label is an error, not an empty result: nothing is minted at bind time.
//! - Integer fields: a decimal `i32`. Decimals, millisecond timestamps and
//!   currency amounts need the lane encodings of D-ONT-6 (minor units, day
//!   numbers) and are refused here until then.

use lance_graph_quack::bind::{Binder, FieldKind};
use lance_graph_quack::{Cmp, Col, Filter, Mask};

/// A HubSpot filter operator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Operator {
    /// `EQ`
    Eq,
    /// `NEQ`
    Neq,
    /// `LT`
    Lt,
    /// `LTE`
    Lte,
    /// `GT`
    Gt,
    /// `GTE`
    Gte,
    /// `BETWEEN` (inclusive on both ends)
    Between,
    /// `IN`
    In,
    /// `NOT_IN`
    NotIn,
    /// `HAS_PROPERTY`
    HasProperty,
    /// `NOT_HAS_PROPERTY`
    NotHasProperty,
    /// `CONTAINS_TOKEN`
    ContainsToken,
    /// `NOT_CONTAINS_TOKEN`
    NotContainsToken,
}

impl Operator {
    /// The operator HubSpot's API spells `name`.
    pub fn from_hubspot(name: &str) -> Option<Self> {
        Some(match name {
            "EQ" => Self::Eq,
            "NEQ" => Self::Neq,
            "LT" => Self::Lt,
            "LTE" => Self::Lte,
            "GT" => Self::Gt,
            "GTE" => Self::Gte,
            "BETWEEN" => Self::Between,
            "IN" => Self::In,
            "NOT_IN" => Self::NotIn,
            "HAS_PROPERTY" => Self::HasProperty,
            "NOT_HAS_PROPERTY" => Self::NotHasProperty,
            "CONTAINS_TOKEN" => Self::ContainsToken,
            "NOT_CONTAINS_TOKEN" => Self::NotContainsToken,
            _ => return None,
        })
    }
}

/// One HubSpot filter: `propertyName`, `operator`, `value` / `highValue` /
/// `values`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PropertyFilter {
    /// `propertyName`.
    pub property: String,
    /// `operator`.
    pub operator: Operator,
    /// `value` (single-value operators and `BETWEEN`'s low end).
    pub value: Option<String>,
    /// `highValue` (`BETWEEN`'s high end).
    pub high_value: Option<String>,
    /// `values` (`IN` / `NOT_IN`).
    pub values: Vec<String>,
}

impl PropertyFilter {
    /// A filter with one value.
    pub fn new(property: &str, operator: Operator, value: &str) -> Self {
        Self {
            property: property.into(),
            operator,
            value: Some(value.into()),
            high_value: None,
            values: Vec::new(),
        }
    }

    /// `HAS_PROPERTY` / `NOT_HAS_PROPERTY`.
    pub fn presence(property: &str, operator: Operator) -> Self {
        Self {
            property: property.into(),
            operator,
            value: None,
            high_value: None,
            values: Vec::new(),
        }
    }

    /// `BETWEEN low AND high`.
    pub fn between(property: &str, low: &str, high: &str) -> Self {
        Self {
            property: property.into(),
            operator: Operator::Between,
            value: Some(low.into()),
            high_value: Some(high.into()),
            values: Vec::new(),
        }
    }

    /// `IN` / `NOT_IN` over `values`.
    pub fn set(property: &str, operator: Operator, values: &[&str]) -> Self {
        Self {
            property: property.into(),
            operator,
            value: None,
            high_value: None,
            values: values.iter().map(|v| (*v).to_string()).collect(),
        }
    }
}

/// A filter group: its filters are ANDed.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FilterGroup {
    /// `filters`.
    pub filters: Vec<PropertyFilter>,
}

/// Why a filter did not compile. Each names the HubSpot term that failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompileError {
    /// The object type has no table.
    UnknownObject(String),
    /// The object type has no such property.
    UnknownProperty(String),
    /// The property's domain has no such value.
    UnknownValue {
        /// Property.
        property: String,
        /// The literal.
        value: String,
    },
    /// The value is not a decimal `i32`.
    NotAnInteger {
        /// Property.
        property: String,
        /// The literal.
        value: String,
    },
    /// The operator does not apply to the property's kind.
    OperatorNotSupported {
        /// Property.
        property: String,
        /// Operator.
        operator: Operator,
    },
    /// Text search is not a Quack predicate; it is the Tantivy index.
    TextSearch {
        /// Property.
        property: String,
    },
    /// The operator needs a value (or `values`) that was not given.
    MissingValue {
        /// Property.
        property: String,
        /// Operator.
        operator: Operator,
    },
    /// A filter group with no filters.
    EmptyGroup,
}

/// Compile `groups` over the object type `object` into one numeric filter.
///
/// No groups means every live record.
///
/// # Errors
///
/// The first [`CompileError`]; nothing is minted on failure.
pub fn compile(
    object: &str,
    groups: &[FilterGroup],
    binder: &dyn Binder,
) -> Result<Filter, CompileError> {
    let table = binder
        .table(object)
        .ok_or_else(|| CompileError::UnknownObject(object.into()))?;
    let live_mask = binder.live(table);
    let live = Filter::plane(live_mask);
    if groups.is_empty() {
        return Ok(live);
    }
    let mut nullable: Vec<(Col, Mask)> = Vec::new();
    let mut alternatives = Vec::with_capacity(groups.len());
    for group in groups {
        if group.filters.is_empty() {
            return Err(CompileError::EmptyGroup);
        }
        let mut leaves = Vec::with_capacity(group.filters.len());
        for f in &group.filters {
            let field = binder
                .field(table, &f.property)
                .ok_or_else(|| CompileError::UnknownProperty(f.property.clone()))?;
            let leaf = compile_one(
                f,
                field.col,
                field.kind,
                field.validity,
                live_mask,
                &|lit| binder.code(table, field.col, lit),
            )?;
            if let (Some(valid), true) = (field.validity, reads_value(f.operator)) {
                if !nullable.contains(&(field.col, valid)) {
                    nullable.push((field.col, valid));
                }
            }
            leaves.push(leaf);
        }
        alternatives.push(Filter::and(leaves));
    }
    Ok(Filter::and([live, Filter::or(alternatives)]).sql_where(&nullable))
}

/// Whether the operator compares the value lane (and so is three-valued over
/// a nullable property). The presence operators read only the validity plane.
fn reads_value(op: Operator) -> bool {
    !matches!(op, Operator::HasProperty | Operator::NotHasProperty)
}

fn compile_one(
    f: &PropertyFilter,
    col: Col,
    kind: FieldKind,
    validity: Option<Mask>,
    live: Mask,
    code: &dyn Fn(&str) -> Option<u32>,
) -> Result<Filter, CompileError> {
    let missing = || CompileError::MissingValue {
        property: f.property.clone(),
        operator: f.operator,
    };
    let unsupported = || CompileError::OperatorNotSupported {
        property: f.property.clone(),
        operator: f.operator,
    };
    let value = || f.value.as_deref().ok_or_else(missing);
    let int = |lit: &str| {
        lit.trim()
            .parse::<i32>()
            .map_err(|_| CompileError::NotAnInteger {
                property: f.property.clone(),
                value: lit.into(),
            })
    };
    let coded = |lit: &str| {
        code(lit).ok_or_else(|| CompileError::UnknownValue {
            property: f.property.clone(),
            value: lit.into(),
        })
    };
    let leaf = |cmp| Ok(Filter::cmp(col, cmp));
    match (f.operator, kind) {
        (Operator::HasProperty, _) => Ok(match validity {
            Some(valid) => Filter::is_not_null(valid),
            // Every live record has a value. Quack has no constant TRUE (an
            // empty AND is refused at lowering), so restate `live`.
            None => Filter::plane(live),
        }),
        (Operator::NotHasProperty, _) => Ok(match validity {
            Some(valid) => Filter::is_null(valid),
            // No live record lacks a value: FALSE, spelled `¬live`.
            None => Filter::negate(Filter::plane(live)),
        }),
        (Operator::ContainsToken | Operator::NotContainsToken, _) => {
            Err(CompileError::TextSearch {
                property: f.property.clone(),
            })
        }
        (Operator::Eq, FieldKind::Code) => leaf(Cmp::EqU32(coded(value()?)?)),
        (Operator::Neq, FieldKind::Code) => leaf(Cmp::NeU32(coded(value()?)?)),
        (Operator::Eq, FieldKind::I32) => leaf(Cmp::EqI32(int(value()?)?)),
        (Operator::Neq, FieldKind::I32) => leaf(Cmp::NeI32(int(value()?)?)),
        (Operator::Lt, FieldKind::I32) => leaf(Cmp::LtI32(int(value()?)?)),
        (Operator::Lte, FieldKind::I32) => leaf(Cmp::LeI32(int(value()?)?)),
        (Operator::Gt, FieldKind::I32) => leaf(Cmp::GtI32(int(value()?)?)),
        (Operator::Gte, FieldKind::I32) => leaf(Cmp::GeI32(int(value()?)?)),
        (Operator::Between, FieldKind::I32) => {
            let low = int(value()?)?;
            let high = int(f.high_value.as_deref().ok_or_else(missing)?)?;
            Ok(Filter::and([
                Filter::cmp(col, Cmp::GeI32(low)),
                Filter::cmp(col, Cmp::LeI32(high)),
            ]))
        }
        (Operator::In | Operator::NotIn, _) => {
            if f.values.is_empty() {
                return Err(missing());
            }
            let set = match kind {
                FieldKind::Code => Filter::in_u32(
                    col,
                    f.values
                        .iter()
                        .map(|v| coded(v))
                        .collect::<Result<Vec<_>, _>>()?,
                ),
                FieldKind::I32 => Filter::in_i32(
                    col,
                    f.values
                        .iter()
                        .map(|v| int(v))
                        .collect::<Result<Vec<_>, _>>()?,
                ),
            };
            Ok(if f.operator == Operator::In {
                set
            } else {
                Filter::negate(set)
            })
        }
        (
            Operator::Lt | Operator::Lte | Operator::Gt | Operator::Gte | Operator::Between,
            FieldKind::Code,
        ) => Err(unsupported()),
    }
}
