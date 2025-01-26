use std::path::{Path, PathBuf};

pub(crate) fn print_status_msg(status: &str, message: &str) {
    let status_style = anstyle::Style::new() | anstyle::Effects::BOLD;
    let status_style = status_style.fg_color(Some(anstyle::Color::Ansi(anstyle::AnsiColor::Green)));

    eprintln!("{status_style}{status:>12}{status_style:#} {message}");
}

pub(crate) fn find_cargo_toml(starting_dir: &Path) -> Option<PathBuf> {
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
