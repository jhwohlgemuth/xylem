use crate::script::{add_dot_slash, get_conda_environment_list, get_conda_environment_name, Command};
use std::path::PathBuf;

const FIXTURES: &str = "../tests/fixtures";

#[test]
fn test_add_dot_slash() {
    assert_eq!(add_dot_slash(PathBuf::from("foo")), PathBuf::from("./foo"));
    assert_eq!(add_dot_slash(PathBuf::from("./foo")), PathBuf::from("./foo"));
    assert_eq!(add_dot_slash(PathBuf::from("foo/bar")), PathBuf::from("./foo/bar"));
    assert_eq!(add_dot_slash(PathBuf::from("foo/bar.json")), PathBuf::from("./foo/bar.json"));
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
