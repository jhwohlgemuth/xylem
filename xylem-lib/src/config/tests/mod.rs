use crate::config::*;

#[test]
fn test_parse_sha256() {
    let expected = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
    let parsed = parse_sha256(&format!("SHA256:{}", expected.to_ascii_uppercase())).expect("valid SHA-256 should parse");

    assert_eq!(parsed.algorithm, ChecksumAlgorithm::Sha256);
    assert_eq!(parsed.checksum_value, expected);
    assert!(parse_sha256("not-a-checksum").is_none());
    assert!(parse_sha256(&format!("sha512:{expected}")).is_none());
}

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
