use std::path::Path;
use std::sync::OnceLock;

use regex::Regex;

use crate::classify::Kind;

#[derive(Debug, Clone)]
pub struct JsImport {
    /// The raw string: "express", "lodash/fp", "./utils", "node:fs"
    pub spec: String,
    pub line: usize,
}

/// Matches the quoted specifier in all of these:
///   import x from 'pkg'      export * from 'pkg'     import 'pkg'
///   import('pkg')            require('pkg')          import x = require('pkg')
fn re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r#"(?:\bfrom\s*|\bimport\s*\(?\s*|\brequire\s*\(\s*)['"]([^'"\n]+)['"]"#)
            .unwrap()
    })
}

/// Line by line, so we know line numbers and can skip comment lines.
pub fn extract(src: &str) -> Vec<JsImport> {
    let mut out = Vec::new();
    for (i, line) in src.lines().enumerate() {
        let t = line.trim_start();
        if t.starts_with("//") || t.starts_with("/*") || t.starts_with('*') {
            continue;
        }
        for cap in re().captures_iter(line) {
            out.push(JsImport {
                spec: cap[1].to_string(),
                line: i + 1,
            });
        }
    }
    out
}

// ---------- classification ----------

const BUILTINS: &[&str] = &[
    "assert", "async_hooks", "buffer", "child_process", "cluster", "console", "constants",
    "crypto", "dgram", "diagnostics_channel", "dns", "domain", "events", "fs", "http",
    "http2", "https", "inspector", "module", "net", "os", "path", "perf_hooks", "process",
    "punycode", "querystring", "readline", "repl", "stream", "string_decoder", "sys",
    "timers", "tls", "trace_events", "tty", "url", "util", "v8", "vm", "wasi",
    "worker_threads", "zlib",
];

/// "lodash/fp" -> "lodash", "@scope/pkg/sub" -> "@scope/pkg", "@/x" -> None
fn package_name(spec: &str) -> Option<String> {
    let mut parts = spec.split('/');
    let first = parts.next()?;
    if first.is_empty() {
        return None;
    }
    let name = if let Some(scope) = first.strip_prefix('@') {
        let second = parts.next()?;
        if scope.is_empty() || second.is_empty() {
            return None; // "@/components" is an alias, not a package
        }
        format!("{first}/{second}")
    } else {
        first.to_string()
    };
    // npm names are simple. Anything odd (spaces, ${...}) is not a package.
    let valid = name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '@' | '/'));
    valid.then_some(name)
}

/// tsconfig-style absolute imports ("components/Button") that point at a
/// real folder or file in the project are local, not packages.
fn looks_local(name: &str, root: &Path) -> bool {
    [root.to_path_buf(), root.join("src")].iter().any(|base| {
        base.join(name).is_dir()
            || ["js", "jsx", "ts", "tsx", "mjs", "cjs"]
                .iter()
                .any(|ext| base.join(format!("{name}.{ext}")).is_file())
    })
}

/// Raw specifier -> (kind, package name). Anything that isn't a package
/// (URLs, aliases, relative paths) counts as Local, meaning "ignore it".
pub fn resolve(spec: &str, root: &Path) -> (Kind, String) {
    if spec.starts_with("node:") {
        return (Kind::Std, spec.to_string());
    }
    let not_a_package = spec.starts_with(['.', '/', '~', '#']) || spec.contains(':');
    if not_a_package {
        return (Kind::Local, spec.to_string());
    }
    let Some(name) = package_name(spec) else {
        return (Kind::Local, spec.to_string());
    };
    if BUILTINS.contains(&name.as_str()) {
        (Kind::Std, name)
    } else if looks_local(&name, root) {
        (Kind::Local, name)
    } else {
        (Kind::External, name)
    }
}