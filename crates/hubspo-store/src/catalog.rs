//! The table and field catalog that Quack binds through (H-ADR-6).
//!
//! `lance-graph-quack::bind` names two seams and implements neither:
//! [`Registrar`] (who owns the table catalog) and [`Binder`] (names and text
//! literals → numeric identities). HubSPO-rs owns both here. Proven first in
//! probe P-QDB-1 against a plain-loop oracle.
//!
//! Text stops at this module. A coded field carries a [`Codebook`]: its
//! labels are numbered `0..n` in the order given, a literal binds to its
//! number once, and an unknown literal binds to nothing ([`Binder::code`]
//! returns `None`; nothing is minted at bind time). A coded field without a
//! codebook (a foreign key) never binds a text literal.
//!
//! Plane convention: every table's resident plane `0` is its live plane
//! ([`LIVE`]). A nullable field names its own validity plane.

use std::collections::HashMap;

use lance_graph_quack::bind::{Binder, BoundField, FieldKind, RegisterError, Registrar, TableId};
use lance_graph_quack::{Col, Mask};

/// Every table's live plane: resident mask `0`.
pub const LIVE: Mask = Mask(0);

/// Labels of one coded field, numbered in the order given.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Codebook {
    labels: Vec<String>,
    codes: HashMap<String, u32>,
}

impl Codebook {
    /// A codebook over `labels`; label `i` gets code `i`.
    ///
    /// # Errors
    ///
    /// [`CatalogError::DuplicateLabel`] if a label repeats: two codes for one
    /// label would make a binding depend on which one was looked up.
    pub fn new<S: Into<String>>(labels: impl IntoIterator<Item = S>) -> Result<Self, CatalogError> {
        let labels: Vec<String> = labels.into_iter().map(Into::into).collect();
        let mut codes = HashMap::with_capacity(labels.len());
        for (i, label) in labels.iter().enumerate() {
            let code = u32::try_from(i).map_err(|_| CatalogError::CodebookTooLarge)?;
            if codes.insert(label.clone(), code).is_some() {
                return Err(CatalogError::DuplicateLabel(label.clone()));
            }
        }
        Ok(Self { labels, codes })
    }

    /// The code of `label`, if the codebook has it.
    pub fn code(&self, label: &str) -> Option<u32> {
        self.codes.get(label).copied()
    }

    /// The label of `code`, if the codebook has it (hydration, last).
    pub fn label(&self, code: u32) -> Option<&str> {
        self.labels.get(code as usize).map(String::as_str)
    }

    /// Number of labels.
    pub fn len(&self) -> usize {
        self.labels.len()
    }

    /// Whether the codebook has no labels.
    pub fn is_empty(&self) -> bool {
        self.labels.is_empty()
    }
}

/// One field to declare on a table.
#[derive(Debug, Clone)]
pub struct FieldSpec {
    /// The field's name, as filters spell it.
    pub name: String,
    /// The lane the executor reads.
    pub col: Col,
    /// What a literal for it must be.
    pub kind: FieldKind,
    /// The validity plane when the field is nullable.
    pub validity: Option<Mask>,
    /// Labels for a coded field; `None` for a foreign key or an integer.
    pub codebook: Option<Codebook>,
}

/// Why a declaration was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CatalogError {
    /// No table with this id.
    UnknownTable(TableId),
    /// The table already has a field with this name.
    DuplicateField(String),
    /// The table already has a field on this lane.
    DuplicateColumn(Col),
    /// A codebook on an integer field: integer literals never go through one.
    CodebookOnInteger(String),
    /// A field's validity plane is the live plane.
    ValidityIsLive(String),
    /// A label appears twice in one codebook.
    DuplicateLabel(String),
    /// More labels than `u32` codes.
    CodebookTooLarge,
}

#[derive(Debug, Clone)]
struct TableMeta {
    name: String,
    width: u32,
    fields: Vec<(String, BoundField, Option<Codebook>)>,
}

/// HubSPO-rs's catalog: the production [`Registrar`] and [`Binder`].
#[derive(Debug, Clone, Default)]
pub struct Catalog {
    tables: Vec<TableMeta>,
}

impl Catalog {
    /// An empty catalog.
    pub fn new() -> Self {
        Self::default()
    }

    /// Declare a field on a registered table.
    ///
    /// # Errors
    ///
    /// See [`CatalogError`]; nothing is changed on error.
    pub fn declare(&mut self, table: TableId, spec: FieldSpec) -> Result<(), CatalogError> {
        let meta = self
            .tables
            .get_mut(table.0 as usize)
            .ok_or(CatalogError::UnknownTable(table))?;
        if meta.fields.iter().any(|f| f.0 == spec.name) {
            return Err(CatalogError::DuplicateField(spec.name));
        }
        if meta.fields.iter().any(|f| f.1.col == spec.col) {
            return Err(CatalogError::DuplicateColumn(spec.col));
        }
        if spec.kind == FieldKind::I32 && spec.codebook.is_some() {
            return Err(CatalogError::CodebookOnInteger(spec.name));
        }
        if spec.validity == Some(LIVE) {
            return Err(CatalogError::ValidityIsLive(spec.name));
        }
        meta.fields.push((
            spec.name,
            BoundField {
                col: spec.col,
                kind: spec.kind,
                validity: spec.validity,
            },
            spec.codebook,
        ));
        Ok(())
    }

    /// The codebook of a field, for hydrating codes back to labels.
    pub fn codebook(&self, table: TableId, field: &str) -> Option<&Codebook> {
        self.field_meta(table, field)?.2.as_ref()
    }

    /// A table's declared row width in bytes.
    pub fn width(&self, table: TableId) -> Option<u32> {
        self.tables.get(table.0 as usize).map(|t| t.width)
    }

    fn field_meta(
        &self,
        table: TableId,
        field: &str,
    ) -> Option<&(String, BoundField, Option<Codebook>)> {
        self.tables
            .get(table.0 as usize)?
            .fields
            .iter()
            .find(|f| f.0 == field)
    }
}

impl Registrar for Catalog {
    fn register(&mut self, name: &str, width: u32) -> Result<TableId, RegisterError> {
        if self.tables.iter().any(|t| t.name == name) {
            return Err(RegisterError::Exists(name.into()));
        }
        // Lanes are 4-byte (`U32` / `I32`); a width that is not a whole
        // number of lanes has no layout here.
        if width % 4 != 0 {
            return Err(RegisterError::Refused {
                table: name.into(),
                width,
            });
        }
        let id = u32::try_from(self.tables.len()).map_err(|_| RegisterError::Refused {
            table: name.into(),
            width,
        })?;
        self.tables.push(TableMeta {
            name: name.into(),
            width,
            fields: Vec::new(),
        });
        Ok(TableId(id))
    }
}

impl Binder for Catalog {
    fn table(&self, name: &str) -> Option<TableId> {
        self.tables
            .iter()
            .position(|t| t.name == name)
            .and_then(|i| u32::try_from(i).ok())
            .map(TableId)
    }

    fn live(&self, _table: TableId) -> Mask {
        LIVE
    }

    fn field(&self, table: TableId, name: &str) -> Option<BoundField> {
        self.field_meta(table, name).map(|f| f.1)
    }

    fn code(&self, table: TableId, field: Col, literal: &str) -> Option<u32> {
        self.tables
            .get(table.0 as usize)?
            .fields
            .iter()
            .find(|f| f.1.col == field)?
            .2
            .as_ref()?
            .code(literal)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lance_graph_quack::bind::create_table;

    fn contact() -> (Catalog, TableId) {
        let mut c = Catalog::new();
        let t = create_table("contact")
            .width(8)
            .register(&mut c)
            .expect("register")
            .id;
        c.declare(
            t,
            FieldSpec {
                name: "lifecyclestage".into(),
                col: Col(0),
                kind: FieldKind::Code,
                validity: None,
                codebook: Some(Codebook::new(["lead", "customer"]).expect("codebook")),
            },
        )
        .expect("declare");
        (c, t)
    }

    #[test]
    fn literals_bind_to_their_position_and_unknown_ones_to_nothing() {
        let (c, t) = contact();
        assert_eq!(c.code(t, Col(0), "lead"), Some(0));
        assert_eq!(c.code(t, Col(0), "customer"), Some(1));
        assert_eq!(c.code(t, Col(0), "evangelist"), None);
        assert_eq!(
            c.codebook(t, "lifecyclestage").and_then(|b| b.label(1)),
            Some("customer")
        );
    }

    #[test]
    fn a_field_without_a_codebook_binds_no_text() {
        let (mut c, t) = contact();
        c.declare(
            t,
            FieldSpec {
                name: "owner".into(),
                col: Col(1),
                kind: FieldKind::Code,
                validity: Some(Mask(1)),
                codebook: None,
            },
        )
        .expect("declare");
        assert_eq!(c.code(t, Col(1), "lead"), None);
        assert_eq!(c.field(t, "owner").map(|f| f.validity), Some(Some(Mask(1))));
    }

    #[test]
    fn declarations_that_would_make_binding_ambiguous_are_refused() {
        let (mut c, t) = contact();
        let spec = |name: &str, col: u16| FieldSpec {
            name: name.into(),
            col: Col(col),
            kind: FieldKind::I32,
            validity: None,
            codebook: None,
        };
        assert_eq!(
            c.declare(t, spec("lifecyclestage", 5)),
            Err(CatalogError::DuplicateField("lifecyclestage".into()))
        );
        assert_eq!(
            c.declare(t, spec("amount", 0)),
            Err(CatalogError::DuplicateColumn(Col(0)))
        );
        let mut coded = spec("amount", 2);
        coded.codebook = Some(Codebook::new(["x"]).expect("codebook"));
        assert_eq!(
            c.declare(t, coded),
            Err(CatalogError::CodebookOnInteger("amount".into()))
        );
        let mut live = spec("amount", 2);
        live.validity = Some(LIVE);
        assert_eq!(
            c.declare(t, live),
            Err(CatalogError::ValidityIsLive("amount".into()))
        );
        assert_eq!(
            Codebook::new(["a", "b", "a"]),
            Err(CatalogError::DuplicateLabel("a".into()))
        );
        // The good declaration still goes through after the refusals.
        assert_eq!(c.declare(t, spec("amount", 2)), Ok(()));
        assert_eq!(
            c.declare(TableId(9), spec("x", 3)),
            Err(CatalogError::UnknownTable(TableId(9)))
        );
    }

    #[test]
    fn tables_register_once_and_only_in_whole_lanes() {
        let (mut c, _) = contact();
        assert_eq!(
            create_table("contact")
                .width(8)
                .register(&mut c)
                .map(|r| r.id),
            Err(RegisterError::Exists("contact".into()))
        );
        assert!(matches!(
            create_table("deal").width(6).register(&mut c),
            Err(RegisterError::Refused { .. })
        ));
        let deal = create_table("deal")
            .width(12)
            .register(&mut c)
            .expect("deal")
            .id;
        assert_eq!(c.table("deal"), Some(deal));
        assert_eq!(c.width(deal), Some(12));
        assert_eq!(c.table("company"), None);
    }
}
