#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used,
    clippy::arithmetic_side_effects
)]
use super::*;
use std::fs::{create_dir_all, metadata, read, remove_dir_all, write};
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::process;
use tar::{Builder, EntryType, Header};

fn append(builder: &mut Builder<File>, path: &str, contents: &[u8]) {
    let mut header = Header::new_gnu();
    header.set_entry_type(EntryType::Regular);
    header.set_mode(0o755);
    header.set_size(contents.len() as u64);
    header.set_cksum();
    builder.append_data(&mut header, path, contents).expect("append fixture entry");
}
fn append_link(builder: &mut Builder<File>, path: &str, target: &str) {
    let mut header = Header::new_gnu();
    header.set_entry_type(EntryType::Symlink);
    header.set_mode(0o777);
    header.set_size(0);
    header.set_link_name(target).expect("set fixture link target");
    header.set_cksum();
    builder.append_data(&mut header, path, io::empty()).expect("append fixture link");
}
fn append_root(builder: &mut Builder<File>) {
    let mut header = Header::new_gnu();
    header.set_entry_type(EntryType::Directory);
    header.set_mode(0o755);
    header.set_size(0);
    header.set_cksum();
    builder.append_data(&mut header, ".", io::empty()).expect("append fixture root");
}
fn root(label: &str) -> PathBuf {
    std::env::temp_dir().join(format!("xylem-sandbox-archive-{label}-{}", process::id()))
}

#[test]
fn create_export_builds_empty_writable_file() {
    let directory = root("export");
    create_dir_all(&directory).expect("create fixture directory");
    let archive = directory.join("export.tar");
    create_export(&archive).expect("create export archive");
    let archive_metadata = metadata(&archive).expect("inspect export archive");
    assert_eq!(archive_metadata.len(), 0);
    #[cfg(unix)]
    assert_eq!(archive_metadata.permissions().mode() & 0o777, 0o666);
    remove_dir_all(directory).expect("remove fixture directory");
}
#[test]
fn extraction_accepts_the_gnu_tar_root_directory_entry() {
    let directory = root("root-entry");
    create_dir_all(&directory).expect("create fixture directory");
    let archive = directory.join("prepared.tar");
    let mut builder = Builder::new(File::create(&archive).expect("create fixture archive"));
    append_root(&mut builder);
    append(&mut builder, "module", b"contents");
    builder.finish().expect("finish fixture archive");
    let destination = directory.join("output");
    extract(&archive, &destination, 1024, 10).expect("extract archive with root entry");
    assert_eq!(read(destination.join("module")).expect("read extracted module"), b"contents");
    remove_dir_all(directory).expect("remove fixture directory");
}
#[test]
fn extraction_counts_materialized_links_toward_the_byte_limit() {
    let directory = root("link-limit");
    create_dir_all(&directory).expect("create fixture directory");
    let archive = directory.join("prepared.tar");
    let mut builder = Builder::new(File::create(&archive).expect("create fixture archive"));
    append(&mut builder, "module", b"1234");
    append_link(&mut builder, "module-copy", "module");
    builder.finish().expect("finish fixture archive");
    assert!(extract(&archive, &directory.join("output"), 4, 10).is_err());
    remove_dir_all(directory).expect("remove fixture directory");
}
#[test]
fn extraction_materializes_safe_links() {
    let directory = root("link");
    create_dir_all(&directory).expect("create fixture directory");
    let archive = directory.join("prepared.tar");
    let mut builder = Builder::new(File::create(&archive).expect("create fixture archive"));
    append(&mut builder, "modules/0/python3", b"python");
    append_link(&mut builder, "modules/0/python", "python3");
    builder.finish().expect("finish fixture archive");
    let destination = directory.join("output");
    extract(&archive, &destination, 1024, 10).expect("extract safe archive");
    assert_eq!(read(destination.join("modules/0/python")).expect("read materialized link"), b"python");
    remove_dir_all(directory).expect("remove fixture directory");
}
#[test]
fn extraction_rejects_duplicate_paths_and_unsafe_links() {
    let directory = root("invalid");
    create_dir_all(&directory).expect("create fixture directory");
    let duplicate = directory.join("duplicate.tar");
    let mut builder = Builder::new(File::create(&duplicate).expect("create duplicate archive"));
    append(&mut builder, "module", b"one");
    append(&mut builder, "./module", b"two");
    builder.finish().expect("finish duplicate archive");
    assert!(extract(&duplicate, &directory.join("duplicate"), 1024, 10).is_err());
    let escaping = directory.join("escaping.tar");
    let mut builder = Builder::new(File::create(&escaping).expect("create escaping archive"));
    append_link(&mut builder, "module", "../outside");
    builder.finish().expect("finish escaping archive");
    assert!(extract(&escaping, &directory.join("escaping"), 1024, 10).is_err());
    remove_dir_all(directory).expect("remove fixture directory");
}
#[test]
fn extraction_rejects_oversized_archives() {
    let directory = root("oversized");
    create_dir_all(&directory).expect("create fixture directory");
    let archive = directory.join("prepared.tar");
    let mut builder = Builder::new(File::create(&archive).expect("create fixture archive"));
    append(&mut builder, "module", b"too large");
    builder.finish().expect("finish fixture archive");
    assert!(extract(&archive, &directory.join("output"), 4, 10).is_err());
    write(directory.join("sentinel"), "ok").expect("write fixture sentinel");
    remove_dir_all(directory).expect("remove fixture directory");
}
