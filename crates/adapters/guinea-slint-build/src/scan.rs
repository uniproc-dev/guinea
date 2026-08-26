//! Which `.slint` file declares which component, and what it asks for.
//!
//! The route declaration names Rust types; the markup names components. They
//! are matched by name, so what is needed here is where each exported
//! component lives - and whether a layout wants to be told which route is
//! showing.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use walkdir::WalkDir;

#[derive(Clone, Debug)]
pub(crate) struct Component {
    pub file: PathBuf,
    /// Declares `in property <RouteId> route`, so the generated tree passes it
    /// down - what a tab strip needs to know which of its tabs is current.
    pub takes_route: bool,
}

/// Every exported component under `root`, by name.
pub(crate) fn components_under(root: &Path) -> anyhow::Result<HashMap<String, Component>> {
    let mut found = HashMap::new();

    for entry in WalkDir::new(root).into_iter().filter_map(Result::ok) {
        let path = entry.path();
        if path.extension().is_none_or(|extension| extension != "slint") {
            continue;
        }

        let source = fs::read_to_string(path)?;
        let takes_route = asks_for_the_route(&source);

        for name in exported_in(&source) {
            found.insert(
                name,
                Component {
                    file: path.to_path_buf(),
                    takes_route,
                },
            );
        }
    }

    Ok(found)
}

/// The components a file exports.
///
/// A scan and not a parse: the compiler's own parser is not published, and
/// what is needed is one line's worth of shape. Renaming exports
/// (`export { Inner as Outer }`) is not followed.
fn exported_in(source: &str) -> Vec<String> {
    source
        .lines()
        .map(str::trim)
        .filter_map(|line| {
            let rest = line.strip_prefix("export component ")?;
            let name = rest
                .split_once(" inherits ")
                .map(|(name, _)| name)
                .unwrap_or(rest)
                .trim_end_matches(['{', ' '])
                .trim();
            (!name.is_empty()).then(|| name.to_string())
        })
        .collect()
}

fn asks_for_the_route(source: &str) -> bool {
    source
        .lines()
        .map(str::trim)
        .any(|line| line.starts_with("in property <RouteId> route"))
}

#[cfg(test)]
mod tests {
    use super::{asks_for_the_route, exported_in};

    #[test]
    fn finds_components_with_and_without_a_base() {
        let source = "
            export component ProcessesPage inherits Window { }
            export component TabsLayout {
            }
            component Row inherits Rectangle { }
        ";

        // `Row` is not exported, so nothing outside its file can name it.
        assert_eq!(exported_in(source), vec!["ProcessesPage", "TabsLayout"]);
    }

    #[test]
    fn a_commented_out_component_is_not_one() {
        assert!(exported_in("// export component Ghost { }").is_empty());
    }

    #[test]
    fn a_layout_asking_for_the_route_is_recognised() {
        assert!(asks_for_the_route("    in property <RouteId> route;"));
        assert!(!asks_for_the_route("    in property <int> current;"));
    }
}
