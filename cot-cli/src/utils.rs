use std::path::{Path, PathBuf};

pub fn find_cargo_toml(starting_dir: &Path) -> Option<PathBuf> {
    let mut current_dir = starting_dir;

    loop {
        let candidate = current_dir.join("Cargo.toml");
        if candidate.exists() {
            return Some(candidate);
        }

        match current_dir.parent() {
            Some(parent) => current_dir = parent,
            None => break,
        }
    }

    None
}
