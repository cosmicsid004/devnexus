use core::str;
use std::fmt::{Debug};
use std::path::{Path, PathBuf};

use anyhow::Result;
use clap::{Parser, Subcommand};
use ignore::WalkBuilder;

mod collect;
mod pyhton_classify;
mod python_imports;
mod cargo_manifest;
mod classify;
mod rust_imports;
use std::collections::{BTreeMap, BTreeSet};

use collect::Findings;

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

fn print_locs(locs: &[String]) {
    for loc in locs {
        println!(" {loc}");
    }
}

fn print_rust(root: &Path, f: &Findings, verbose: bool) {
    println!("\nRust: {} std imports, {} local imports (ignored)", f.std_n, f.local_n);
    println!("External crates used:");
    for dep in &f.external {
        println!(" {} ({}x)", dep.name, dep.locs.len());
        if verbose {
            print_locs(&dep.locs);
        }
    }

    if !f.manifest_found {
        println!("\nNo valid Crgo.toml in {}, skipping diff.", root.display());
    } else {
        let missing: Vec<&str> = f
            .external
            .iter()
            .filter(|d| !d.declared)
            .map(|d| d.name.as_str())
            .collect();
         let used: BTreeSet<&str> = f.external.iter().map(|d| d.name.as_str()).collect();
         let unused: Vec<&String> = f.declared.iter().filter(|d| !used.contains(d.as_str())).collect();

         if missing.is_empty() && unused.is_empty() {
             println!("\nCargo.toml matches the code")
         }
         if !missing.is_empty() {
             println!("\nMissing from Cargo.toml (try 'cargo add <name>):");
             for n in missing {
                 println!(" - {n}");
             }
         }
         if !unused.is_empty() {
             println!("\nDeclared but never imported (possibly unused):");
             for n in unused {
                 println!(" - {n}");
             }
         }
    }

    if f.unparseable > 0 {
        println!("\n{} Rust file(s) could not be parsed", f.unparseable);
    }
}


fn print_python(f: &Findings, verbose: bool) {
    println!("\nPython: {} stdlib imports, {} local imports (ignored)", f.std_n, f.local_n);
    println!("Third-party modules used: ");
    let mut undeclared = 0;
    for dep in &f.external {
        let mark = if dep.declared {
            "✓"
        } else {
            undeclared += 1;
            "?"
        };
        let tag = if dep.typing_only {
            " [typing only]"
        } else if dep.optioinal {
            " [optional]"
        } else {
            ""
        };
        println!(" {mark} {} ({}x){tag}", dep.name, dep.locs.len());
        if verbose {
            print_locs(&dep.locs);
        }
    }

    println!("\n ✓ = declared in requirements.txt / pyproject.toml");
    if undeclared > 0 {
        println!(" ? = not found by that name (may be declared under different under a different package name, e.g. cv2 -> opencv-python)");
        if f.declared.is_empty() {
            println!(" (no requirements.txt or pyptoject.toml dependencies found)");
        }
        if f.unparseable > 0 {
            println!("\n{} Pyhton file(s) could not be parsed", f.unparseable);
        }
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
                let findings = collect::collect_rust(&path, files);
                print_rust(&path, &findings, verbose);
            }

            if let Some(files) = found.get(&Lang::Python) {
                let findings = collect::collect_python(&path, files);
                print_python(&findings, verbose);
            } 
        }
    }

    Ok(())
}
