use crate::util::*;
use std::env::set_var;
use std::path::{Path, PathBuf};

#[test]
fn test_add_forward_slash() {
    assert_eq!(add_forward_slash(PathBuf::from("foo")), PathBuf::from("foo/"));
    assert_eq!(add_forward_slash(PathBuf::from("foo/bar")), PathBuf::from("foo/bar/"));
    assert_eq!(add_forward_slash(PathBuf::from("foo/bar.json")), PathBuf::from("foo/bar.json"));
}

#[test]
#[should_panic]
fn test_config_read_notfound() {
    let _ = Config::read(PathBuf::from("does/not/exist.json"));
}

#[test]
fn test_config_read() {
    // Load variables into configuration (JSON or YAML)
    let config = Config::read(PathBuf::from("tests/fixtures/config.json"));
    assert_eq!(config.clone().unwrap().variables.clone().unwrap().get("ANSWER").unwrap(), 42);
    assert_eq!(config.unwrap().modules.len(), 4);
    let config = Config::read(PathBuf::from("tests/fixtures/config.yaml"));
    assert_eq!(config.clone().unwrap().variables.clone().unwrap().get("ANSWER").unwrap(), 42);
    assert_eq!(config.unwrap().modules.len(), 3);
}

#[test]
fn test_config_read_json() {
    // Load variables into configuration from JSON file
    let path = "tests/fixtures/config.json";
    let config = Config::read_json(PathBuf::from(path)).expect("=> [ERROR] Cannot import config in tests");
    assert_eq!(config.variables.clone().unwrap().get("ANSWER").unwrap(), 42);
    assert_eq!(config.modules.len(), 4);
}

#[test]
fn test_config_read_yaml() {
    // Load variables into configuration from YAML file
    let path = "tests/fixtures/config.yaml";
    let config = Config::read_yaml(PathBuf::from(path)).expect("=> [ERROR] Cannot import config in tests");
    assert_eq!(config.variables.clone().unwrap().get("ANSWER").unwrap(), 42);
    assert_eq!(config.modules.len(), 3);
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
fn test_get_conda_environment_name() {
    let manifest_path = PathBuf::from("tests/fixtures/environment.yml");
    assert_eq!(
        get_conda_environment_name(manifest_path),
        Some("some-really-interesting-name".to_string())
    );
    let manifest_path = PathBuf::from("/non/existing/manifest.yml");
    assert_eq!(get_conda_environment_name(manifest_path), None);
}

#[test]
fn test_get_parent() {
    assert_eq!(get_parent(PathBuf::from("foo/bar.json")), PathBuf::from("foo"));
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
    let python = Command::init().name("python".to_string()).virtual_environment("base".to_string()).build();
    assert_eq!(python.name, "python".to_string());
    assert_eq!(python.virtual_environment, Some("base".to_string()));
    let nonexistent: Command = Command {
        name: "does-not-exist".to_string(),
        virtual_environment: None,
    };
    assert!(!is_command(nonexistent));
    if cfg!(target_os = "windows") {
        // WIP
    } else {
        let cargo = Command::init().name("cargo".to_string()).build();
        assert!(is_command(cargo));
        // Using conda virtual environment
        let conda = Command::init().name("conda".to_string()).build();
        if is_command(conda) {
            assert!(is_command(python));
        }
    }
}

#[test]
fn test_module() {
    let module = Module {
        name: "module-a".to_string(),
        module_type: ModuleType::Configuration { uri: "uri".to_string() },
        description: None,
        variables: None,
        before_script: None,
        after_script: None,
        template: None,
    };
    assert_eq!(module.to_string(), "module-a");
}

#[test]
fn test_module_uri() {
    assert_eq!(ModuleUri::from(""), ModuleUri::Unknown("".to_string()));
    assert_eq!(ModuleUri::from("Not a valid URI"), ModuleUri::Unknown("Not a valid URI".to_string()));
    assert_eq!(
        ModuleUri::from("/path/to/local/folder"),
        ModuleUri::Unknown("/path/to/local/folder".to_string())
    );
    assert_eq!(ModuleUri::from("file:///etc/to/binary"), ModuleUri::File(PathBuf::from("/etc/to/binary")));
    match ModuleUri::from("https://code.ornl.gov/GSHS/GDS/Common/PIPE/module-a") {
        | ModuleUri::Url(uri) => {
            assert_eq!(uri.scheme(), "HTTPS");
            assert_eq!(uri.path(), "/GSHS/GDS/Common/PIPE/module-a");
        }
        | _ => panic!(),
    }
    match ModuleUri::from("ssh://git@code.ornl.gov/GSHS/GDS/Common/PIPE/module-a.git") {
        | ModuleUri::Url(uri) => {
            assert_eq!(uri.scheme(), "SSH");
            assert_eq!(uri.path(), "/GSHS/GDS/Common/PIPE/module-a.git");
        }
        | _ => panic!(),
    }
    let expected = "code_ornl_gov_pipe_module-a";
    let url = ModuleUri::from("http://code.ornl.gov/PIPE/module-a");
    assert_eq!(expected, url.hash());
    let url = ModuleUri::from("https://code.ornl.gov/GSHS/GDS/Common/PIPE/module-a");
    assert_eq!(expected, url.hash());
}

#[test]
fn test_semantic_version() {
    let version = SemanticVersion::from_string("1.2.3");
    assert_eq!(version.major, 1);
    assert_eq!(version.minor, 2);
    assert_eq!(version.patch, 3);
    assert_eq!(format!("{}", version), "1.2.3");
    let default = SemanticVersion::init().build();
    assert_eq!(default.major, 0);
    assert_eq!(default.minor, 0);
    assert_eq!(default.patch, 0);
    assert_eq!(format!("{}", default), "0.0.0");
}

#[test]
fn test_script() {
    let script = Script::init().build();
    println!("{:?}", script);
    assert_eq!(script.command.name, "".to_string());
    assert_eq!(script.command.virtual_environment, None);
    assert!(script.arguments.is_empty());
    assert_eq!(script.envs, None);
}

#[test]
fn test_script_expand_arguments() {
    set_var("_PIPE_INPUT_DIRECTORY", "/path/to/input");
    set_var("_PIPE_OUTPUT_DIRECTORY", "/path/to/output");
    set_var("_PIPE_FOO", "one!");
    set_var("_PIPE_BAR", "two!");
    set_var("_PIPE_BAZ", "three!");
    let command = Command::init().name("python".to_string()).build();
    let arguments = to_string(vec![
        "-m",
        "lib",
        "--input",
        "{{ INPUT_DIRECTORY }}",
        "--output",
        "{{ OUTPUT_DIRECTORY }}",
    ]);
    let script = Script::init().command(command).arguments(arguments).build();
    assert_eq!(
        script.clone().expand_arguments().arguments,
        vec!["-m", "lib", "--input", "/path/to/input", "--output", "/path/to/output"]
    );
    let environment = "foo".to_string();
    assert_eq!(
        script.clone().expand_arguments().with_environment(Some(environment.clone())).to_string(),
        format!(
            "conda run --name {} --no-capture-output python -m lib --input /path/to/input --output /path/to/output",
            environment
        )
    );
    assert_eq!(
        script.clone().expand_arguments().with_environment(None).to_string(),
        "python -m lib --input /path/to/input --output /path/to/output"
    );
    let command = Command::init().name("echo".to_string()).build();
    let arguments = to_string(vec![
        "{{ FOO }}",
        "{{ BAR }} and",
        "{{ BAZ }}",
        "{{ FOO }} {{ BAR }} {{ BAZ }}",
        "{{FOO}}/{{BAZ}}",
    ]);
    let script = Script::init().command(command).arguments(arguments).build();
    assert_eq!(
        script.clone().expand_arguments().with_environment(None).to_string(),
        "echo one! two! and three! one! two! three! one!/three!"
    );
}

#[test]
fn test_script_expand_arguments_from() {
    let command = Command::init().name("python".to_string()).build();
    let arguments = to_string(vec![
        "-m",
        "lib",
        "--input",
        "{{ INPUT_DIRECTORY }}",
        "--output",
        "{{ OUTPUT_DIRECTORY }}",
    ]);
    let script = Script::init().command(command).arguments(arguments).build();
    // Expand from HashMap parameter instead of environment
    let envs = HashMap::from([
        ("INPUT_DIRECTORY".to_string(), "FOO".to_string()),
        ("OUTPUT_DIRECTORY".to_string(), "BAR".to_string()),
    ]);
    let environment = "foo".to_string();
    assert_eq!(
        script
            .clone()
            .expand_arguments_from(envs.clone())
            .with_environment(Some(environment.clone()))
            .to_string(),
        format!(
            "conda run --name {} --no-capture-output python -m lib --input FOO --output BAR",
            environment
        )
    );
    assert_eq!(
        script.clone().expand_arguments_from(envs.clone()).with_environment(None).to_string(),
        "python -m lib --input FOO --output BAR"
    );
    let command = Command::init().name("echo".to_string()).build();
    let arguments = to_string(vec![
        "{{ FOO }}",
        "{{ BAR }} and",
        "{{ BAZ }}",
        "{{ FOO }} {{ BAR }} {{ BAZ }}",
        "{{FOO}}/{{BAZ}}",
    ]);
    let envs = HashMap::from([
        ("FOO".to_string(), "one".to_string()),
        ("BAR".to_string(), "two".to_string()),
        ("BAZ".to_string(), "three".to_string()),
    ]);
    let script = Script::init().command(command).arguments(arguments).build();
    assert_eq!(
        script.clone().expand_arguments_from(envs.clone()).with_environment(None).to_string(),
        "echo one two and three one two three one/three"
    );
}

#[test]
fn test_script_from_template() {
    let expected_value: Vec<String> = to_string(vec!["-m", "lib", "--input", "{{INPUT_DIRECTORY}}", "--output", "{{OUTPUT_DIRECTORY}}"]);
    // "shell" format arguments
    let path = PathBuf::from("tests/fixtures/template.json");
    let script = Script::from_path(path);
    assert_eq!(script.command.name, "python");
    assert_eq!(script.arguments, expected_value);
    // "exec" format arguments
    let path = PathBuf::from("tests/fixtures/template_exec.json");
    let script = Script::from_path(path);
    assert_eq!(
        format!("{}", script.to_string()),
        "conda run --name base --no-capture-output python -m lib --input {{INPUT_DIRECTORY}} --output {{OUTPUT_DIRECTORY}}"
    );
    assert_eq!(script.command.name, "python");
    assert_eq!(script.arguments, expected_value);
}

#[test]
fn test_script_run() {
    if cfg!(target_os = "windows") {
        // WIP
    } else {
        let command = Command::init().name("echo".to_string()).build();
        let arguments = to_string(vec!["hello", "world"]);
        let script = Script::init().command(command).arguments(arguments).build();
        let output = script.run();
        assert_eq!(&output, "hello world");
        let command = Command::init().name("pwd".to_string()).build();
        let script = Script::init().command(command).build();
        let output = script.with_current_dir("/root".to_string()).run();
        assert_eq!(&output, "/root");
        let path = PathBuf::from("tests/fixtures/template_run.json");
        let script = Script::from_path(path);
        assert_eq!(script.run(), "hello world");
    }
}

#[test]
fn test_string_template_expand() {
    let vars = vec![
        ("GREETING".to_string(), "Hello".to_string()),
        ("LOCATION".to_string(), "World".to_string()),
    ];
    let expected = "Hello World!!!";
    let template = StringTemplate::new("{{ GREETING }} {{ LOCATION }}!!!");
    assert_eq!(template.expand(vars.clone()), expected);
    let template = StringTemplate::new("Hello {{ LOCATION }}!!!");
    assert_eq!(template.expand(vars.clone()), expected);
    let template = StringTemplate::new("Hello World!!!");
    assert_eq!(template.expand(vars.clone()), expected);
}

#[test]
fn test_string_template_handlebars() {
    // invalid string values
    let not_upper_case = "{{ not_upper_case }}";
    let not_snake_case = "{{ NOT-SNAKE-CASE }}";
    let pascal_case = "{{ PascalCase }}";
    // is_handlebars_template
    assert!(StringTemplate::is_handlebars(StringTemplate::new("{{FOO}}")));
    assert!(StringTemplate::is_handlebars(StringTemplate::new("{{INPUT_DIRECTORY}}")));
    assert!(StringTemplate::is_handlebars(StringTemplate::new("{{ INPUT_DIRECTORY }}")));
    assert!(!StringTemplate::is_handlebars(StringTemplate::new(not_upper_case)));
    assert!(!StringTemplate::is_handlebars(StringTemplate::new(not_snake_case)));
    assert!(!StringTemplate::is_handlebars(StringTemplate::new(pascal_case)));
    assert!(!StringTemplate::is_handlebars(StringTemplate::new("Not a handlebars template")));
    // parse_handlebars_template
    assert_eq!(StringTemplate::get_variable(StringTemplate::new("{{FOO}}")), "FOO");
    assert_eq!(StringTemplate::get_variable(StringTemplate::new("{{ FOO }}")), "FOO");
    assert_eq!(StringTemplate::get_variable(StringTemplate::new("{{BAR}}")), "BAR");
    assert_eq!(StringTemplate::get_variable(StringTemplate::new("{{  BAR   }}")), "BAR");
    assert_eq!(StringTemplate::get_variable(StringTemplate::new("BAZ")), "");
    assert_eq!(StringTemplate::get_variable(StringTemplate::new(not_upper_case)), "");
    assert_eq!(StringTemplate::get_variable(StringTemplate::new(not_snake_case)), "");
    assert_eq!(StringTemplate::get_variable(StringTemplate::new(pascal_case)), "");
    // trim_handlebars_template
    let expected = "{{FOO}}";
    assert_eq!(StringTemplate::new("{{FOO}}").remove_spaces().value, expected);
    assert_eq!(StringTemplate::new("{{FOO  }}").remove_spaces().value, expected);
    assert_eq!(StringTemplate::new("{{  FOO}}").remove_spaces().value, expected);
    assert_eq!(StringTemplate::new("{{  FOO  }}").remove_spaces().value, expected);
    let expected = "FOO {{BAR}} BAZ";
    assert_eq!(StringTemplate::new("FOO {{BAR}} BAZ").remove_spaces().value, expected);
    assert_eq!(StringTemplate::new("FOO {{ BAR }} BAZ").remove_spaces().value, expected);
    assert_eq!(
        StringTemplate::new("FOO {{ BAR }} BAZ {{ BIN }}").remove_spaces().value,
        "FOO {{BAR}} BAZ {{BIN}}"
    );
}

#[test]
fn test_to_string() {
    let input = vec!["a", "b", "c"];
    let expected_output = vec!["a".to_string(), "b".to_string(), "c".to_string()];
    assert_eq!(to_string(input), expected_output);
}
