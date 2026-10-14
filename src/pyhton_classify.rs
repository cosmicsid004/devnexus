use std::collections::{BTreeSet, HashSet};
use std::path::Path;
use std::sync::OnceLock;

use crate::classify::Kind;

static STDLIB_TXT: &str = include_str!("stdlib_py.txt");

// Built once on first use 
fn stdlib() -> &'static HashSet<&'static str> {
    static SET: OnceLock<HashSet<&'static str>> = OnceLock::new();
    SET.get_or_init(|| STDLIB_TXT.split_whitespace().collect())
} 

// Does 'dir' contain 'name.py' or a folder called 'name'
fn exists_as_module(dir: &Path, name: &str) -> bool {
    dir.join(format!("{name}.py")).is_file() || dir.join(name).is_dir()
}

// Std local or externla ?
pub fn classify(name: &str, file: &Path, root: &Path) -> Kind {
    if stdlib().contains(name) {
        return Kind::Std;
    }
    let here = file.parent().unwrap_or(root);
    let src = root.join("src");
    if exists_as_module(here, name) || exists_as_module(root, name) || exists_as_module(&src, name) {
        Kind::Local
    } else {
        Kind::External
    }
}

// declared dependecies
fn req_name(line: &str) -> Option<String> {
    let line = line.trim();
    if line.is_empty() || line.starts_with('#') || line.starts_with('-') {
        return None;
    }
    let name: String = line
        .chars()
        .take_while(|c| c.is_alphanumeric() || matches!(c, '-' | '_' | '.'))
        .collect();
    if name.is_empty() {
        None
    } else {
        Some(name.to_lowercase().replace(['-', '.'], "_"))
    }
}

pub fn declared(root: &Path) -> BTreeSet<String> {
    let mut out = BTreeSet::new();

    if let Ok(text) = std::fs::read_to_string(root.join("requirements.txt")) {
        out.extend(text.lines().filter_map(req_name));
    }

    if let Ok(text) = std::fs::read_to_string(root.join("pyproject.toml")) {
        if let Ok(table) = text.parse::<toml::Table>() {
            if let Some(arr) = table 
                .get("project")
                .and_then(|p| p.get("dependecies"))
                .and_then(|d| d.as_array()) 
            {
                out.extend(arr.iter().filter_map(|v| v.as_str()).filter_map(req_name));
            }

            if let Some(t) = table 
                .get("tool")
                .and_then(|t| t.get("poetry"))
                .and_then(|p| p.get("dependencies"))
                .and_then(|d| d.as_table())
            {
                out.extend(t.keys().filter(|k| *k != "python").filter_map(|k| req_name(k)));
            }
        }
    } 
    out
}
