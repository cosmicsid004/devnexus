use core::str;
use std::fmt::Debug;
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Result, bail};
use clap::{Parser, Subcommand};
use ignore::WalkBuilder;

mod cargo_manifest;
mod classify;
mod collect;
mod js_imports;
mod node_env;
mod py_map;
mod python_classify;
mod python_env;
mod python_imports;
mod rust_env;
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

    // Write requirements.txt, create a .venv ans install (Pyhton for now)
    Init {
        #[arg(default_value = ".")]
        path: PathBuf,

        // Show the plan without changinf anything
        #[arg(long)]
        dry_run: bool,

        // Overwrite an existing requirements.txt
        #[arg(long)]
        force: bool,

        // Skip the confirmation prompt
        #[arg(short, long)]
        yes: bool,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Lang {
    Rust,
    Python,
    JavaScript,
    TypeScript,
}

impl Lang {
    fn from_path(path: &Path) -> Option<Lang> {
        match path.extension()?.to_str()? {
            "rs" => Some(Lang::Rust),
            "py" => Some(Lang::Python),
            "js" | "jsx" | "mjs" | "cjs" => Some(Lang::JavaScript),
            "ts" | "tsx" | "mts" | "cts" => Some(Lang::TypeScript),
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
    println!(
        "\nRust: {} std imports, {} local imports (ignored)",
        f.std_n, f.local_n
    );
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
        let unused: Vec<&String> = f
            .declared
            .iter()
            .filter(|d| !used.contains(d.as_str()))
            .collect();

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
    println!(
        "\nPython: {} stdlib imports, {} local imports (ignored)",
        f.std_n, f.local_n
    );
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
        } else if dep.optional {
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
        println!(
            " ? = not found by that name (may be declared under different package name, e.g. cv2 -> opencv-python)"
        );
        if f.declared.is_empty() {
            println!(" (no requirements.txt or pyptoject.toml dependencies found)");
        }
        if f.unparseable > 0 {
            println!("\n{} Pyhton file(s) could not be parsed", f.unparseable);
        }
    }
}

/// JS and TS share npm packages, so we treat them as one group.
fn js_files(found: &BTreeMap<Lang, Vec<PathBuf>>) -> Vec<PathBuf> {
    [Lang::JavaScript, Lang::TypeScript]
        .iter()
        .filter_map(|l| found.get(l))
        .flatten()
        .cloned()
        .collect()
}

fn print_js(f: &Findings, verbose: bool) {
    println!(
        "\nJS/TS: {} Node built-in imports, {} local imports (ignored)",
        f.std_n, f.local_n
    );
    println!("npm packages used:");
    for dep in &f.external {
        let mark = if dep.declared { "✓" } else { "+" };
        println!("  {mark} {} ({}x)", dep.name, dep.locs.len());
        if verbose {
            print_locs(&dep.locs);
        }
    }
    println!("\n  ✓ = in package.json    + = missing (init will add it)");
    if !f.manifest_found {
        println!("  (no valid package.json found)");
    }
}

fn confirm(prompt: &str) -> Result<bool> {
    print!("{prompt}");
    std::io::stdout().flush()?;
    let mut line = String::new();
    std::io::stdin().read_line(&mut line)?;
    Ok(matches!(line.trim(), "y" | "Y" | "yes"))
}

type Checks = Vec<(String, bool)>;

fn init_python(root: &Path, pkgs: &[python_env::Package], force: bool) -> Result<Checks> {
    let req = python_env::write_requirements(root, pkgs, force)?;
    println!("Wrote {}", req.display());
    let py = python_env::create_venv(root)?;
    python_env::install(&py, &req, pkgs);
    Ok(python_env::verify(&py, pkgs))
}

fn init_node(root: &Path, plan: &node_env::Plan) -> Result<Checks> {
    node_env::install(root, plan)?;
    Ok(node_env::verify(root, plan))
}

/// Prints ✓/✗ per item. Returns true if everything worked.
fn report_result(label: &str, result: Result<Checks>) -> bool {
    match result {
        Ok(checks) => {
            let bad = checks.iter().filter(|(_, ok)| !ok).count();
            println!(
                "\n{label}: {}/{} verified",
                checks.len() - bad,
                checks.len()
            );
            for (name, ok) in &checks {
                println!("  {} {name}", if *ok { "✓" } else { "✗" });
            }
            if bad > 0 {
                println!(
                    "  (✗ = not installed or not importable. The name may not be a real package.)"
                );
            }
            bad == 0
        }
        Err(e) => {
            println!("\n{label}: ✗ {e:#}");
            false
        }
    }
}

fn init(root: &Path, dry_run: bool, force: bool, yes: bool) -> Result<()> {
    let found = scan(root)?;

    // 1. Decide what we'd do, without touching anything
    let py_pkgs: Vec<python_env::Package> = match found.get(&Lang::Python) {
        Some(files) => python_env::plan(&collect::collect_python(root, files)),
        None => Vec::new(),
    };
    let js = js_files(&found);
    let node_plan = if js.is_empty() {
        None
    } else {
        let p = node_env::plan(root, &collect::collect_js(root, &js));
        if p.is_empty() { None } else { Some(p) }
    };
    let rust_plan = match found.get(&Lang::Rust) {
        Some(files) => rust_env::plan(root, &collect::collect_rust(root, files)),
        None => rust_env::Plan::Nothing,
    };

    let rust_work = matches!(rust_plan, rust_env::Plan::Add(_));
    if py_pkgs.is_empty() && node_plan.is_none() && !rust_work {
        if let rust_env::Plan::Skip(why) = &rust_plan {
            println!("Rust: skipped, {why}");
        }
        println!("Nothing to install.");
        return Ok(());
    }

    // 2. Show the plan
    if !py_pkgs.is_empty() {
        println!("Python packages (requirements.txt + .venv):");
        for p in &py_pkgs {
            if p.import_name != p.install_name {
                println!("  {}  (import {})", p.install_name, p.import_name);
            } else {
                println!("  {}", p.install_name);
            }
        }
    }
    if let Some(p) = &node_plan {
        println!("\nNode packages ({}):", p.pm.name());
        for n in &p.add {
            println!("  {n}");
        }
        if p.add.is_empty() {
            println!(
                "  (all already in package.json, will run `{} install`)",
                p.pm.name()
            );
        }
        if !p.has_manifest {
            println!("  (no package.json yet, one will be created)");
        }
    }
    match &rust_plan {
        rust_env::Plan::Add(names) => {
            println!("\nRust crates missing from Cargo.toml (cargo add):");
            for n in names {
                println!("  {n}");
            }
        }
        rust_env::Plan::Skip(why) => println!("\nRust: skipped, {why}"),
        rust_env::Plan::Nothing => {}
    }

    if dry_run {
        println!("\n(dry run) Nothing changed.");
        return Ok(());
    }
    if !yes && !confirm("\nProceed? [y/N] ")? {
        println!("Cancelled.");
        return Ok(());
    }

    // 3. Do it. Each part reports on its own, so one failure doesn't block the others.
    let mut all_ok = true;
    if !py_pkgs.is_empty() {
        all_ok &= report_result("Python", init_python(root, &py_pkgs, force));
        if root.join(".venv").exists() {
            if cfg!(windows) {
                println!("  activate: {}\\.venv\\Scripts\\activate", root.display());
            } else {
                println!("  activate: source {}/.venv/bin/activate", root.display());
            }
        }
    }
    if let Some(p) = &node_plan {
        all_ok &= report_result("Node", init_node(root, p));
    }
    if let rust_env::Plan::Add(names) = &rust_plan {
        all_ok &= report_result("Rust", Ok(rust_env::add(root, names)));
    }

    println!();
    if all_ok {
        println!("All done.");
        Ok(())
    } else {
        bail!("some steps did not complete, see ✗ above")
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

            let js = js_files(&found);
            if !js.is_empty() {
                let findings = collect::collect_js(&path, &js);
                print_js(&findings, verbose);
            }
        }

        Command::Init {
            path,
            dry_run,
            force,
            yes,
        } => init(&path, dry_run, force, yes)?,
    }

    Ok(())
}
