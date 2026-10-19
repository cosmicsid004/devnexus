use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use crate::classify::{self, Kind};
use crate::{cargo_manifest, js_imports, python_classify, python_imports, rust_imports};

// external depecndencie
#[derive(Debug, Clone)]
pub struct Dep {
    pub name: String,
    pub locs: Vec<String>, // "file:line"
    pub optional: bool,    // every use in inside try/except
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
        let Some(src) = read_source(file) else {
            continue;
        };
        match rust_imports::extract(&src) {
            Ok(found) => {
                for imp in found.imports {
                    uses.entry(imp.name).or_default().push(format!(
                        "{}: {}",
                        file.display(),
                        imp.line
                    ));
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
                    optional: false,
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
        let Some(src) = read_source(file) else {
            continue;
        };
        match python_imports::extract(&src) {
            Ok(imports) => {
                for imp in imports {
                    match python_classify::classify(&imp.name, file, root) {
                        Kind::Std => f.std_n += 1,
                        Kind::Local => f.local_n += 1,
                        Kind::External => {
                            let dep = external.entry(imp.name.clone()).or_insert_with(|| Dep {
                                name: imp.name.clone(),
                                locs: Vec::new(),
                                optional: true,
                                typing_only: true,
                                declared: false,
                            });
                            dep.locs.push(format!("{}: {}", file.display(), imp.line));
                            // one required use makes the whole dependecny required
                            dep.optional &= imp.optional;
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

    f.declared = python_classify::declared(root);
    f.manifest_found = !f.declared.is_empty();
    for dep in external.values_mut() {
        dep.declared = f
            .declared
            .contains(&dep.name.to_lowercase().replace('-', "_"));
    }
    f.external = external.into_values().collect();
    f
}

/// Names in package.json's dependency sections. None if there is no valid file.
fn package_json_deps(root: &Path) -> Option<BTreeSet<String>> {
    let text = std::fs::read_to_string(root.join("package.json")).ok()?;
    let json: serde_json::Value = serde_json::from_str(&text).ok()?;
    let mut out = BTreeSet::new();
    for section in [
        "dependencies",
        "devDependencies",
        "peerDependencies",
        "optionalDependencies",
    ] {
        if let Some(obj) = json.get(section).and_then(|s| s.as_object()) {
            out.extend(obj.keys().cloned());
        }
    }
    Some(out)
}

/// Handles .js and .ts files together, since both use the same npm packages.
pub fn collect_js(root: &Path, files: &[PathBuf]) -> Findings {
    let mut f = Findings::default();
    let mut external: BTreeMap<String, Dep> = BTreeMap::new();

    for file in files {
        if file.to_string_lossy().ends_with(".min.js") {
            continue; // minified bundles aren't source
        }
        let Some(src) = read_source(file) else {
            continue;
        };
        for imp in js_imports::extract(&src) {
            let (kind, name) = js_imports::resolve(&imp.spec, root);
            match kind {
                Kind::Std => f.std_n += 1,
                Kind::Local => f.local_n += 1,
                Kind::External => {
                    let dep = external.entry(name.clone()).or_insert_with(|| Dep {
                        name,
                        locs: Vec::new(),
                        optional: false,
                        typing_only: false,
                        declared: false,
                    });
                    dep.locs.push(format!("{}:{}", file.display(), imp.line));
                }
            }
        }
    }

    let declared = package_json_deps(root);
    f.manifest_found = declared.is_some();
    f.declared = declared.unwrap_or_default();
    for dep in external.values_mut() {
        dep.declared = f.declared.contains(&dep.name);
    }
    f.external = external.into_values().collect();
    f
}
