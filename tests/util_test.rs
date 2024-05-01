#[path = "../src/util.rs"]
#[macro_use]
mod util;

#[cfg(test)]
mod tests {
    use crate::util::{add_forward_slash, get_parent, has_extension, import_config, Details};
    use std::path::{Path, PathBuf};

    #[test]
    fn test_import_config() {
        let content = import_config(PathBuf::from("tests/fixtures/config.json"));
        assert!(content.is_ok());
    }

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
}
