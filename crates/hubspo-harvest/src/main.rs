//! `hubspo-harvest select <index.json>` — prints one line per selected API:
//! `slug <TAB> version <TAB> stage <TAB> openApi-url`.
//!
//! `hubspo-harvest report <cache-dir>` — reads `<cache-dir>/index.json` and
//! `<cache-dir>/specs/<slug>@<version>.json`, prints the report as JSON.
//!
//! `hubspo-harvest merge <table.tsv> <object> <kb.tsv> [section ...]` — prints
//! the position table with the page's labels (only those under the named
//! sections, if any) merged into `object`, append-only (see `positions`).
//!
//! `hubspo-harvest emit <table.tsv>` — prints the generated Rust for
//! `hubspo-generated/src/standard_properties.rs`.
//!
//! `hubspo-harvest kb <page.html>` — prints one line per property label on a
//! HubSpot default-property page: `section <TAB> label` (see `kb`).

use std::path::Path;
use std::process::ExitCode;

use hubspo_harvest::catalog::Catalog;
use hubspo_harvest::{kb, positions, report};

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
        [cmd, page] if cmd == "kb" => {
            let path = Path::new(page);
            let html =
                std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
            for e in kb::parse(&html) {
                println!("{}\t{}", e.section, e.label);
            }
            Ok(())
        }
        [cmd, table, object, page, sections @ ..] if cmd == "merge" => {
            let rows = read_table(Path::new(table))?;
            let path = Path::new(page);
            let kb_tsv =
                std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
            let found: Vec<(String, String)> = kb_tsv
                .lines()
                .filter_map(|l| l.split_once('\t'))
                .filter(|(s, _)| sections.is_empty() || sections.iter().any(|x| x == s))
                .map(|(s, l)| (s.to_string(), l.to_string()))
                .collect();
            if found.is_empty() {
                return Err(format!("{page}: no labels in the selected sections"));
            }
            let merged = positions::merge(&rows, object, &found).map_err(|e| format!("{e:?}"))?;
            print!("{}", positions::to_tsv(&merged));
            Ok(())
        }
        [cmd, table] if cmd == "emit" => {
            let rows = read_table(Path::new(table))?;
            print!("{}", positions::emit(&rows));
            Ok(())
        }
        _ => Err(
            "usage: hubspo-harvest select <index.json> | report <cache-dir> | kb <page.html> \
             | merge <table.tsv> <object> <kb.tsv> [section ...] | emit <table.tsv>"
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
