use crate::util::*;
use std::path::{Path, PathBuf};

#[test]
fn test_add_forward_slash() {
    assert_eq!(add_forward_slash(PathBuf::from("foo")), PathBuf::from("foo/"));
    assert_eq!(add_forward_slash(PathBuf::from("foo/bar")), PathBuf::from("foo/bar/"));
    assert_eq!(add_forward_slash(PathBuf::from("foo/bar.json")), PathBuf::from("foo/bar.json"));
}

#[test]
fn test_get_parent() {
    assert_eq!(get_parent(PathBuf::from("foo/bar.json")), PathBuf::from("foo"));
}

#[test]
fn test_has_extension() {
    assert!(has_extension("txt", Path::new("foo.txt")));
    assert!(has_extension("txt", Path::new("foo/bar.txt")));
    assert!(has_extension("", Path::new("foo")));
    assert!(has_extension("", Path::new("foo/bar")));
    assert!(has_extension("", Path::new(".foo")));
    assert!(has_extension("", Path::new("foo/.bar")))
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
    assert_eq!(trim_handlebars_template("{{FOO}}".to_owned()), expected);
    assert_eq!(trim_handlebars_template("{{FOO  }}".to_owned()), expected);
    assert_eq!(trim_handlebars_template("{{  FOO}}".to_owned()), expected);
    assert_eq!(trim_handlebars_template("{{  FOO  }}".to_owned()), expected);
    let expected = "FOO {{BAR}} BAZ";
    assert_eq!(trim_handlebars_template("FOO {{BAR}} BAZ".to_owned()), expected);
    assert_eq!(trim_handlebars_template("FOO {{ BAR }} BAZ".to_owned()), expected);
    assert_eq!(
        trim_handlebars_template("FOO {{ BAR }} BAZ {{ BIN }}".to_owned()),
        "FOO {{BAR}} BAZ {{BIN}}"
    );
}

#[test]
fn test_to_string() {
    let input = vec!["a", "b", "c"];
    let expected_output = vec!["a".to_string(), "b".to_string(), "c".to_string()];
    assert_eq!(to_string(input), expected_output);
}
