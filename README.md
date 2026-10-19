# devnexus

**Scan a codebase, work out which packages it needs, and set up the environment — in one command.**

`devnexus` is a command-line tool written in Rust. It reads your source files with real parsers (no LLM, no network calls to guess anything), figures out which third-party packages the project imports, writes the requirements file, creates the environment, installs everything, and then checks that the install actually worked.

> **Status: early alpha (v0.1).** It works end to end on small and medium projects, but it has real limitations — see [Limitations](#limitations). Please read them before pointing it at something you care about.

## What it does

| Language | Reads imports from | Writes | Environment | Installs with |
|---|---|---|---|---|
| **Python** | `.py` files | `requirements.txt` | `.venv/` | `pip` (inside the venv) |
| **JavaScript / TypeScript** | `.js .jsx .mjs .cjs .ts .tsx .mts .cts` | `package.json` (only if missing) | `node_modules/` | `npm`, `pnpm`, `yarn` or `bun` (picked from your lockfile) |
| **Rust** | `.rs` files | — (uses your `Cargo.toml`) | Cargo's own | `cargo add` for crates that are used but missing |

After installing, `devnexus` **verifies** the result instead of trusting exit codes: Python imports are tested inside the new venv, npm packages are checked in `node_modules/`, and Rust crates are checked in `Cargo.toml`.

## Install

You need the [Rust toolchain](https://rustup.rs). Then:

```bash
cargo install --git https://github.com/cosmicsid004/devnexus
```

Or build from source:

```bash
git clone https://github.com/cosmicsid004/devnexus
cd devnexus
cargo build --release
# binary: target/release/devnexus
```

The first build is slow because the Python parser (`ruff_python_parser`) is pulled from Git and compiled from source. Later builds are fast.

To actually set up environments you also need the tools for the languages in your project: Python 3 (with `venv`), Node.js and a package manager, and/or `cargo`.

Developed and tested on macOS. Linux and Windows are expected to work but are not yet tested.

## Usage

```bash
devnexus scan [PATH] [--verbose]
devnexus init [PATH] [--dry-run] [--force] [--yes]
```

`PATH` defaults to the current directory.

### `scan` — look, don't touch

Prints what the project uses, split into standard library, local modules (ignored) and third-party packages, and compares it with what is already declared.

```bash
devnexus scan ./my-project
devnexus scan ./my-project --verbose   # show every file:line where each package is used
```

In the Python section, `✓` means the module is already declared in `requirements.txt` / `pyproject.toml`, and `?` means it was not found under that name. A `?` is not always a real miss: the import name and the package name often differ (for example `yaml` is installed as `PyYAML`).

Imports inside `try/except` are tagged `[optional]`, and imports inside `if TYPE_CHECKING:` are tagged `[typing only]`.

### `init` — set it up

```bash
devnexus init ./my-project --dry-run   # show the plan, change nothing
devnexus init ./my-project             # show the plan, ask, then do it
```

| Flag | Meaning |
|---|---|
| `--dry-run` | Print the plan and exit. Nothing is written or installed. |
| `--force` | Overwrite an existing `requirements.txt` (**this discards any version pins in it**). |
| `-y`, `--yes` | Skip the confirmation prompt. |

Example (illustrative output):

```text
$ devnexus init ./showcase --dry-run
Python packages (requirements.txt + .venv):
  beautifulsoup4  (import bs4)
  PyYAML  (import yaml)
  requests

Rust crates missing from Cargo.toml (cargo add):
  rand
  serde_json

(dry run) Nothing changed.
```

```text
$ devnexus init ./showcase
...
Python: 3/3 verified
  ✓ beautifulsoup4 (import bs4)
  ✓ PyYAML (import yaml)
  ✓ requests

Rust: 2/2 verified
  ✓ rand
  ✓ serde_json

All done.
```

Each language is handled independently, so a failure in one does not stop the others. If any step fails, `init` exits with a non-zero status, so it is safe to use in scripts.

## How it works

1. **Walk** the project, skipping `venv`, `.venv`, `node_modules`, `target`, `__pycache__` and `.git`.
2. **Parse** each file and pull out its imports:
   - Rust with [`syn`](https://crates.io/crates/syn)
   - Python with Ruff's parser (`ruff_python_parser`), so imports nested inside functions, classes and `try` blocks are found
   - JavaScript and TypeScript with a regular expression (a placeholder — see below)
3. **Classify** each import as standard library, local to your project, or an external package.
4. **Map** Python import names to PyPI package names using a table (`cv2` → `opencv-python`, `PIL` → `Pillow`, `yaml` → `PyYAML`, …). The table is a plain data file: [`src/py_map.txt`](src/py_map.txt).
5. **Plan**, show it, and (unless `--dry-run`) ask for confirmation.
6. **Install**, then **verify**.

## Safety

`init` installs packages **by the names it finds in the code**. A name that is not what you expect could resolve to a different package on PyPI or npm. For that reason:

- Always run `--dry-run` first and read the list.
- `init` asks for confirmation unless you pass `--yes`.
- It will not overwrite an existing `requirements.txt` without `--force`.
- It will not modify a Cargo workspace.
- When a project has no `package.json`, it creates one first, so your package manager does not walk up the folder tree and edit a parent project's file.

Try it on a copy of a project, or one you trust, before using `--yes`.

## Limitations

This is an alpha. These are known and intentional gaps, not surprises:

- **It does not know which versions your code needs.** Requirements are written without version numbers, so pip installs the *latest* release. Code written against an older API can break. A real example: a hand-tracking project using MediaPipe's legacy `mp.solutions` API installed fine, and every package verified, but the newest MediaPipe releases no longer include that API, so the program failed at runtime. An import line contains a name, never a version. If your project already has pinned versions, keep your own `requirements.txt` (note that `--force` replaces it).
- **Verification is shallow.** A passing check means "installed and importable", not "your program runs".
- **Python import names are only mapped if they are in the table.** Unknown names are assumed to equal the package name, which can be wrong. Only top-level module names are matched, so namespace packages such as `google.cloud.*` or `azure.*` are not resolved correctly yet.
- **Local vs. third-party is decided by name.** A folder or file with the same name as a package makes the import look local.
- **The Python standard-library list is not version-aware.** It is a superset covering several Python versions.
- **JavaScript/TypeScript parsing uses a regular expression**, so it reads text rather than code. An import-like string inside a multi-line string or block comment can be misread. `import()` with a variable, and `importlib.import_module(name)` in Python, cannot be resolved at all.
- **No `@types/*` handling** for TypeScript, and no dev-vs-runtime dependency split.
- **Rust: only `use` and `extern crate` are seen.** A crate used purely by path (`serde_json::to_string(..)` with no `use`) is missed, which can show up as a false "possibly unused". Cargo workspaces are detected and skipped.
- **One project root only.** Monorepos and workspaces are not understood; run it on each sub-project.
- Scanning is single-threaded.

## Roadmap

- [ ] Lock exact versions after a successful verify, and respect pins from an existing `requirements.txt` / `pyproject.toml`
- [ ] Unit tests and CI on Linux, macOS and Windows
- [ ] Parallel scanning with `rayon`
- [ ] Replace the JS/TS regex with a real parser (`oxc_parser`)
- [ ] Namespace-package-aware mapping, version-aware stdlib lists
- [ ] `@types/*` suggestions for TypeScript, dev vs. runtime dependencies
- [ ] Workspace and monorepo support
- [ ] A `doctor` command: diff declared vs. used dependencies on an existing project
- [ ] Publish to crates.io (blocked while the Python parser is a Git dependency)

## Project layout

```text
src/
  main.rs            CLI (scan, init) and output
  collect.rs         turns scan results into data (no printing)
  python_imports.rs  Python import extraction (Ruff parser)
  python_classify.rs stdlib / local / third-party, declared deps
  py_map.rs, .txt    import name -> PyPI package name
  js_imports.rs      JS/TS import extraction and classification
  rust_imports.rs    Rust `use` extraction (syn)
  cargo_manifest.rs  Cargo.toml reader
  classify.rs        shared Std / Local / External type
  python_env.rs      requirements.txt, venv, pip, verify
  node_env.rs        package manager detection, install, verify
  rust_env.rs        cargo add plan
```

## Contributing

Issues and pull requests are welcome. If a scan gets a project wrong, the most useful report is the project (or a minimal snippet), the output you got, and the output you expected. Missing import-to-package mappings are the easiest first contribution: add a line to `src/py_map.txt` in the form `import_name=pip-package-name`.

## License

[MIT](LICENSE)
