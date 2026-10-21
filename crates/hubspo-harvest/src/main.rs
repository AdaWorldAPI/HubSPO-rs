//! `hubspo-harvest select <index.json>` — prints one line per selected API:
//! `slug <TAB> version <TAB> stage <TAB> openApi-url`.
//!
//! `hubspo-harvest report <cache-dir>` — reads `<cache-dir>/index.json` and
//! `<cache-dir>/specs/<slug>@<version>.json`, prints the report as JSON.
//!
//! `hubspo-harvest objdef <page.html>` — prints one line per property defined
//! on an object-definition page: `name <TAB> type <TAB> field_type <TAB>
//! group <TAB> sensitivity <TAB> hidden <TAB> label` (see `objdef`).
//!
//! `hubspo-harvest guide <page.md>` — prints the internal names listed in an
//! activity guide's property table, in the same seven-column shape with the
//! descriptive columns empty.
//!
//! `hubspo-harvest merge <table.tsv> <object> <found.tsv>` — prints the
//! position table with `found.tsv` (the output of `objdef` or `guide`) merged
//! into `object`, append-only (see `positions`).
//!
//! `hubspo-harvest surface <page.html> ...` — prints every operation in the
//! pages' API-reference navigation, one per line: `version <TAB> area <TAB>
//! method <TAB> path <TAB> spec <TAB> title <TAB> href` (see `surface`).
//!
//! `hubspo-harvest coverage <surface.tsv> <harvest.json>` — prints, per
//! version and area, how many documented operations a previous `report`
//! output already holds, and how many also exist under the current version:
//! `version <TAB> area <TAB> operations <TAB> harvested <TAB> in_current`.
//!
//! `hubspo-harvest deliverables <surface.tsv> <map.tsv>` — prints, per
//! deliverable in the map, the documented functions it owns:
//! `deliverable <TAB> families <TAB> current <TAB> legacy_only <TAB>
//! beta_only <TAB> functions`, then one `coverage:<class> <TAB> families <TAB>
//! functions` line per coverage class. A family without a map row is an
//! error.
//!
//! `hubspo-harvest emit <table.tsv>` — prints the generated Rust for
//! `hubspo-generated/src/standard_properties.rs`.

use std::path::Path;
use std::process::ExitCode;

use hubspo_harvest::catalog::Catalog;
use hubspo_harvest::positions::Property;
use hubspo_harvest::{objdef, positions, report, surface};

fn read_catalog(path: &Path) -> Result<Catalog, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    Catalog::parse(&text).map_err(|e| format!("{}: {e}", path.display()))
}

fn read_table(path: &Path) -> Result<Vec<positions::Row>, String> {
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(format!("{}: {e}", path.display())),
    };
    positions::parse(&text).map_err(|e| format!("{}: {e:?}", path.display()))
}

fn read(path: &str) -> Result<String, String> {
    std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))
}

/// One property as `objdef`/`guide` print it and `merge` reads it.
fn found_line(p: &Property) -> String {
    format!(
        "{}\t{}\t{}\t{}\t{}\t{}\t{}",
        p.name, p.kind, p.field_type, p.group, p.sensitivity, p.hidden, p.label
    )
}

fn parse_found(line: &str) -> Result<Property, String> {
    let f: Vec<&str> = line.split('\t').collect();
    let [name, kind, field_type, group, sensitivity, hidden, label] = f[..] else {
        return Err(line.to_string());
    };
    let hidden = match hidden {
        "true" => true,
        "false" => false,
        _ => return Err(line.to_string()),
    };
    Ok(Property {
        name: name.into(),
        kind: kind.into(),
        field_type: field_type.into(),
        group: group.into(),
        sensitivity: sensitivity.into(),
        hidden,
        label: label.into(),
    })
}

fn run(args: &[String]) -> Result<(), String> {
    match args {
        [cmd, index] if cmd == "select" => {
            let catalog = read_catalog(Path::new(index))?;
            for api in &catalog.results {
                if let Some(v) = api.select() {
                    println!("{}\t{}\t{}\t{}", api.slug(), v.version, v.stage, v.open_api);
                }
            }
            Ok(())
        }
        [cmd, dir] if cmd == "report" => {
            let dir = Path::new(dir);
            let catalog = read_catalog(&dir.join("index.json"))?;
            let report = report::build(&catalog, |api, v| {
                let path = dir
                    .join("specs")
                    .join(format!("{}@{}.json", api.slug(), v.version));
                std::fs::read_to_string(&path).map_err(|e| e.to_string())
            });
            let json = serde_json::to_string_pretty(&report).map_err(|e| e.to_string())?;
            println!("{json}");
            if !report.missing.is_empty() {
                eprintln!(
                    "{} selected specs missing from the cache",
                    report.missing.len()
                );
            }
            Ok(())
        }
        [cmd, page] if cmd == "objdef" => {
            let defs = objdef::definitions(&read(page)?).map_err(|e| format!("{page}: {e:?}"))?;
            if defs.is_empty() {
                return Err(format!("{page}: no property definitions"));
            }
            for d in defs {
                let p = Property {
                    name: d.name,
                    kind: d.kind,
                    field_type: d.field_type,
                    group: d.group_name,
                    sensitivity: d.data_sensitivity.unwrap_or_default(),
                    hidden: d.hidden,
                    label: d.label,
                };
                println!("{}", found_line(&p));
            }
            Ok(())
        }
        [cmd, page] if cmd == "guide" => {
            let names = objdef::guide_names(&read(page)?);
            if names.is_empty() {
                return Err(format!("{page}: no property names"));
            }
            for name in names {
                println!(
                    "{}",
                    found_line(&Property {
                        name,
                        ..Property::default()
                    })
                );
            }
            Ok(())
        }
        [cmd, table, object, found] if cmd == "merge" => {
            let rows = read_table(Path::new(table))?;
            let found = read(found)?
                .lines()
                .map(parse_found)
                .collect::<Result<Vec<_>, _>>()
                .map_err(|line| format!("{found}: bad line: {line}"))?;
            let merged = positions::merge(&rows, object, &found).map_err(|e| format!("{e:?}"))?;
            print!("{}", positions::to_tsv(&merged));
            Ok(())
        }
        [cmd, pages @ ..] if cmd == "surface" && !pages.is_empty() => {
            let mut all: Vec<surface::Operation> = Vec::new();
            for page in pages {
                let ops = surface::operations(&read(page)?).map_err(|e| format!("{page}: {e:?}"))?;
                if ops.is_empty() {
                    return Err(format!("{page}: no operations"));
                }
                for op in ops {
                    surface::merge(&mut all, op);
                }
            }
            print!("{}", surface::to_tsv(&all));
            Ok(())
        }
        [cmd, table, harvest] if cmd == "coverage" => {
            let ops = surface::parse_tsv(&read(table)?).map_err(|l| format!("{table}: bad line {l}"))?;
            let report: serde_json::Value =
                serde_json::from_str(&read(harvest)?).map_err(|e| format!("{harvest}: {e}"))?;
            let harvested: Vec<(String, String)> = report["apis"]
                .as_array()
                .ok_or(format!("{harvest}: no apis"))?
                .iter()
                .flat_map(|api| api["operations"].as_array().into_iter().flatten())
                .filter_map(|o| Some((o["method"].as_str()?.to_string(), o["path"].as_str()?.to_string())))
                .collect();
            if harvested.is_empty() {
                return Err(format!("{harvest}: no operations"));
            }
            for c in surface::coverage(&ops, &harvested) {
                println!(
                    "{}\t{}\t{}\t{}\t{}",
                    c.version, c.area, c.operations, c.harvested, c.in_current
                );
            }
            Ok(())
        }
        [cmd, table, map] if cmd == "deliverables" => {
            let ops = surface::parse_tsv(&read(table)?).map_err(|l| format!("{table}: bad line {l}"))?;
            let map_rows = surface::parse_map(&read(map)?).map_err(|l| format!("{map}: bad line {l}"))?;
            let counts = surface::by_deliverable(&ops, &map_rows)
                .map_err(|f| format!("{map}: no row for spec family {f}"))?;
            for c in counts {
                println!(
                    "{}\t{}\t{}\t{}\t{}\t{}",
                    c.deliverable,
                    c.families,
                    c.current,
                    c.legacy_only,
                    c.beta_only,
                    c.functions()
                );
            }
            let coverage = surface::by_coverage(&ops, &map_rows)
                .map_err(|f| format!("{map}: no row for spec family {f}"))?;
            for c in coverage {
                println!("coverage:{}\t{}\t{}", c.deliverable, c.families, c.functions());
            }
            Ok(())
        }
        [cmd, table] if cmd == "emit" => {
            let rows = read_table(Path::new(table))?;
            print!("{}", positions::emit(&rows));
            Ok(())
        }
        _ => Err(
            "usage: hubspo-harvest select <index.json> | report <cache-dir> | objdef <page.html> \
             | guide <page.md> | surface <page.html>... | coverage <surface.tsv> <harvest.json> | deliverables <surface.tsv> <map.tsv> | merge <table.tsv> <object> <found.tsv> | emit <table.tsv>"
                .to_string(),
        ),
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}
