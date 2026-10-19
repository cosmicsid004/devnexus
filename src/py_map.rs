use std::collections::HashMap;
use std::sync::OnceLock;

static MAP_TXT: &str = include_str!("py_map.txt");

// Parde once on the first use. Lines look like cv2=opencv-python
fn table() -> &'static HashMap<&'static str, &'static str> {
    static T: OnceLock<HashMap<&'static str, &'static str>> = OnceLock::new();
    T.get_or_init(|| {
        MAP_TXT
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty() && !l.starts_with('#'))
            .filter_map(|l| l.split_once('='))
            .map(|(k, v)| (k.trim(), v.trim()))
            .collect()
    })
}

// yaml -> PyYAML. Unknown names are assumed to match the package name.
pub fn install_name(import: &str) -> String {
    table().get(import).copied().unwrap_or(import).to_string()
}
