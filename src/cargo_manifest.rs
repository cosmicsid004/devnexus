// It scans a Rust project's Cargo.toml and creates a clean, sorted list of its declared dependencies and package name.

use std::collections::BTreeSet;
use std::path::Path;

#[derive(Debug, Default)]
pub struct Manifest {
    pub package_name: Option<String>,
    // declared depedency names
    pub deps: BTreeSet<String>,
}

// In Cargo.toml: serde-json 
// In code: serde_json
pub fn normalize(name: &str) -> String {
    name.replace('-', "_")
}

// None if no valid Cargo.toml
pub fn read(root: &Path) -> Option<Manifest> {
    let text = std::fs::read_to_string(root.join("Cargo.toml")).ok()?;
    let table: toml::Table = text.parse().ok()?;

    let package_name = table
        .get("package")
        .and_then(|p| p.get("name"))
        .and_then(|n| n.as_str())
        .map(normalize);

    let mut deps = BTreeSet::new();
    for section in ["dependencies", "dev-dependencies", "build-dependencies"] {
        if let Some(t) = table.get(section).and_then(|v| v.as_table()) {
            for key in t.keys() {
                deps.insert(normalize(key));
            }
        }
    }

    Some(Manifest { package_name, deps })
}