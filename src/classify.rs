// It helps devnexus identify whether an imported crate is built into Rust, belongs to the current project, or is an external dependency.

use std::collections::BTreeSet;

#[derive(Debug, PartialEq, Eq)]
pub enum Kind {
    Std,
    Local,
    External
}

const STD_CRATES: &[&str] = &["std", "core", "alloc", "proc_macro", "test"];
const RELATIVE: &[&str] = &["crate", "self", "super", "Self"];

pub fn classify(name: &str, local_names: &BTreeSet<String>, own_package: Option<&str>) -> Kind {
    if STD_CRATES.contains(&name) {
        Kind::Std
    } else if RELATIVE.contains(&name) || local_names.contains(name) || own_package == Some(name) {
        Kind::Local
    } else {
        Kind::External
    }
}