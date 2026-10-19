use std::path::Path;
use std::process::Command;

use crate::cargo_manifest;
use crate::collect::Findings;

pub enum Plan {
    /// No Rust files, or nothing missing
    Nothing,
    /// We could act, but shouldn't. The text says why.
    Skip(&'static str),
    /// Crates used in code but missing from Cargo.toml
    Add(Vec<String>),
}

/// In a workspace, sibling crates import each other by name. We'd mistake
/// them for crates.io packages and could install a stranger's crate.
fn is_workspace(root: &Path) -> bool {
    std::fs::read_to_string(root.join("Cargo.toml"))
        .map(|t| t.contains("[workspace]"))
        .unwrap_or(false)
}

pub fn plan(root: &Path, f: &Findings) -> Plan {
    if !f.manifest_found {
        return Plan::Skip("no Cargo.toml here (run `cargo init` first)");
    }
    if is_workspace(root) {
        return Plan::Skip("Cargo workspace detected, not supported yet");
    }
    let add: Vec<String> = f
        .external
        .iter()
        .filter(|d| !d.declared)
        .map(|d| d.name.clone())
        .collect();
    if add.is_empty() {
        Plan::Nothing
    } else {
        Plan::Add(add)
    }
}

fn cargo_add(root: &Path, names: &[String]) -> bool {
    Command::new("cargo")
        .arg("add")
        .args(names)
        .current_dir(root)
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Runs `cargo add`, then checks Cargo.toml to see what actually landed.
pub fn add(root: &Path, names: &[String]) -> Vec<(String, bool)> {
    println!("Running `cargo add` ...");
    if !cargo_add(root, names) {
        // cargo add is all-or-nothing, so one bad name blocks the rest
        println!("\nBulk add failed. Retrying one crate at a time...");
        for n in names {
            cargo_add(root, std::slice::from_ref(n));
        }
    }
    let declared = cargo_manifest::read(root)
        .map(|m| m.deps)
        .unwrap_or_default();
    names.iter().map(|n| (n.clone(), declared.contains(n))).collect()
}