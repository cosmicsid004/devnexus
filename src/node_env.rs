use std::path::Path;
use std::process::Command;

use anyhow::{bail, Result};

use crate::collect::Findings;

/// Which package manager does this project use? Decided by its lockfile.
#[derive(Debug, Clone, Copy)]
pub enum Pm {
    Npm,
    Pnpm,
    Yarn,
    Bun,
}

impl Pm {
    pub fn detect(root: &Path) -> Pm {
        if root.join("pnpm-lock.yaml").exists() {
            Pm::Pnpm
        } else if root.join("yarn.lock").exists() {
            Pm::Yarn
        } else if root.join("bun.lockb").exists() || root.join("bun.lock").exists() {
            Pm::Bun
        } else {
            Pm::Npm
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Pm::Npm => "npm",
            Pm::Pnpm => "pnpm",
            Pm::Yarn => "yarn",
            Pm::Bun => "bun",
        }
    }

    /// On Windows, npm/pnpm/yarn are .cmd scripts.
    fn program(self) -> String {
        if cfg!(windows) && !matches!(self, Pm::Bun) {
            format!("{}.cmd", self.name())
        } else {
            self.name().to_string()
        }
    }

    /// npm adds with `install <pkg>`, the others with `add <pkg>`.
    fn add_verb(self) -> &'static str {
        match self {
            Pm::Npm => "install",
            _ => "add",
        }
    }
}

pub struct Plan {
    pub pm: Pm,
    /// Packages used in code but missing from package.json
    pub add: Vec<String>,
    pub has_manifest: bool,
    /// package.json exists but node_modules doesn't (fresh clone)
    pub needs_install: bool,
}

impl Plan {
    pub fn is_empty(&self) -> bool {
        self.add.is_empty() && !self.needs_install
    }
}

pub fn plan(root: &Path, f: &Findings) -> Plan {
    Plan {
        pm: Pm::detect(root),
        add: f
            .external
            .iter()
            .filter(|d| !d.declared)
            .map(|d| d.name.clone())
            .collect(),
        has_manifest: f.manifest_found,
        needs_install: f.manifest_found && !root.join("node_modules").exists(),
    }
}

fn available(pm: Pm) -> bool {
    Command::new(pm.program())
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn run(pm: Pm, root: &Path, verb: &str, pkgs: &[String]) -> bool {
    Command::new(pm.program())
        .arg(verb)
        .args(pkgs)
        .current_dir(root)
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Without a package.json, npm walks UP the folder tree and may modify a
/// parent project's file. Creating one here keeps everything in this folder.
fn write_manifest(root: &Path) -> Result<()> {
    let dir = std::fs::canonicalize(root)?;
    let raw = dir.file_name().and_then(|n| n.to_str()).unwrap_or("project");
    let cleaned: String = raw
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' { c } else { '-' })
        .collect();
    let name = cleaned.trim_matches('-');
    let name = if name.is_empty() { "project" } else { name };

    let json = serde_json::json!({ "name": name, "version": "1.0.0", "private": true });
    std::fs::write(
        root.join("package.json"),
        serde_json::to_string_pretty(&json)? + "\n",
    )?;
    println!("Wrote package.json");
    Ok(())
}

/// Returns the packages that failed to install.
pub fn install(root: &Path, plan: &Plan) -> Result<Vec<String>> {
    let pm = plan.pm;
    if !available(pm) {
        bail!("{} not found. Install Node.js first (https://nodejs.org)", pm.name());
    }
    if !plan.has_manifest {
        write_manifest(root)?;
    }

    // Only a fresh clone: install what package.json already lists.
    if plan.add.is_empty() {
        if !run(pm, root, "install", &[]) {
            bail!("`{} install` failed", pm.name());
        }
        return Ok(Vec::new());
    }

    println!("Running `{} {}` ...", pm.name(), pm.add_verb());
    if run(pm, root, pm.add_verb(), &plan.add) {
        return Ok(Vec::new());
    }

    // One unknown package name fails the whole command, so retry singly.
    println!("\nBulk install failed. Retrying one package at a time...");
    let mut failed = Vec::new();
    for p in &plan.add {
        if !run(pm, root, pm.add_verb(), std::slice::from_ref(p)) {
            failed.push(p.clone());
        }
    }
    Ok(failed)
}