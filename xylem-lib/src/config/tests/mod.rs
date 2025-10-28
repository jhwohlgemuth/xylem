use crate::config::*;

#[test]
fn test_get_children() {
    assert_eq!(get_children("../../does/not/exist".to_string()), Vec::<String>::new());
    match PathBuf::from("/root/dev/command").try_exists() {
        | Ok(true) => {
            let expected = vec![
                "/root/dev/command/xylem-lib/src".to_string(),
                "/root/dev/command/xylem-lib/Cargo.toml".to_string(),
            ];
            assert_eq!(get_children("../xylem-lib".to_string()), expected);
        }
        | Ok(false) => (),
        | Err(_) => (),
    };
}
