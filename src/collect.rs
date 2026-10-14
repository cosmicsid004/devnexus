use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use crate::classify::{self, Kind};
use crate::{cargo_manifest, pyhton_classify, python_imports, rust_imports};

// external depecndencie
#[derive(Debug, Clone)]
pub struct Dep {
    pub name: String,
    pub locs: Vec<String>, // "file:line"
    pub optioinal: bool,   // every use in inside try/except
    pub typing_only: bool, // every use is unside 'if TYPE_CHECKING'
    pub declared: bool,    // already in Cargo.toml / requirements.txt / pyproject.toml
}

// Everything Scan learns about one language
#[derive(Debug, Default)]
pub struct Findings {
    pub std_n: usize,
    pub local_n: usize,
    pub external: Vec<Dep>,
    pub declared: BTreeSet<String>,
    pub manifest_found: bool,
    pub unparseable: usize,
}

fn read_source(file: &Path) -> Option<String> {
    match std::fs::read_to_string(file) {
        Ok(s) => Some(s),
        Err(e) => {
            eprintln!("warning: can't read {}: {e}", file.display());
            None
        }
    }
}

pub fn collect_rust(root: &Path, files: &[PathBuf]) -> Findings {
    let mut f = Findings::default();
    let mut uses: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut local_names: BTreeSet<String> = BTreeSet::new();

    for file in files {
        let Some(src) = read_source(file) else { continue };
        match rust_imports::extract(&src) {
            Ok(found) => {
                for imp in found.imports {
                    uses.entry(imp.name)
                        .or_default()
                        .push(format!("{}: {}", file.display(), imp.line));
                }
                local_names.extend(found.modules);
            }
            Err(e) => {
                f.unparseable += 1;
                eprintln!("warning: can't parse {}: {e}", file.display());
            }
        }
    }

    let manifest = cargo_manifest::read(root);
    let own = manifest.as_ref().and_then(|m| m.package_name.as_deref());
    f.manifest_found = manifest.is_some();
    if let Some(m) = &manifest {
        f.declared = m.deps.clone();
    }

    for (name, locs) in uses {
        match classify::classify(&name, &local_names, own) {
           Kind::Std => f.std_n += locs.len(),
           Kind::Local => f.local_n += locs.len(),
           Kind::External => {
               let declared = f.declared.contains(&name);
               f.external.push(Dep {
                   name,
                   locs,
                   optioinal: false,
                   typing_only: false,
                   declared,
               });
           }
        }
    }

    f
}

pub fn collect_python(root: &Path, files: &[PathBuf]) -> Findings {
    let mut f = Findings::default();
    let mut external: BTreeMap<String, Dep> = BTreeMap::new();

    for file in files {
        let Some(src) = read_source(file) else { continue };
        match python_imports::extract(&src) {
            Ok(imports) => {
                for imp in imports {
                    match pyhton_classify::classify(&imp.name, file, root) {
                        Kind::Std => f.std_n += 1,
                        Kind::Local => f.local_n += 1,
                        Kind::External => {
                            let dep = external.entry(imp.name.clone()).or_insert_with(|| Dep {
                                name: imp.name.clone(),
                                locs: Vec::new(),
                                optioinal: true,
                                typing_only: true,
                                declared: false,
                            });
                            dep.locs.push(format!("{}: {}", file.display(), imp.line));
                            // one required use makes the whole dependecny required
                            dep.optioinal &= imp.optional;
                            dep.typing_only &= imp.typing_only;
                        }
                    }
                }
            }

            Err(e) => {
                f.unparseable += 1;
                eprintln!("warning: can't parse {}: {e}", file.display());
            }
        }
    }

    f.declared = pyhton_classify::declared(root);
    f.manifest_found = !f.declared.is_empty();
    for dep in external.values_mut() {
        dep.declared = f.declared.contains(&dep.name.to_lowercase().replace('-', "_"));
    }
    f.external = external.into_values().collect();
    f
}
