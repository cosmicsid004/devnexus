use std::fmt::{Debug, format};
use std::path::{Path, PathBuf};

use anyhow::Result;
use clap::{Parser, Subcommand};
use ignore::WalkBuilder;

mod cargo_manifest;
mod classify;
mod rust_imports;
use std::collections::{BTreeMap, BTreeSet};

// folders that we will never scan
const SKIP_DIRS: &[&str] = &[
    "venv",
    ".venv",
    "node_modules",
    "target",
    "__pycache__",
    ".git",
];

#[derive(Parser)] // takes the arg from terminal to this struct
#[command(
    name = "devnexus",
    about = "Scan projects and figure out what it needs"
)] // Configures the CLI

struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    // Scaning the project
    Scan {
        // folder to scan
        #[arg(default_value = ".")]
        path: PathBuf,

        // show every file:line where each crate is used
        #[arg(short, long)]
        verbose: bool,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Lang {
    Rust,
    Python,
    JavaScript,
    TypeScript
}

impl Lang {
    fn from_path(path: &Path) -> Option<Lang> {
        match path.extension()?.to_str()? {
            "rs" => Some(Lang::Rust),
            "py" => Some(Lang::Python),
            "js" => Some(Lang::JavaScript),
            "ts" => Some(Lang::TypeScript),
            _ => None,
        }
    }
}

fn scan(root: &Path) -> Result<BTreeMap<Lang, Vec<PathBuf>>> {
    // Rust -> [main.rs, lib.rs]
    // Pyhton -> [main.py, app.py]
    let mut found: BTreeMap<Lang, Vec<PathBuf>> = BTreeMap::new();

    let walker = WalkBuilder::new(root) // scanner in root dir
        .filter_entry(|e| {
            e.file_name()
                .to_str()
                .map_or(true, |n| !SKIP_DIRS.contains(&n))
        })
        .build(); // builds the walker with configured filters

    // for entry in walker {
    //     println!("{:?}", entry?);
    // }

    for entry in walker {
        let entry = entry?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        if let Some(lang) = Lang::from_path(path) {
            found.entry(lang).or_default().push(path.to_path_buf());
        }
    }

    Ok(found)
}

fn report_rust(root: &Path, files: &[PathBuf], verbose: bool) {
    // crate name -> list of "file:line"
    let mut uses: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut local_names: BTreeSet<String> = BTreeSet::new();
    let mut unparseable = 0;

    for file in files {
        let src = match std::fs::read_to_string(file) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("warning: can't read {}: {e}", file.display());
                continue;
            }
        };

        match rust_imports::extract(&src) {
            Ok(found) => {
                for imp in found.imports {
                    uses.entry(imp.name)
                        .or_default()
                        .push(format!("{}:{}", file.display(), imp.line));
                }
                local_names.extend(found.modules);
            }

            Err(e) => {
                unparseable += 1;
                eprintln!("warning: can't parse {}: {e}", file.display());
            }
        }
    }

    // -- classify

    let manifest = cargo_manifest::read(root);
    let own = manifest.as_ref().and_then(|m| m.package_name.as_deref()); // current project name

    let mut external: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let (mut std_n, mut local_n) = (0, 0);
    for (name, locs) in uses {
        match classify::classify(&name, &local_names, own) {
            classify::Kind::Std => std_n += locs.len(),
            classify::Kind::Local => local_n += locs.len(),
            classify::Kind::External => {
                external.insert(name, locs);
            }
        }
    }

    println!("\nRust: {std_n} std imports, {local_n} local imports (ignored)");
    println!("External crates used");
    for (name, locs) in &external {
        println!(" {name} ({}x)", locs.len());
        if verbose {
            for loc in locs {
                println!(" {loc}");
            }
        }
    }

    // diff against Cargo.toml
    // compares the external crates actually imported in Rust code against the dependencies declared in Cargo.toml
    match &manifest {
        None => println!("\n No valid Cargo.toml in {}, skipping diff.", root.display()),
        Some(m) => {
            let missing: Vec<&String> = external.keys().filter(|n| !m.deps.contains(*n)).collect();
            let unused: Vec<&String> = m.deps.iter().filter(|d| !external.contains_key(*d)).collect();

            if missing.is_empty() && unused.is_empty() {
                println!("\nCargo.toml matches the code");
            }

            if !missing.is_empty() {
                println!("\nMissing from Cargo.toml (try `cargo add <name>`):");
                for n in missing {
                    println!(" + {n}")
                }
            }

            if !unused.is_empty() {
                println!("\nDeclared but never imported (possibly unused)");
                for n in unused {
                    println!(" - {n}")
                }
            }
        }
    }

    if unparseable > 0 {
        println!("\n{unparseable} file(s) could not be parsed")
    }
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::Scan { path, verbose } => {
            let found = scan(&path)?;
            if found.is_empty() {
                println!("No Rust or Python files found.")
            }

            for (lang, files) in &found {
                println!("{:?}: {} files", lang, files.len());
            }

            if let Some(files) = found.get(&Lang::Rust) {
                report_rust(&path, files, verbose);
            }
        }
    }

    Ok(())
}
