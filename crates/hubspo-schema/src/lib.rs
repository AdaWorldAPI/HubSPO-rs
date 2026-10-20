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
//! **Provisional data** (operator decision 2026-10-20). The tables come
//! from HubSpot's public object-definition pages and activity guides
//! (`ISS-KB-PROPERTIES-PROVISIONAL`), not from a portal. A field's
//! `predicate_iri` is `hubspot-property:<object>/<name>`, with `name` the
//! property's internal name; its label is the display label, or the name
//! where the source gives none (the activity guides list names only).
//!
//! **The 256-position ceiling.** `WideFieldMask` positions are `u8`, so a
//! mask reaches positions `0..256` only. HubSpot's contact object has more
//! standard properties than that; the view exposes the first 256 as fields
//! and reports the rest through [`HubSpoClassView::unaddressable`], so no
//! role can be granted a property silently dropped from its mask
//! (`ISS-FIELD-MASK-256`).
//!
//! Object types whose HubSpot name has no concept, or shares one, are not in
//! the ClassView; their tables are generated all the same. Invoice and Order
//! converge with Quote on one concept, and one concept cannot hold three
//! tables' positions.

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
    ("commerce_payment", "CommercePayment"),
];

/// The positions a `WideFieldMask` can address (`u8`).
pub const MASK_POSITIONS: usize = 256;

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
        Self::build(OBJECT_CONCEPT_NAMES)
    }

    fn build(names: &[(&'static str, &'static str)]) -> Result<Self, SchemaError> {
        let mut skins: Vec<Skin> = Vec::with_capacity(names.len());
        for &(object, name) in names {
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
                .take(MASK_POSITIONS)
                .map(|p| {
                    let label = if p.label.is_empty() { p.name } else { p.label };
                    FieldRef::new(format!("hubspot-property:{object}/{}", p.name), label)
                })
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

    /// The field position of the property with internal name `name` on
    /// `class`. `None` when the class has no such property, or when its
    /// position is past what a mask can address ([`Self::unaddressable`]).
    #[must_use]
    pub fn position_of(&self, class: ClassId, name: &str) -> Option<u8> {
        self.standard(class)?
            .iter()
            .find(|p| p.name == name)
            .and_then(|p| u8::try_from(p.position).ok())
    }

    /// The standard properties of `class` past position 255: in the table,
    /// but outside every `WideFieldMask`, so no grant can reach them.
    #[must_use]
    pub fn unaddressable(&self, class: ClassId) -> &'static [StandardProperty] {
        self.standard(class)
            .map_or(&[], |t| t.get(MASK_POSITIONS..).unwrap_or(&[]))
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
        let counts: Vec<(&str, usize, usize)> = OBJECT_CONCEPT_NAMES
            .iter()
            .map(|&(o, n)| {
                let c = concept(n);
                (o, v.field_count(c), v.unaddressable(c).len())
            })
            .collect();
        assert_eq!(
            counts,
            vec![
                ("contact", 256, 16),
                ("company", 159, 0),
                ("deal", 97, 0),
                ("ticket", 107, 0),
                ("lead", 93, 0),
                ("product", 30, 0),
                ("line_item", 58, 0),
                ("call", 16, 0),
                ("meeting", 29, 0),
                ("task", 8, 0),
                ("note", 4, 0),
                ("email", 15, 0),
                ("commerce_payment", 92, 0),
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
                let p = &table[n];
                assert_eq!(usize::from(p.position), n);
                assert_eq!(
                    f.predicate_iri,
                    format!("hubspot-property:{object}/{}", p.name)
                );
                let label = if p.label.is_empty() { p.name } else { p.label };
                assert_eq!(f.label, label);
                let n8 = u8::try_from(n).unwrap();
                assert_eq!(v.field_label(c, n8), Some(label));
                assert_eq!(v.position_of(c, p.name), Some(n8));
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
        assert!(v.position_of(deal, "amount").is_some());
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
        assert!(v.unaddressable(pipeline).is_empty());
        assert_eq!(v.position_of(pipeline, "amount"), None);
    }

    #[test]
    fn two_object_types_on_one_concept_are_refused() {
        // Invoice and Order both converge on COMMERCIAL_DOCUMENT; one
        // concept cannot hold two tables' positions.
        let names = [("invoice", "Invoice"), ("order", "Order")];
        assert_eq!(
            HubSpoClassView::build(&names).map(|v| v.concepts().count()),
            Err(SchemaError::SharedConcept(concept("Invoice")))
        );
        assert_eq!(
            HubSpoClassView::build(&[("cart", "Cart")]).map(|_| ()),
            Err(SchemaError::UnknownName("Cart"))
        );
        assert_eq!(
            HubSpoClassView::build(&[("quote", "Quote")]).map(|_| ()),
            Err(SchemaError::NoTable("quote"))
        );
        // Silence twin: distinct concepts build.
        assert_eq!(
            HubSpoClassView::build(&[("invoice", "Invoice"), ("deal", "Deal")])
                .map(|v| v.concepts().count()),
            Ok(2)
        );
    }

    #[test]
    fn names_resolve_to_their_positions() {
        let v = view();
        let contact = concept("Contact");
        let p = v
            .position_of(contact, "firstname")
            .expect("defined on the page");
        assert_eq!(v.field_label(contact, p), Some("First Name"));
        assert_eq!(
            v.position_of(contact, "First Name"),
            None,
            "names, not labels"
        );
        assert_eq!(v.position_of(contact, "dealstage"), None);
        // A guide row has no label: the name stands in.
        let call = concept("Call");
        let body = v.position_of(call, "hs_call_body").unwrap();
        assert_eq!(v.field_label(call, body), Some("hs_call_body"));
    }

    #[test]
    fn positions_past_255_are_reported_never_addressed() {
        let v = view();
        let contact = concept("Contact");
        let past = v.unaddressable(contact);
        assert_eq!(past.len(), 16);
        assert_eq!(past[0].position, 256);
        for p in past {
            assert_eq!(v.position_of(contact, p.name), None);
            assert!(
                v.standard(contact)
                    .unwrap()
                    .iter()
                    .any(|q| q.name == p.name)
            );
        }
        // Silence twin: a table under the ceiling reports nothing.
        assert!(v.unaddressable(concept("Deal")).is_empty());
        assert_eq!(v.fields(contact).len(), MASK_POSITIONS);
    }

    #[test]
    fn standard_properties_are_all_non_sensitive_where_stated() {
        // HubSpot classifies every standard property it defines on the
        // object-definition pages as `non_sensitive`; sensitive data lives in
        // tenant (custom) properties. Guide rows state no class.
        let mut stated = 0;
        for (_, table) in standard_properties::OBJECTS {
            for p in *table {
                if !p.sensitivity.is_empty() {
                    assert_eq!(p.sensitivity, "non_sensitive", "{}", p.name);
                    stated += 1;
                }
            }
        }
        assert_eq!(stated, 1337);
    }
}
