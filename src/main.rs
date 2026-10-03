use std::collections::BTreeMap;
use std::fmt::Debug;
use std::path::{Path, PathBuf};

use anyhow::Result;
use clap::{Parser, Subcommand};
use ignore::WalkBuilder;

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

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::Scan { path } => {
            let found = scan(&path)?;
            if found.is_empty() {
                println!("No Rust or Python files found.")
            }

            for (lang, files) in &found {
                println!("{:?}: {} files", lang, files.len());
            }
        }
    }

    Ok(())
}
