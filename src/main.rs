use std::fmt::{Debug};
use std::path::{Path, PathBuf};

use anyhow::Result;
use clap::{Parser, Subcommand};
use ignore::WalkBuilder;

mod rust_import;
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

fn report_rust(files: &[PathBuf], verbose: bool) {
    // crate name -> list of "file:line"
    let mut uses: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut modules: BTreeSet<String> = BTreeSet::new();
    let mut unparseable = 0;

    for file in files {
        let src = match std::fs::read_to_string(file) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("warning: can't read {}: {e}", file.display());
                continue;
            }
        };

        match rust_import::extract(&src) {
            Ok(found) => {
                for imp in found.imports {
                    uses.entry(imp.name)
                        .or_default()
                        .push(format!("{}:{}", file.display(), imp.line));
                }
                modules.extend(found.modules);
            }

            Err(e) => {
                unparseable += 1;
                eprintln!("warning: can't parse {}: {e}", file.display());
            }
        }
    }

    println!("\nRust imports (first path segment)");
    for (name, locations) in &uses {
        println!("{name} ({}x)", locations.len());
        if verbose {
            for loc in locations {
                println!("{loc}");
            }
        }
    }

    if !modules.is_empty() {
        println!("\nLocal modules (mod foo;): {}", modules.iter().cloned().collect::<Vec<_>>().join(", "));
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
                report_rust(files, verbose);
            }
        }
    }

    Ok(())
}
