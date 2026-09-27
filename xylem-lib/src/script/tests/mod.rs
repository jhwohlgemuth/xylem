use crate::script::{
    add_dot_slash, get_conda_environment_list, get_conda_environment_name, Command, Script, ScriptTemplate, VirtualEnvironment,
    VirtualEnvironmentManager,
};
use std::path::{Path, PathBuf};

const FIXTURES: &str = "../tests/fixtures";

#[test]
fn test_add_dot_slash() {
    assert_eq!(add_dot_slash(PathBuf::from("foo")), PathBuf::from("./foo"));
    assert_eq!(add_dot_slash(PathBuf::from("./foo")), PathBuf::from("./foo"));
    assert_eq!(add_dot_slash(PathBuf::from("foo/bar")), PathBuf::from("./foo/bar"));
    assert_eq!(add_dot_slash(PathBuf::from("foo/bar.json")), PathBuf::from("./foo/bar.json"));
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
        assert!(list.contains(&"base".to_string()));
    } else {
        assert!(list.is_empty());
    }
}
#[test]
fn test_get_conda_environment_name() {
    let path = format!("{FIXTURES}/environment.yml");
    let manifest_path = PathBuf::from(path);
    assert!(get_conda_environment_name(manifest_path)
        .unwrap()
        .starts_with("some-really-interesting-name"));
    let manifest_path = PathBuf::from("/non/existing/manifest.yml");
    assert_eq!(get_conda_environment_name(manifest_path), None);
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
