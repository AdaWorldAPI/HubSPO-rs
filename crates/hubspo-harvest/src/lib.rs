//! # hubspo-harvest
//!
//! Tool (D-ONT-1): reads HubSpot's public API catalog and the OpenAPI
//! documents it points at, and produces a normalised [`report::HarvestReport`]
//! — per object type, the operations HubSpot exposes for it. That is the
//! METHOD evidence for the concept mint dossier (D-ONT-3) and the input the
//! OGAR OpenAPI front end will consume (D-ONT-2).
//!
//! - Zone: tool (wire formats allowed; never linked into the server).
//! - Fetching is done by `scripts/fetch-hubspot-specs.sh` into a gitignored
//!   cache. This crate reads files only, so every result is reproducible from
//!   the cache and testable without a network.
//! - HubSpot's documents are never committed. Tests use synthetic fixtures.
//! - Property definitions are not in the specs: HubSpot properties are
//!   runtime data (`properties: map<string,string>` on every object). They
//!   come from a sandbox portal's properties API, which needs the portal
//!   named in `ISSUES.md` → `ISS-ORACLE-PORTAL`.

pub mod catalog;
pub mod objdef;
pub mod openapi;
pub mod positions;
pub mod report;
pub mod surface;
