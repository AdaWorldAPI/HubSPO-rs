//! # hubspo-schema
//!
//! ClassView skins per object type, the standard property position tables (generated), and tenant custom-schema rows (H-ADR-3).
//!
//! - Zone: zone 2 (no serde, no wire formats).
//! - Deliverables: D-ONT-6, D-ONT-7 (`.claude/plans/hubspo-integration-v1.md`).
//!
//! ## What is here (D-ONT-6)
//!
//! [`HubSpoClassView`] is HubSPO-rs's [`ClassView`]: for each concept a
//! HubSpot object type converges on, its field set is that type's standard
//! property table from `hubspo-generated`. Field position `n` is the
//! property's position in the table, which is append-only, so a position
//! held in a `WideFieldMask` keeps its meaning.
//!
//! Keyed by concept alone (`ClassId = u16`): CRM keys carry classview
//! `0x0000` (operator decision 2026-10-20, `ISS-CRM-KEY-CLASSVIEW`), so the
//! concept is the whole class identity and `HubSpoPort::APP_PREFIX` plays no
//! part. Concepts come from `hubspo-port`, never from a local hex.
//!
//! **Provisional data.** The tables come from HubSpot's public
//! default-property pages, which give display labels only. A field's
//! `predicate_iri` is therefore `hubspot-kb:<object>/<position>`, not a
//! HubSpot internal name: guessing internal names would make a later portal
//! harvest disagree silently. The portal harvest (`ISS-ORACLE-PORTAL`) adds
//! internal names and types at the same positions.
//!
//! Two object types on the pages have no concept to key on and are not in
//! the ClassView: `communication` and `postal_mail` (their tables are
//! generated all the same).

use hubspo_generated::standard_properties::{self, StandardProperty};
use lance_graph_contract::class_view::{ClassId, ClassView};
use lance_graph_contract::ontology::{DisplayTemplate, FieldRef};

/// Each generated object table with the HubSpot object name `hubspo-port`
/// resolves to a concept. Object types without a concept are absent.
pub const OBJECT_CONCEPT_NAMES: &[(&str, &str)] = &[
    ("contact", "Contact"),
    ("company", "Company"),
    ("deal", "Deal"),
    ("ticket", "Ticket"),
    ("lead", "Lead"),
    ("product", "Product"),
    ("line_item", "LineItem"),
    ("call", "Call"),
    ("meeting", "Meeting"),
    ("task", "Task"),
    ("note", "Note"),
    ("email", "Email"),
];

/// The table an object name's concept reads.
#[derive(Debug, Clone)]
struct Skin {
    concept: ClassId,
    object: &'static str,
    table: &'static [StandardProperty],
    fields: Vec<FieldRef>,
}

/// HubSPO-rs's [`ClassView`] over the standard property tables.
#[derive(Debug, Clone)]
pub struct HubSpoClassView {
    skins: Vec<Skin>,
}

/// Why the ClassView could not be built.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SchemaError {
    /// `hubspo-port` has no concept for this HubSpot object name.
    UnknownName(&'static str),
    /// No generated table for this object.
    NoTable(&'static str),
    /// Two object types resolve to the same concept.
    SharedConcept(ClassId),
}

impl HubSpoClassView {
    /// Build the view from the generated tables.
    ///
    /// # Errors
    ///
    /// [`SchemaError`] when a name does not resolve, a table is missing, or
    /// two object types would share one concept's field positions.
    pub fn new() -> Result<Self, SchemaError> {
        let mut skins: Vec<Skin> = Vec::with_capacity(OBJECT_CONCEPT_NAMES.len());
        for &(object, name) in OBJECT_CONCEPT_NAMES {
            let concept = hubspo_port::class_id(name).ok_or(SchemaError::UnknownName(name))?;
            if skins.iter().any(|s| s.concept == concept) {
                return Err(SchemaError::SharedConcept(concept));
            }
            let table = standard_properties::OBJECTS
                .iter()
                .find_map(|(o, t)| (*o == object).then_some(*t))
                .ok_or(SchemaError::NoTable(object))?;
            let fields = table
                .iter()
                .map(|p| FieldRef::new(format!("hubspot-kb:{object}/{}", p.position), p.label))
                .collect();
            skins.push(Skin {
                concept,
                object,
                table,
                fields,
            });
        }
        Ok(Self { skins })
    }

    fn skin(&self, class: ClassId) -> Option<&Skin> {
        self.skins.iter().find(|s| s.concept == class)
    }

    /// The standard property table of `class`, if HubSPO-rs has one.
    #[must_use]
    pub fn standard(&self, class: ClassId) -> Option<&'static [StandardProperty]> {
        self.skin(class).map(|s| s.table)
    }

    /// The generated object name behind `class` (`"contact"`, …).
    #[must_use]
    pub fn object(&self, class: ClassId) -> Option<&'static str> {
        self.skin(class).map(|s| s.object)
    }

    /// The field position of the property labelled `label` on `class`.
    #[must_use]
    pub fn position_of(&self, class: ClassId, label: &str) -> Option<u8> {
        self.standard(class)?
            .iter()
            .find(|p| p.label == label)
            .map(|p| p.position)
    }

    /// Every concept the view covers, in table order.
    pub fn concepts(&self) -> impl Iterator<Item = ClassId> + '_ {
        self.skins.iter().map(|s| s.concept)
    }
}

impl ClassView for HubSpoClassView {
    /// A concept HubSPO-rs has no table for has no fields.
    fn fields(&self, class: ClassId) -> &[FieldRef] {
        self.skin(class).map_or(&[], |s| s.fields.as_slice())
    }

    fn template(&self, _class: ClassId) -> DisplayTemplate {
        DisplayTemplate::Detail
    }

    /// DOLCE classification is the ontology cache's job; `0` = unclassified,
    /// as in OGAR's `OgarClassView`.
    fn dolce_category_id(&self, _class: ClassId) -> u8 {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    fn view() -> HubSpoClassView {
        HubSpoClassView::new().expect("every object name resolves")
    }

    fn concept(name: &str) -> ClassId {
        hubspo_port::class_id(name).expect("name resolves")
    }

    /// Field counts per object type, pinned exactly (`== N`, plan gate G1).
    /// They move only when the table is re-harvested, and the diff of this
    /// test then shows which object grew.
    #[test]
    fn field_counts_are_pinned() {
        let v = view();
        let counts: Vec<(&str, usize)> = OBJECT_CONCEPT_NAMES
            .iter()
            .map(|&(o, n)| (o, v.field_count(concept(n))))
            .collect();
        assert_eq!(
            counts,
            vec![
                ("contact", 175),
                ("company", 93),
                ("deal", 75),
                ("ticket", 50),
                ("lead", 61),
                ("product", 26),
                ("line_item", 54),
                ("call", 29),
                ("meeting", 28),
                ("task", 20),
                ("note", 11),
                ("email", 29),
            ]
        );
    }

    #[test]
    fn field_n_is_table_position_n() {
        let v = view();
        for c in v.concepts() {
            let table = v.standard(c).unwrap();
            let object = v.object(c).unwrap();
            for (n, f) in v.fields(c).iter().enumerate() {
                assert_eq!(usize::from(table[n].position), n);
                assert_eq!(f.label, table[n].label);
                assert_eq!(f.predicate_iri, format!("hubspot-kb:{object}/{n}"));
                assert_eq!(
                    v.field_label(c, u8::try_from(n).unwrap()),
                    Some(table[n].label)
                );
            }
        }
    }

    #[test]
    fn keyed_by_concept_with_no_app_prefix() {
        let v = view();
        let deal = concept("Deal");
        assert_eq!(v.object(deal), Some("deal"));
        // The render classid carries the app prefix in its low half; the
        // ClassView key is the concept alone.
        let rendered = hubspo_port::render_class_id("Deal").unwrap();
        assert_eq!(rendered >> 16, u32::from(deal));
        assert_ne!(rendered & 0xFFFF, 0);
        assert!(
            u16::try_from(rendered).is_err(),
            "a render classid is not a ClassId"
        );
        assert!(v.position_of(deal, "Amount").is_some());
    }

    #[test]
    fn concepts_are_distinct_and_unknown_ones_have_no_fields() {
        let v = view();
        let all: Vec<ClassId> = v.concepts().collect();
        assert_eq!(all.len(), OBJECT_CONCEPT_NAMES.len());
        assert_eq!(all.iter().collect::<BTreeSet<_>>().len(), all.len());
        let pipeline = concept("Pipeline");
        assert!(!all.contains(&pipeline));
        assert!(v.fields(pipeline).is_empty());
        assert_eq!(v.standard(pipeline), None);
        assert_eq!(v.position_of(pipeline, "Amount"), None);
    }

    #[test]
    fn labels_resolve_to_their_positions() {
        let v = view();
        let contact = concept("Contact");
        let p = v
            .position_of(contact, "First name")
            .expect("listed on the page");
        assert_eq!(v.field_label(contact, p), Some("First name"));
        assert_eq!(
            v.position_of(contact, "first name"),
            None,
            "labels are exact"
        );
        assert_eq!(v.position_of(contact, "Deal stage"), None);
    }

    #[test]
    fn every_table_fits_a_wide_field_mask() {
        for (_, table) in standard_properties::OBJECTS {
            assert!(table.len() <= 256);
        }
        // The contact table needs the wide mask: a `FieldMask` would drop it.
        assert!(view().field_count(concept("Contact")) > 64);
    }
}
