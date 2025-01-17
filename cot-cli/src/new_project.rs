use std::path::Path;

use convert_case::{Case, Casing};
use tracing::trace;

use crate::utils::print_status_msg;

macro_rules! project_file {
    ($name:literal) => {
        ($name, include_str!(concat!("project_template/", $name)))
    };
}

const PROJECT_FILES: [(&str, &str); 6] = [
    project_file!("Cargo.toml"),
    project_file!("bacon.toml"),
    project_file!(".gitignore"),
    project_file!("src/main.rs"),
    project_file!("static/css/main.css"),
    project_file!("templates/index.html"),
];

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum CotSource {
    Git,
    PublishedCrate,
}

pub fn new_project(path: &Path, project_name: &str, cot_source: CotSource) -> anyhow::Result<()> {
    print_status_msg("Creating", &format!("Cot project `{project_name}`"));

    if path.exists() {
        anyhow::bail!("destination `{}` already exists", path.display());
    }

    let app_name = format!("{}App", project_name.to_case(Case::Pascal));
    let cot_source = match cot_source {
        CotSource::Git => {
            "package = \"cot\", git = \"https://github.com/cot-rs/cot.git\"".to_owned()
        }
        CotSource::PublishedCrate => format!("version = \"{}\"", env!("CARGO_PKG_VERSION")),
    };

    for (file_name, content) in PROJECT_FILES {
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
                .replace("{{ app_name }}", &app_name)
                .replace("{{ cot_source }}", &cot_source),
        )?;
    }

    Ok(())
}
