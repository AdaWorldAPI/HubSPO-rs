//! # hubspo-guards
//!
//! Structural guards for the HubSPO-rs workspace, run as ordinary tests
//! (`tests/workspace.rs`). They turn the plan's Gate G0 rules into failures
//! instead of review comments:
//!
//! | rule | why |
//! |---|---|
//! | no stalwart crate or stalwart git source | stalwart is AGPL; it runs as its own process (H-ADR-8) |
//! | no `surrealdb` / SurrealQL adapter | SurrealQL is deprecated workspace-wide |
//! | no `datafusion*` | DataFusion is in a grace period; nothing new goes to it (H-ADR-6) |
//! | no `lettre` | mail is sent through stalwart's submission (H-ADR-8) |
//! | serde only in wire crates | zone discipline: zone 2 carries no wire formats |
//! | every member classified | a crate with no zone cannot be checked |
//! | no `…id: String` field | identity is `NodeGuid` + classid (H-ADR-1) |
//! | no `NodeGuid::new(` | the V1 tail is forbidden for new mints; use `mint_for` |
//!
//! The checks are line-based on purpose. They read `Cargo.toml` and `.rs`
//! text without a parser dependency, so this crate stays dependency-free.
//! The cost is that they can be evaded by unusual formatting; they are a
//! backstop for honest mistakes, not a security boundary.
//!
//! This crate's own sources are not source-scanned: they must spell the
//! forbidden patterns in order to test them. Its manifest is still checked.

/// Which wire formats a crate may carry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Zone {
    /// Zone 2: domain data only. No serde, no `Serialize` derives.
    NoWire,
    /// Zone 3: talks to the outside world. serde allowed.
    Wire,
    /// Test-only or tooling crate: serde allowed, never linked into the server.
    Test,
}

/// The zone of every workspace member. A member missing here is a violation
/// ([`Rule::UnclassifiedCrate`]); an entry with no member is caught by the
/// workspace test.
pub const ZONES: &[(&str, Zone)] = &[
    ("hubspo-port", Zone::NoWire),
    ("hubspo-schema", Zone::NoWire),
    ("hubspo-generated", Zone::NoWire),
    ("hubspo-store", Zone::NoWire),
    ("hubspo-query", Zone::NoWire),
    ("hubspo-actions", Zone::NoWire),
    ("hubspo-flow", Zone::NoWire),
    ("hubspo-sim", Zone::NoWire),
    ("hubspo-iam", Zone::NoWire),
    ("hubspo-intel", Zone::NoWire),
    ("hubspo-docs", Zone::NoWire),
    ("hubspo-comms", Zone::Wire),
    ("hubspo-compat", Zone::Wire),
    ("hubspo-ui", Zone::Wire),
    ("hubspo-server", Zone::Wire),
    ("hubspo-harvest", Zone::Test),
    ("hubspo-parity-fixtures", Zone::Test),
    ("hubspo-guards", Zone::Test),
];

/// Looks up a member's zone.
pub fn zone_of(crate_name: &str) -> Option<Zone> {
    ZONES
        .iter()
        .find(|(name, _)| *name == crate_name)
        .map(|(_, zone)| *zone)
}

/// Which guard a [`Violation`] broke.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rule {
    ForbiddenDependency,
    SerdeOutsideWireZone,
    UnclassifiedCrate,
    StringId,
    V1Mint,
    SerializeDeriveOutsideWireZone,
}

/// One broken guard, with enough context to find and fix it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Violation {
    pub file: String,
    pub line: usize,
    pub rule: Rule,
    pub detail: String,
}

/// Dependency names forbidden anywhere in the workspace, as exact names or
/// name prefixes (the `bool` is "prefix").
const FORBIDDEN_DEPS: &[(&str, bool)] = &[
    ("stalwart", true),
    ("surrealdb", true),
    ("ogar-adapter-surrealql", false),
    ("datafusion", true),
    ("lettre", false),
];

fn forbidden_name(name: &str) -> Option<&'static str> {
    FORBIDDEN_DEPS.iter().find_map(|&(pattern, prefix)| {
        let hit = if prefix {
            name.starts_with(pattern)
        } else {
            name == pattern
        };
        hit.then_some(pattern)
    })
}

/// Which kind of dependency section a manifest line sits in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Section {
    Other,
    /// `[dependencies]`, `[build-dependencies]`, `[workspace.dependencies]`,
    /// and their `[target.….dependencies]` forms: linked into the crate.
    Linked,
    /// `[dev-dependencies]`: tests only.
    Dev,
    /// `[dependencies.foo]` table form: the dependency is named in the header.
    LinkedTable,
    DevTable,
}

fn classify_header(header: &str) -> (Section, Option<String>) {
    let parts: Vec<&str> = header.split('.').map(str::trim).collect();
    for (i, part) in parts.iter().enumerate() {
        let kind = match *part {
            "dependencies" | "build-dependencies" => Some(false),
            "dev-dependencies" => Some(true),
            _ => None,
        };
        if let Some(dev) = kind {
            return match parts.get(i + 1) {
                Some(name) => (
                    if dev {
                        Section::DevTable
                    } else {
                        Section::LinkedTable
                    },
                    Some(name.trim_matches('"').to_string()),
                ),
                None => (if dev { Section::Dev } else { Section::Linked }, None),
            };
        }
    }
    (Section::Other, None)
}

/// Checks one `Cargo.toml`. `crate_name` is `None` for the workspace root,
/// which only gets the forbidden-dependency rule.
pub fn check_manifest(crate_name: Option<&str>, file: &str, toml: &str) -> Vec<Violation> {
    let mut out = Vec::new();
    let zone = match crate_name {
        Some(name) => match zone_of(name) {
            Some(zone) => Some(zone),
            None => {
                out.push(Violation {
                    file: file.to_string(),
                    line: 0,
                    rule: Rule::UnclassifiedCrate,
                    detail: format!("`{name}` has no entry in hubspo_guards::ZONES"),
                });
                None
            }
        },
        None => None,
    };

    let mut section = Section::Other;
    for (idx, raw) in toml.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let lineno = idx + 1;

        // A header names the section; in table form it also names the dependency.
        if let Some(header) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            let (kind, table_dep) = classify_header(header);
            section = kind;
            if let Some(name) = table_dep {
                check_dep(
                    &mut out,
                    file,
                    lineno,
                    &name,
                    line,
                    kind == Section::DevTable,
                    zone,
                );
            }
            continue;
        }

        match section {
            Section::Linked | Section::Dev => {
                let Some((key, _)) = line.split_once('=') else {
                    continue;
                };
                // `foo.workspace = true` names `foo`.
                let name = key.trim().split('.').next().unwrap_or("").trim_matches('"');
                check_dep(
                    &mut out,
                    file,
                    lineno,
                    name,
                    line,
                    section == Section::Dev,
                    zone,
                );
            }
            Section::LinkedTable | Section::DevTable => {
                // Inside `[dependencies.foo]`: a git source or a renamed package.
                if line.contains("stalwart") {
                    out.push(forbidden(file, lineno, line));
                }
                if let Some(pkg) = line
                    .strip_prefix("package")
                    .and_then(|l| l.split_once('='))
                    .map(|(_, v)| v.trim().trim_matches('"'))
                    && forbidden_name(pkg).is_some()
                {
                    out.push(forbidden(file, lineno, line));
                }
            }
            Section::Other => {}
        }
    }
    out
}

fn forbidden(file: &str, line: usize, text: &str) -> Violation {
    Violation {
        file: file.to_string(),
        line,
        rule: Rule::ForbiddenDependency,
        detail: text.to_string(),
    }
}

fn check_dep(
    out: &mut Vec<Violation>,
    file: &str,
    line: usize,
    name: &str,
    text: &str,
    dev: bool,
    zone: Option<Zone>,
) {
    // A renamed dependency (`x = { package = "surrealdb" }`) or a git source
    // pointing at the stalwart repository is as forbidden as the plain name.
    let renamed = text
        .split("package")
        .nth(1)
        .and_then(|rest| rest.split('"').nth(1));
    if forbidden_name(name).is_some()
        || renamed.is_some_and(|pkg| forbidden_name(pkg).is_some())
        || text.contains("stalwart")
    {
        out.push(forbidden(file, line, text));
    }
    if !dev && zone == Some(Zone::NoWire) && (name == "serde" || name.starts_with("serde_")) {
        out.push(Violation {
            file: file.to_string(),
            line,
            rule: Rule::SerdeOutsideWireZone,
            detail: format!("`{name}` in a zone-2 crate"),
        });
    }
}

/// Checks one Rust source file of `crate_name`.
pub fn check_source(crate_name: &str, file: &str, src: &str) -> Vec<Violation> {
    let mut out = Vec::new();
    let no_wire = zone_of(crate_name) == Some(Zone::NoWire);
    for (idx, raw) in src.lines().enumerate() {
        let line = raw.trim_start();
        if line.starts_with("//") {
            continue;
        }
        let lineno = idx + 1;
        if has_string_id(line) {
            out.push(Violation {
                file: file.to_string(),
                line: lineno,
                rule: Rule::StringId,
                detail: raw.trim().to_string(),
            });
        }
        if line.contains("NodeGuid::new(") {
            out.push(Violation {
                file: file.to_string(),
                line: lineno,
                rule: Rule::V1Mint,
                detail: raw.trim().to_string(),
            });
        }
        if no_wire && line.starts_with("#[derive(") && line.contains("Serialize") {
            out.push(Violation {
                file: file.to_string(),
                line: lineno,
                rule: Rule::SerializeDeriveOutsideWireZone,
                detail: raw.trim().to_string(),
            });
        }
    }
    out
}

/// True when the line declares an identifier ending in `id` typed `String`,
/// e.g. `id: String` or `company_id: String`, but not `idle: String` or
/// `id: StringPool`.
fn has_string_id(line: &str) -> bool {
    let mut rest = line;
    while let Some(pos) = rest.find(": String") {
        let before = &rest[..pos];
        let after = &rest[pos + ": String".len()..];
        let ident: String = before
            .chars()
            .rev()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        let ends_cleanly = !after
            .chars()
            .next()
            .is_some_and(|c| c.is_alphanumeric() || c == '_');
        if ends_cleanly && (ident == "id" || ident.ends_with("_id")) {
            return true;
        }
        rest = after;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rules(v: &[Violation]) -> Vec<Rule> {
        v.iter().map(|x| x.rule).collect()
    }

    // ── forbidden dependencies: each spelling can fire ──

    #[test]
    fn forbidden_dependency_fires_in_every_spelling() {
        let cases = [
            "[dependencies]\nsurrealdb = \"2\"\n",
            "[dependencies]\ndatafusion-expr = \"54\"\n",
            "[dependencies]\nlettre.workspace = true\n",
            "[dev-dependencies]\nogar-adapter-surrealql = { git = \"x\" }\n",
            "[dependencies]\nsmtp = { git = \"https://github.com/AdaWorldAPI/stalwart\" }\n",
            "[dependencies]\ndb = { package = \"surrealdb\", version = \"2\" }\n",
            "[dependencies.mail]\ngit = \"https://github.com/AdaWorldAPI/stalwart\"\n",
            "[dependencies.query]\npackage = \"datafusion\"\n",
            "[target.'cfg(unix)'.dependencies]\nlettre = \"0.11\"\n",
            "[workspace.dependencies]\ndatafusion = \"54\"\n",
        ];
        for toml in cases {
            let v = check_manifest(None, "Cargo.toml", toml);
            assert_eq!(
                rules(&v),
                vec![Rule::ForbiddenDependency],
                "expected one forbidden dependency in:\n{toml}"
            );
        }
    }

    #[test]
    fn allowed_dependencies_stay_silent() {
        // Names that merely share a stem with a forbidden one must not fire.
        let toml = "[dependencies]\n\
            lance-graph-contract.workspace = true\n\
            lance = { version = \"12.*\" }\n\
            mail-parser = \"0.11\"\n\
            surreal-ish-helper = \"1\"\n\
            [package]\n\
            name = \"datafusion-free\"\n";
        assert!(check_manifest(None, "Cargo.toml", toml).is_empty());
    }

    // ── zone discipline ──

    #[test]
    fn serde_in_a_zone_two_crate_fires() {
        let v = check_manifest(
            Some("hubspo-store"),
            "Cargo.toml",
            "[dependencies]\nserde = \"1\"\nserde_json.workspace = true\n",
        );
        assert_eq!(
            rules(&v),
            vec![Rule::SerdeOutsideWireZone, Rule::SerdeOutsideWireZone]
        );
    }

    #[test]
    fn serde_stays_silent_in_wire_crates_and_in_dev_dependencies() {
        let wire = check_manifest(
            Some("hubspo-compat"),
            "Cargo.toml",
            "[dependencies]\nserde = \"1\"\n",
        );
        assert!(wire.is_empty(), "{wire:?}");
        let dev = check_manifest(
            Some("hubspo-store"),
            "Cargo.toml",
            "[dev-dependencies]\nserde_json = \"1\"\n",
        );
        assert!(dev.is_empty(), "{dev:?}");
    }

    #[test]
    fn an_unclassified_crate_fires() {
        let v = check_manifest(Some("hubspo-mystery"), "Cargo.toml", "[package]\n");
        assert_eq!(rules(&v), vec![Rule::UnclassifiedCrate]);
    }

    #[test]
    fn serialize_derive_fires_only_in_zone_two() {
        let src = "#[derive(Debug, serde::Serialize)]\nstruct Row;\n";
        assert_eq!(
            rules(&check_source("hubspo-query", "a.rs", src)),
            vec![Rule::SerializeDeriveOutsideWireZone]
        );
        assert!(check_source("hubspo-server", "a.rs", src).is_empty());
    }

    // ── identity shapes ──

    #[test]
    fn string_ids_fire() {
        for line in [
            "pub id: String,",
            "    company_id: String,",
            "fn f(owner_id: String) {}",
        ] {
            assert_eq!(
                rules(&check_source("hubspo-schema", "a.rs", line)),
                vec![Rule::StringId],
                "{line}"
            );
        }
    }

    #[test]
    fn non_id_strings_stay_silent() {
        let src = "pub name: String,\n\
            pub idle: String,\n\
            pub id: StringPool,\n\
            pub guid: NodeGuid,\n\
            // pub id: String, (commented out)\n";
        let v = check_source("hubspo-schema", "a.rs", src);
        assert!(v.is_empty(), "{v:?}");
    }

    #[test]
    fn v1_mint_fires_and_v3_mint_stays_silent() {
        assert_eq!(
            rules(&check_source(
                "hubspo-store",
                "a.rs",
                "let g = NodeGuid::new(c, t);"
            )),
            vec![Rule::V1Mint]
        );
        assert!(
            check_source("hubspo-store", "a.rs", "let g = NodeGuid::mint_for(v, c);").is_empty()
        );
    }
}
