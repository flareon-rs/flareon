use std::path::Path;

use convert_case::{Case, Casing};
use rand::rngs::StdRng;
use rand::{RngCore, SeedableRng};
use tracing::trace;

use crate::utils::print_status_msg;

macro_rules! project_file {
    ($name:literal) => {
        ($name, include_str!(concat!("project_template/", $name)))
    };
}

const PROJECT_FILES: [(&str, &str); 8] = [
    project_file!("Cargo.toml.template"),
    project_file!("bacon.toml"),
    project_file!(".gitignore"),
    project_file!("src/main.rs"),
    project_file!("static/css/main.css"),
    project_file!("templates/index.html"),
    project_file!("config/dev.toml"),
    project_file!("config/prod.toml.example"),
];

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum CotSource<'a> {
    Git,
    #[allow(dead_code)] // used in integration tests
    Path(&'a Path),
    PublishedCrate,
}

impl CotSource<'_> {
    fn as_cargo_toml_source(&self) -> String {
        match self {
            CotSource::Git => {
                "package = \"cot\", git = \"https://github.com/cot-rs/cot.git\"".to_owned()
            }
            CotSource::Path(path) => {
                format!(
                    "path = \"{}\"",
                    path.display().to_string().replace('\\', "\\\\")
                )
            }
            CotSource::PublishedCrate => format!("version = \"{}\"", env!("CARGO_PKG_VERSION")),
        }
    }
}

pub fn new_project(
    path: &Path,
    project_name: &str,
    cot_source: &CotSource<'_>,
) -> anyhow::Result<()> {
    print_status_msg("Creating", &format!("Cot project `{project_name}`"));

    if path.exists() {
        anyhow::bail!("destination `{}` already exists", path.display());
    }

    let project_struct_name = format!("{}Project", project_name.to_case(Case::Pascal));
    let app_name = format!("{}App", project_name.to_case(Case::Pascal));
    let cot_source = cot_source.as_cargo_toml_source();
    let dev_secret_key = generate_secret_key();

    for (file_name, content) in PROJECT_FILES {
        // Cargo reads and parses all files that are named "Cargo.toml" in a repository,
        // so we need a different name so that it doesn't fail on build.
        let file_name = file_name.replace(".template", "");

        let file_path = path.join(file_name);
        trace!("Writing file: {:?}", file_path);

        std::fs::create_dir_all(
            file_path
                .parent()
                .expect("joined path should always have a parent"),
        )?;

        std::fs::write(
            file_path,
            content
                .replace("{{ project_name }}", project_name)
                .replace("{{ project_struct_name }}", &project_struct_name)
                .replace("{{ app_name }}", &app_name)
                .replace("{{ cot_source }}", &cot_source)
                .replace("{{ dev_secret_key }}", &dev_secret_key),
        )?;
    }

    Ok(())
}

fn generate_secret_key() -> String {
    // Cryptographically secure random number generator:
    // https://rust-random.github.io/book/guide-rngs.html#cryptographically-secure-pseudo-random-number-generators-csprngs
    // https://cheatsheetseries.owasp.org/cheatsheets/Cryptographic_Storage_Cheat_Sheet.html#secure-random-number-generation
    let mut rng = StdRng::from_os_rng();
    let mut key = [0u8; 32];
    rng.fill_bytes(&mut key);
    hex::encode(key)
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    #[test]
    fn as_cargo_toml_source_git() {
        let source = CotSource::Git;
        assert_eq!(
            source.as_cargo_toml_source(),
            "package = \"cot\", git = \"https://github.com/cot-rs/cot.git\""
        );
    }

    #[test]
    fn as_cargo_toml_source_path() {
        let path = Path::new("/some/local/path");
        let source = CotSource::Path(path);
        assert_eq!(source.as_cargo_toml_source(), "path = \"/some/local/path\"");
    }

    #[test]
    fn as_cargo_toml_source_path_windows() {
        let path = Path::new("C:\\some\\local\\path");
        let source = CotSource::Path(path);
        assert_eq!(
            source.as_cargo_toml_source(),
            "path = \"C:\\\\some\\\\local\\\\path\""
        );
    }

    #[test]
    fn as_cargo_toml_source_published_crate() {
        let source = CotSource::PublishedCrate;
        assert_eq!(
            source.as_cargo_toml_source(),
            format!("version = \"{}\"", env!("CARGO_PKG_VERSION"))
        );
    }
}
