//! `hubspo-harvest select <index.json>` — prints one line per selected API:
//! `slug <TAB> version <TAB> stage <TAB> openApi-url`.
//!
//! `hubspo-harvest report <cache-dir>` — reads `<cache-dir>/index.json` and
//! `<cache-dir>/specs/<slug>@<version>.json`, prints the report as JSON.

use std::path::Path;
use std::process::ExitCode;

use hubspo_harvest::catalog::Catalog;
use hubspo_harvest::report;

fn read_catalog(path: &Path) -> Result<Catalog, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    Catalog::parse(&text).map_err(|e| format!("{}: {e}", path.display()))
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
        _ => Err("usage: hubspo-harvest select <index.json> | report <cache-dir>".to_string()),
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
