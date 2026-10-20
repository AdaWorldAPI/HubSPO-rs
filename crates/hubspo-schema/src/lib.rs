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
//! property's lane `n` (stored in the table, append-only), so a position held
//! in a `WideFieldMask` keeps its meaning. Field 0 is the object's packed
//! run: one position for every property the table puts there.
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
//! mask reaches positions `0..256` only, and HubSpot's contact object has 272
//! standard properties. Properties in rarely queried groups share the packed
//! run at field 0 (operator decision 2026-10-20, `ISS-FIELD-MASK-256`;
//! `hubspo-harvest::positions::PACKED_GROUPS`). A role granted field 0 sees
//! every packed property of the object; one not granted it sees none, so a
//! packed property is never reachable through any other position.
//!
//! Object types whose HubSpot name has no concept, or shares one, are not in
//! the ClassView; their tables are generated all the same. Invoice and Order
//! converge with Quote on one concept, and one concept cannot hold three
//! tables' positions.

use hubspo_generated::standard_properties::{self, PACKED_RUN_POSITION, Slot, StandardProperty};
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

/// The name of every object's packed-run field (not a HubSpot property name:
/// `@` cannot occur in one).
pub const PACKED_RUN_NAME: &str = "@packed";

/// The label of every object's packed-run field.
pub const PACKED_RUN_LABEL: &str = "Packed properties";

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
            // Field 0 is the packed run; lanes are dense from 1 in table
            // order (the generator checks both).
            let run = FieldRef::new(
                format!("hubspot-property:{object}/{PACKED_RUN_NAME}"),
                PACKED_RUN_LABEL,
            );
            let lanes = table
                .iter()
                .filter(|p| matches!(p.slot, Slot::Lane(_)))
                .map(|p| {
                    let label = if p.label.is_empty() { p.name } else { p.label };
                    FieldRef::new(format!("hubspot-property:{object}/{}", p.name), label)
                });
            let fields = std::iter::once(run).chain(lanes).collect();
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

    /// The slot of the property with internal name `name` on `class`.
    #[must_use]
    pub fn slot_of(&self, class: ClassId, name: &str) -> Option<Slot> {
        self.standard(class)?
            .iter()
            .find(|p| p.name == name)
            .map(|p| p.slot)
    }

    /// The field position that grants the property `name` on `class`: its
    /// lane, or [`PACKED_RUN_POSITION`] for a packed property.
    #[must_use]
    pub fn position_of(&self, class: ClassId, name: &str) -> Option<u8> {
        self.slot_of(class, name).map(|slot| match slot {
            Slot::Lane(n) => n,
            Slot::Packed => PACKED_RUN_POSITION,
        })
    }

    /// The properties of `class` held in its packed run, in table order.
    pub fn packed(&self, class: ClassId) -> impl Iterator<Item = &'static StandardProperty> {
        self.standard(class)
            .unwrap_or(&[])
            .iter()
            .filter(|p| p.slot == Slot::Packed)
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

    /// Field counts (packed run included) and packed counts per object
    /// type, pinned exactly (`== N`, plan gate G1). They move only when the
    /// table is re-harvested, and the diff of this test then shows which
    /// object grew.
    #[test]
    fn field_counts_are_pinned() {
        let v = view();
        let counts: Vec<(&str, usize, usize)> = OBJECT_CONCEPT_NAMES
            .iter()
            .map(|&(o, n)| {
                let c = concept(n);
                (o, v.field_count(c), v.packed(c).count())
            })
            .collect();
        assert_eq!(
            counts,
            vec![
                ("contact", 246, 27),
                ("company", 160, 0),
                ("deal", 98, 0),
                ("ticket", 108, 0),
                ("lead", 94, 0),
                ("product", 31, 0),
                ("line_item", 59, 0),
                ("call", 17, 0),
                ("meeting", 30, 0),
                ("task", 9, 0),
                ("note", 5, 0),
                ("email", 16, 0),
                ("commerce_payment", 93, 0),
            ]
        );
    }

    #[test]
    fn field_0_is_the_run_and_field_n_is_lane_n() {
        let v = view();
        for c in v.concepts() {
            let object = v.object(c).unwrap();
            let fields = v.fields(c);
            assert_eq!(
                fields[0].predicate_iri,
                format!("hubspot-property:{object}/@packed")
            );
            assert_eq!(
                v.field_label(c, PACKED_RUN_POSITION),
                Some(PACKED_RUN_LABEL)
            );
            let mut lanes = 0;
            for p in v.standard(c).unwrap() {
                let Slot::Lane(n) = p.slot else {
                    assert_eq!(v.position_of(c, p.name), Some(PACKED_RUN_POSITION));
                    continue;
                };
                lanes += 1;
                let f = &fields[usize::from(n)];
                assert_eq!(
                    f.predicate_iri,
                    format!("hubspot-property:{object}/{}", p.name)
                );
                let label = if p.label.is_empty() { p.name } else { p.label };
                assert_eq!(f.label, label);
                assert_eq!(v.field_label(c, n), Some(label));
                assert_eq!(v.position_of(c, p.name), Some(n));
            }
            assert_eq!(fields.len(), lanes + 1);
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
        assert_eq!(v.packed(pipeline).count(), 0);
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
    fn every_contact_property_has_a_mask_position() {
        let v = view();
        let contact = concept("Contact");
        let table = v.standard(contact).unwrap();
        assert_eq!(table.len(), 272, "more than a mask's 256 positions");
        for p in table {
            assert!(v.position_of(contact, p.name).is_some(), "{}", p.name);
        }
        let packed: Vec<&str> = v.packed(contact).map(|p| p.name).collect();
        assert!(packed.contains(&"twitterbio"));
        assert!(packed.contains(&"school"));
        // Lanes stay one per property: email is not in the run.
        assert_ne!(v.position_of(contact, "email"), Some(PACKED_RUN_POSITION));
        // Silence twin: an object with no packed group has an empty run.
        assert_eq!(v.packed(concept("Deal")).count(), 0);
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
