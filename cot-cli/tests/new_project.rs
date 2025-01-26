use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

use cot_cli::new_project::{new_project, CotSource};

#[test]
#[cfg_attr(miri, ignore)] // unsupported operation: extern static `pidfd_spawnp` is not supported by Miri
fn new_project_compile_test() {
    let temp_dir = tempfile::tempdir().unwrap();
    let project_path = temp_dir.path().join("my_project");

    let cot_cli_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let cot_workspace_path = cot_cli_path.parent().unwrap().join("cot");
    new_project(
        &project_path,
        "my_project",
        &CotSource::Path(&cot_workspace_path),
    )
    .unwrap();

    let output = cargo(&project_path)
        .arg("build")
        .arg("--quiet")
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "status: {}, stderr: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
}

fn raw_cargo() -> Command {
    match env::var_os("CARGO") {
        Some(cargo) => Command::new(cargo),
        None => Command::new("cargo"),
    }
}

fn cargo(project_path: &Path) -> Command {
    let mut cmd = raw_cargo();
    cmd.current_dir(project_path);
    cmd.env("CARGO_TARGET_DIR", project_path.join("target"));
    cmd.env_remove("RUSTFLAGS");
    cmd.env("CARGO_INCREMENTAL", "0");

    cmd
}
