# D-QR-HUB: ClassView-mapped Boolean QR as deterministic HubSPO association view

**Date:** 2026-10-21. **Status:** LAB PLAN, unimplemented; no CRM API calls or writes. **Rust 1.99.**

## Purpose

Test whether HubSPO's existing `Catalog` / Quack numeric binder, mailbox-owned resident SoA tables and eventual association-edge lanes can use a single **ClassView/WideFieldMask-guided bound relationship** rather than handrolled edge traversal. The first target is deterministic contact→company association existence plus active-list participation, not a new HubSpot graph engine.

Important current-state distinction: `hubspo-store::table::Table`, catalog binder and list masks **exist**. The documentation lists **D-STO-3 association-edge lanes as QUEUED**, and `hubspo-store/src/lib.rs` currently exposes catalog/table/log modules, not a shipped `Association` execution API. First probe MUST use synthetic explicit edge-lane fixtures or only already-implemented list masks. Do **not** report association parity against a nonexistent production edge store.

## Existing physical/semantic anchors

- `hubspo-store::table::Table`: `planes()` returns borrowed mask slices and `LaneRef`, rows keep stable ordinals even after deletion, every owner-stamped `Batch` advances a version; `Applied.changed` gives actual **touched rows** (not automatically exact changed Boolean relations).
- `hubspo-store::catalog::Catalog` implements Quack `Registrar` + `Binder` and resolves text to a field-local numeric code once. `hubspo-schema` supplies ClassView skins, but its coverage must be audited, not assumed complete.
- `hubspo-query` lists/filters and `hubspo-iam` request- and role-scoped read controls already exist. Preserve mailbox owner, tenant scope, ACL field masks and nullable validity planes.
- Reuse `lance-graph-contract::class_view::{ClassView,FieldMask,WideFieldMask}` from current sibling dependency, **without copying or inventing field positions**. `WideFieldMask` expresses field presence/eligibility, not association identity or permission.

## Proposed test-layer interface (illustrative only)

```text
BindHubRelation(
    source_class=Contact, target_class=Company,
    predicate=HubSpotAssociationLabel(l),
    field_basis=ClassView + WideFieldMask,
    owner=MailboxId, source_version, target_version,
    left_ordinal_witness, right_ordinal_witness,
    source_live, target_live, validity, auth_scope
) -> borrowed QR relation view | typed refusal
```

A 64×64 Boolean relationship is **512 B**, a 256×256 one **8 KiB**. Both are per-predicate typed tiles; actual `Table` buffers, `off/pos`, provenance and proof sidecar are additional. `Method8×Slot8` unary method participation is not binary `ContactSlot×CompanySlot` adjacency.

## Tests, independent oracles, kill gates

| Path | Expected | Refuse/counterexample |
|---|---|---|
| H1 Quack field map | numeric bind and ClassView FieldRef positions agree; FieldMask small and WideFieldMask high bit case | tenant custom field absent, ordinal collision or codebook rename stale |
| H2 List mask | `hubspo-query::list` active membership equals direct scalar live∩filter∩auth mask | tombstoned row counted due to old bitmap |
| H3 Synthetic association | typed contact×company `OR/AND` QR64 and QR256 matches a **separate fixture oracle**; no false production parity claim | label A != label B; reversed association direction |
| H4 Multiplicity | duplicate labeled edges: EXISTS remains true, count obeys bag and association-cardinality policy | set/bag collapsed, duplicate support deletion prematurely removes edge |
| H5 Source stamp | publisher `Batch` preserves mailbox owner, version, stable ordinals and invalidates/rebuilds sidecar correctly | source version changed or owner mismatch: no stale answer |
| H6 Data boundary | no `Vec<DTO>` in Count/Exists; query masks may borrow `Table::planes` | codebook-string work or tuple decoding inside hot execution |
| H7 Cost and attention | compare direct boolean masks vs QR64 vs QR256 across 1/8/64 requests; index/build/refresh/invalidation charged | cached QR slower and still promoted |
| H8 Write gate | no API update from a computation certificate; denied read yields nothing | authorization mask interpreted as mere presence; unauthorized associations observable |

## Steps

S0 read `D-STO-3` and `hubspo-schema` status before touching code; coordinate with D-STO owner. S1 create only a **fixture-backed** `ClassView` projection across one existing live table. S2 demonstrate QR64 synthetic typed association and QR256 scaling; store bounded local IDs only. S3 compare existing list/filtered masks with synthetic graph fixture and independent scalar reference. S4 run malformed tenant / nullable / label / stale version / owner tests. S5 benchmark the *whole* bind/index/projection/fold/terminal path; preserve Rust 1.99 and SIMD via ndarray. S6 post results and explicit ADOPT/REJECT/DEFER to EXO-50 before proposing D-STO-3 production integration.

**Do not:** implement HubSpot connectivity, modify D-STO-3 owner work, create non-verified association classes, assume every custom property fits 64 fields, move authorization after projection, or bypass #1640 ADMISSION B.

**References:** [#1659](https://github.com/AdaWorldAPI/lance-graph/pull/1659), [#1660](https://github.com/AdaWorldAPI/lance-graph/pull/1660), [#1659→#1660 mapping](https://github.com/AdaWorldAPI/lance-graph/blob/research/qr256-elimination-rotation-20261021/.claude/plans/qr256-powershell-ordinal-polyfill-bridge-v1.md), [EXO-50](https://linear.app/exo-red/issue/EXO-50).


## Shared rail-coordinate hypothesis (not a HubSPO fact)

The [lance-graph capstone](https://github.com/AdaWorldAPI/lance-graph/blob/research/qr-abi-classview-capstone-20261021/.claude/plans/d-qr-abi-classview-rails-capstone-v1.md) proposes reading an existing `FacetCascade` as `facet_classid(4) + 6×(PART_OF8:ISA8)` and, independently, investigating two **logical** HHTL16 16×8-bit zippers. HubSPO's association labels, company/contact relationships, and tenant owner coordinates must **not** be assigned ISA or PART_OF semantics just because the bytes fit. The initial typed QR axes remain `ContactLocal8×CompanyLocal8` with an explicitly sourced association predicate and version/owner witness. Only use a hierarchy rail when real HubSPO schema/association metadata provides its meaning.

**H9 additional probe:** compare 256 local contact/company codes against real table ordinals, then a 257th unseen ID; refuse truncation, aliased codebooks, changed owner/tenant and unobserved association. Synthetic testing only while D-STO-3 is queued. A `WideFieldMask` selects populated field positions, not external association label values.
