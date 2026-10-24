#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used,
    clippy::arithmetic_side_effects
)]
use crate::script::{
    add_dot_slash, get_conda_environment_list, get_conda_environment_name, get_mamba_environment_list, Command, ContainerEnvironment,
    ContainerStartup, Script, ScriptTemplate, VirtualEnvironment, VirtualEnvironmentManager,
};
use std::ffi::OsString;
use std::path::{Path, PathBuf};

const FIXTURES: &str = "../../tests/fixtures";

#[test]
fn test_add_dot_slash() {
    assert_eq!(add_dot_slash(PathBuf::from("foo")), PathBuf::from("./foo"));
    assert_eq!(add_dot_slash(PathBuf::from("./foo")), PathBuf::from("./foo"));
    assert_eq!(add_dot_slash(PathBuf::from("foo/bar")), PathBuf::from("./foo/bar"));
    assert_eq!(add_dot_slash(PathBuf::from("foo/bar.json")), PathBuf::from("./foo/bar.json"));
}
#[test]
fn test_conda_and_mamba_use_input_owned_prefixes() {
    let prefix = std::env::temp_dir().join(format!("xylem-environment-prefix-{}", std::process::id()));
    let prefix_string = prefix.to_string_lossy().into_owned();
    let command = Command::init().name("python".to_string()).build();
    let conda = VirtualEnvironment::init()
        .name("ignored-name".to_string())
        .path(prefix_string.clone())
        .manager(VirtualEnvironmentManager::Conda)
        .build();
    let mamba = VirtualEnvironment::init()
        .name("ignored-name".to_string())
        .path(prefix_string.clone())
        .manager(VirtualEnvironmentManager::Mamba)
        .build();
    let script = Script::init().command(command).arguments(["-V"]).build();
    assert_eq!(
        script.clone().with_virtual_environment(Some(conda)).to_string(),
        format!("conda run --prefix {prefix_string} --no-capture-output python -V")
    );
    assert_eq!(
        script.with_virtual_environment(Some(mamba)).to_string(),
        format!("mamba run --prefix {prefix_string} python -V")
    );
}
#[test]
fn test_container_startup_resolution() {
    let image_default = ContainerEnvironment::init().name("default".to_string()).build();
    let explicit = ContainerEnvironment::init()
        .name("explicit".to_string())
        .command(vec!["serve".to_string(), "--port".to_string(), "8080".to_string()])
        .build();
    let keep_alive = ContainerEnvironment::init().name("keep-alive".to_string()).keep_alive(true).build();
    assert_eq!(image_default.resolved_startup(), (ContainerStartup::ImageDefault, Vec::new()));
    assert_eq!(
        explicit.resolved_startup(),
        (
            ContainerStartup::ExplicitCommand,
            ["serve", "--port", "8080"].map(OsString::from).to_vec()
        )
    );
    assert_eq!(
        keep_alive.resolved_startup(),
        (ContainerStartup::KeepAlive, ["sleep", "infinity"].map(OsString::from).to_vec())
    );
}
#[test]
fn test_create_conda_environment_uses_input_owned_prefix() {
    let path = PathBuf::from(format!("{FIXTURES}/environment.yml"));
    let prefix = std::env::temp_dir().join(format!("xylem-environment-create-prefix-{}", std::process::id()));
    let script = Script::maybe_create_virtual_environment(
        path.parent().expect("fixture parent").to_string_lossy().into_owned(),
        Some(
            VirtualEnvironment::init()
                .name("ignored-name".to_string())
                .path(prefix.display().to_string())
                .manager(VirtualEnvironmentManager::Conda)
                .build(),
        ),
    )
    .expect("inspect Conda environment")
    .expect("Conda manifest should produce a creation command");
    assert_eq!(
        script.to_string(),
        format!("conda env create --yes --prefix {} -f environment.yml", prefix.display())
    );
}
#[test]
fn test_create_pixi_environment() {
    let path = PathBuf::from(format!("{FIXTURES}/pixi-module"));
    let environment = VirtualEnvironment::init()
        .name("default".to_string())
        .manager(VirtualEnvironmentManager::Pixi)
        .build();
    let script = Script::maybe_create_virtual_environment(path.to_string_lossy().to_string(), Some(environment))
        .expect("inspect Pixi environment")
        .expect("Pixi manifest should produce an install command");

    assert_eq!(script.to_string(), "pixi install --environment default");
    assert!(Path::new(&script.current_dir).ends_with("tests/fixtures/pixi-module"));
}
#[test]
fn test_get_conda_environment_list() {
    let list = get_conda_environment_list();
    let conda = Command::init().name("conda".to_string()).build();
    if conda.test() {
        assert!(list.iter().all(|name| !name.is_empty()));
    } else {
        assert!(list.is_empty());
    }
}
#[test]
fn test_get_conda_environment_name() {
    let path = format!("{FIXTURES}/environment.yml");
    let manifest_path = PathBuf::from(path);
    assert!(get_conda_environment_name(manifest_path).is_some_and(|name| name.starts_with("some-really-interesting-name")));
    let manifest_path = PathBuf::from("/non/existing/manifest.yml");
    assert_eq!(get_conda_environment_name(manifest_path), None);
}
#[test]
fn test_get_mamba_environment_list() {
    let list = get_mamba_environment_list();
    let mamba = Command::init().name("mamba".to_string()).build();
    if mamba.test() {
        assert!(list.iter().all(|name| !name.is_empty()));
    } else {
        assert!(list.is_empty());
    }
}
#[test]
fn test_resolve_pixi_environment() {
    let path = PathBuf::from(format!("{FIXTURES}/pixi-module"));
    let environment = ScriptTemplate::resolve_python_virtual_environment(path)
        .expect("inspect Pixi environment")
        .expect("Pixi should be detected from pixi.toml");
    assert_eq!(environment.name, "default");
    assert_eq!(environment.manager, VirtualEnvironmentManager::Pixi);
}
#[test]
fn test_resolve_pixi_environment_from_pyproject() {
    let path = PathBuf::from(format!("{FIXTURES}/pixi-pyproject-module"));
    let environment = ScriptTemplate::resolve_python_virtual_environment(path)
        .expect("inspect Pixi environment")
        .expect("Pixi should be detected from pyproject.toml");
    assert_eq!(environment.name, "default");
    assert_eq!(environment.manager, VirtualEnvironmentManager::Pixi);
}
#[test]
fn test_virtual_environment_manager_from_reference() {
    fn implements_try_from<T: for<'a> TryFrom<&'a str>>() {}
    implements_try_from::<VirtualEnvironmentManager>();
    assert_eq!(VirtualEnvironmentManager::from("conda"), VirtualEnvironmentManager::Conda);
    assert_eq!(VirtualEnvironmentManager::from("mamba"), VirtualEnvironmentManager::Mamba);
    assert_eq!(VirtualEnvironmentManager::from("unsupported"), VirtualEnvironmentManager::Unknown);
}
