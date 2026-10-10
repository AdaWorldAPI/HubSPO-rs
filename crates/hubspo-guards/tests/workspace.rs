//! Runs every guard in `hubspo_guards` over the real workspace.
//!
//! Anti-vacuity: the test also asserts that it found every member listed in
//! the root `Cargo.toml`, that every member is classified, and that every
//! classified crate exists. A scan that silently found nothing would
//! otherwise pass.

use std::fs;
use std::path::{Path, PathBuf};

use hubspo_guards::{Violation, ZONES, check_manifest, check_source};

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("workspace root")
}

/// Member paths from the root manifest's `members = [...]` list.
fn members(root_toml: &str) -> Vec<String> {
    let start = root_toml.find("members = [").expect("members list");
    let list = &root_toml[start..];
    let end = list.find(']').expect("members list end");
    list[..end]
        .lines()
        .filter_map(|l| l.trim().strip_prefix('"'))
        .filter_map(|l| l.split('"').next())
        .map(str::to_string)
        .collect()
}

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

#[test]
fn the_workspace_breaks_no_guard() {
    let root = workspace_root();
    let root_toml = fs::read_to_string(root.join("Cargo.toml")).expect("root manifest");
    let mut violations: Vec<Violation> = check_manifest(None, "Cargo.toml", &root_toml);

    let members = members(&root_toml);
    assert_eq!(
        members.len(),
        ZONES.len(),
        "root members and hubspo_guards::ZONES disagree: {members:?}"
    );

    let mut source_files = 0;
    for member in &members {
        let dir = root.join(member);
        let name = dir
            .file_name()
            .and_then(|n| n.to_str())
            .expect("member dir name")
            .to_string();
        assert!(
            ZONES.iter().any(|(z, _)| *z == name),
            "member `{name}` is not in ZONES"
        );

        let manifest_path = dir.join("Cargo.toml");
        let manifest = fs::read_to_string(&manifest_path)
            .unwrap_or_else(|e| panic!("{}: {e}", manifest_path.display()));
        violations.extend(check_manifest(
            Some(&name),
            &manifest_path.display().to_string(),
            &manifest,
        ));

        // This crate spells the forbidden patterns in order to test them.
        if name == "hubspo-guards" {
            continue;
        }
        let mut files = Vec::new();
        rust_files(&dir.join("src"), &mut files);
        rust_files(&dir.join("tests"), &mut files);
        for file in files {
            let src = fs::read_to_string(&file).expect("source file");
            violations.extend(check_source(&name, &file.display().to_string(), &src));
            source_files += 1;
        }
    }

    // Every non-guard member has at least a lib.rs.
    assert!(
        source_files >= members.len() - 1,
        "scanned only {source_files} source files for {} members",
        members.len()
    );
    assert!(
        violations.is_empty(),
        "guard violations:\n{}",
        violations
            .iter()
            .map(|v| format!("  {}:{} {:?} — {}", v.file, v.line, v.rule, v.detail))
            .collect::<Vec<_>>()
            .join("\n")
    );
}
