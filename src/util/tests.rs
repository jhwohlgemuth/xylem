use crate::util::*;
use std::path::{Path, PathBuf};

#[test]
fn test_add_forward_slash() {
    assert_eq!(add_forward_slash(PathBuf::from("foo")), PathBuf::from("foo/"));
    assert_eq!(add_forward_slash(PathBuf::from("foo/bar")), PathBuf::from("foo/bar/"));
    assert_eq!(add_forward_slash(PathBuf::from("foo/bar.json")), PathBuf::from("foo/bar.json"));
}

#[test]
fn test_config() {
    let path = "tests/fixtures/config.json";
    let config = Config::read_json(PathBuf::from(path)).expect("[ERROR] => Cannot import config in tests");
    // assert_eq!(config.variables.unwrap().get("ANSWER").unwrap(), 42);
    // assert_eq!(config.modules.len(), 3);
    let _ = config.set_environment_variables();
}

#[test]
fn test_details() {
    let good_foo = Details {
        condition: true,
        description: Some("foo".to_string()),
        error: None,
    };
    let good_bar = Details {
        condition: true,
        description: Some("bar".to_string()),
        error: None,
    };
    let bad_foo = Details {
        condition: false,
        description: Some("foo".to_string()),
        error: Some("foo error".to_string()),
    };
    let bad_foo_too = Details {
        condition: false,
        description: Some("foo".to_string()),
        error: Some("foo error".to_string()),
    };
    let bad_bar = Details {
        condition: false,
        description: Some("bar".to_string()),
        error: Some("bar error".to_string()),
    };
    let bad_baz = Details {
        condition: false,
        description: Some("baz".to_string()),
        error: Some("baz error".to_string()),
    };
    let mut results = vec![good_foo, good_bar, bad_foo, bad_foo_too, bad_bar, bad_baz];
    results.dedup();
    assert_eq!(results.len(), 5);
}

#[test]
fn test_get_parent() {
    assert_eq!(get_parent(PathBuf::from("foo/bar.json")), PathBuf::from("foo"));
}

#[test]
fn test_handlebars_templates() {
    // invalid string values
    let not_upper_case = "{{ not_upper_case }}";
    let not_snake_case = "{{ NOT-SNAKE-CASE }}";
    let pascal_case = "{{ PascalCase }}";
    // is_handlebars_template
    assert!(is_handlebars_template("{{FOO}}"));
    assert!(is_handlebars_template("{{INPUT_DIRECTORY}}"));
    assert!(is_handlebars_template("{{ INPUT_DIRECTORY }}"));
    assert!(!is_handlebars_template(not_upper_case));
    assert!(!is_handlebars_template(not_snake_case));
    assert!(!is_handlebars_template(pascal_case));
    assert!(!is_handlebars_template("Not a handlebars template"));
    // parse_handlebars_template
    assert_eq!(get_variable_from_handlebars_template("{{FOO}}"), "FOO");
    assert_eq!(get_variable_from_handlebars_template("{{ FOO }}"), "FOO");
    assert_eq!(get_variable_from_handlebars_template("{{BAR}}"), "BAR");
    assert_eq!(get_variable_from_handlebars_template("{{  BAR   }}"), "BAR");
    assert_eq!(get_variable_from_handlebars_template("BAZ"), "");
    assert_eq!(get_variable_from_handlebars_template(not_upper_case), "");
    assert_eq!(get_variable_from_handlebars_template(not_snake_case), "");
    assert_eq!(get_variable_from_handlebars_template(pascal_case), "");
    // trim_handlebars_template
    let expected = "{{FOO}}";
    assert_eq!(remove_handlebars_spaces("{{FOO}}".to_owned()), expected);
    assert_eq!(remove_handlebars_spaces("{{FOO  }}".to_owned()), expected);
    assert_eq!(remove_handlebars_spaces("{{  FOO}}".to_owned()), expected);
    assert_eq!(remove_handlebars_spaces("{{  FOO  }}".to_owned()), expected);
    let expected = "FOO {{BAR}} BAZ";
    assert_eq!(remove_handlebars_spaces("FOO {{BAR}} BAZ".to_owned()), expected);
    assert_eq!(remove_handlebars_spaces("FOO {{ BAR }} BAZ".to_owned()), expected);
    assert_eq!(
        remove_handlebars_spaces("FOO {{ BAR }} BAZ {{ BIN }}".to_owned()),
        "FOO {{BAR}} BAZ {{BIN}}"
    );
}

#[test]
fn test_has_extension() {
    assert!(has_extension("txt", Path::new("foo.txt")));
    assert!(has_extension("txt", Path::new("foo/bar.txt")));
    assert!(!has_extension("txt", Path::new("foo/bar.md")));
    assert!(has_extension("", Path::new("foo")));
    assert!(has_extension("", Path::new("foo/bar")));
    assert!(has_extension("", Path::new(".foo")));
    assert!(has_extension("", Path::new("foo/.bar")))
}

#[test]
fn test_is_command() {
    let cargo: Command = Command {
        name: "cargo".to_string(),
        conda_environment: None,
    };
    let nonexistent: Command = Command {
        name: "does-not-exist".to_string(),
        conda_environment: None,
    };
    assert!(is_command(cargo));
    assert!(!is_command(nonexistent));
    // Using conda environment
    let conda: Command = Command {
        name: "conda".to_string(),
        conda_environment: None,
    };
    if is_command(conda) {
        let python: Command = Command {
            name: "python".to_string(),
            conda_environment: Some("base".to_string()),
        };
        assert!(is_command(python));
    }
}

#[test]
fn test_module_uri() {
    assert_eq!(ModuleUri::from(""), ModuleUri::Unknown);
    assert_eq!(ModuleUri::from("Not a valid URI"), ModuleUri::Unknown);
    assert_eq!(ModuleUri::from("file:///etc/to/binary"), ModuleUri::File(PathBuf::from("/etc/to/binary")));
    match ModuleUri::from("https://code.ornl.gov/GSHS/GDS/Common/PIPE/module-a") {
        | ModuleUri::Url(uri) => {
            assert_eq!(uri.scheme(), "HTTPS");
            assert_eq!(uri.path(), "/GSHS/GDS/Common/PIPE/module-a");
        }
        | _ => panic!(),
    }
}

#[test]
fn test_semantic_version() {
    let version = SemanticVersion::from_string("1.2.3");
    assert_eq!(version.major, 1);
    assert_eq!(version.minor, 2);
    assert_eq!(version.patch, 3);
    assert_eq!(format!("{}", version), "1.2.3");
}

#[test]
fn test_script_from_template() {
    let expected_value: Vec<String> = to_string(vec!["-m", "lib", "--input", "{{INPUT_DIRECTORY}}", "--output", "{{OUTPUT_DIRECTORY}}"]);
    // "shell" format arguments
    let path = PathBuf::from("tests/fixtures/template.json");
    let script = Script::from_template(path);
    assert_eq!(script.command.name, "python");
    assert_eq!(script.arguments, expected_value);
    // "exec" format arguments
    let path = PathBuf::from("tests/fixtures/template_exec.json");
    let script = Script::from_template(path);
    assert_eq!(script.command.name, "python");
    assert_eq!(script.arguments, expected_value);
}

#[test]
fn test_script_run() {
    let script = Script {
        command: Command {
            name: "echo".to_string(),
            conda_environment: None,
        },
        arguments: to_string(vec!["hello", "world"]),
    };
    let output = script.run();
    assert_eq!(String::from_utf8_lossy(&output.stdout), "hello world\n");
    let path = PathBuf::from("tests/fixtures/template_run.json");
    let script = Script::from_template(path);
    assert_eq!(script.run().stdout, b"hello world\n");
}

#[test]
fn test_to_string() {
    let input = vec!["a", "b", "c"];
    let expected_output = vec!["a".to_string(), "b".to_string(), "c".to_string()];
    assert_eq!(to_string(input), expected_output);
}
