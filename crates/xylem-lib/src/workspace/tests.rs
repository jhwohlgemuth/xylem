#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used,
    clippy::arithmetic_side_effects
)]
use super::*;
use core::panic::AssertUnwindSafe;
use core::sync::atomic::{AtomicU64, Ordering};
use std::env;
use std::fs::{create_dir_all, read, write};
#[cfg(unix)]
use std::os::unix::fs::{symlink, PermissionsExt};
#[cfg(unix)]
use std::os::unix::net::UnixListener;
use std::panic::catch_unwind;
use std::process;

static SEQUENCE: AtomicU64 = AtomicU64::new(0);

fn temporary(name: &str) -> PathBuf {
    env::temp_dir().join(format!(
        "xylem-workspace-{name}-{}-{}",
        process::id(),
        SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ))
}

#[test]
fn cleanup_occurs_during_panic_unwind() {
    let parent = temporary("panic");
    let root = parent.join("run");
    let result = catch_unwind(AssertUnwindSafe(|| {
        let _workspace = RunWorkspace::create(&parent, "run").expect("workspace");
        panic!("exercise workspace drop");
    }));
    assert!(result.is_err());
    assert!(!root.exists());
    let _ = remove_dir_all(parent);
}
#[cfg(unix)]
#[test]
fn materialization_cycles_are_rejected() {
    let root = temporary("cycle");
    let source = root.join("source");
    create_dir_all(&source).expect("source");
    symlink(".", source.join("cycle")).expect("symlink");
    let relative = SafePath::new("destination").expect("safe destination");
    let destination = root.join(relative.as_path());
    assert!(matches!(
        materialize(&source, &root, &relative),
        Err(WorkspaceError::Materialization { .. })
    ));
    assert!(!destination.exists());
    remove_dir_all(root).expect("cleanup");
}
#[cfg(unix)]
#[test]
fn materialization_dangling_links_are_rejected() {
    let root = temporary("dangling");
    let source = root.join("source");
    create_dir_all(&source).expect("source");
    symlink("missing", source.join("dangling")).expect("symlink");
    let relative = SafePath::new("destination").expect("safe destination");
    let destination = root.join(relative.as_path());
    assert!(materialize(&source, &root, &relative).is_err());
    assert!(!destination.exists());
    remove_dir_all(root).expect("cleanup");
}
#[cfg(unix)]
#[test]
fn materialization_preserves_permissions_and_replaces_internal_links() {
    let root = temporary("materialize");
    let source = root.join("source");
    let relative = SafePath::new("destination").expect("safe destination");
    let destination = root.join(relative.as_path());
    create_dir_all(source.join("bin")).expect("source");
    let executable = source.join("bin/tool");
    write(&executable, b"tool").expect("write executable");
    set_permissions(&executable, std::fs::Permissions::from_mode(0o755)).expect("permissions");
    symlink("bin/tool", source.join("tool-link")).expect("symlink");
    materialize(&source, &root, &relative).expect("materialize");
    assert_eq!(read(destination.join("tool-link")).expect("materialized link"), b"tool");
    assert!(!symlink_metadata(destination.join("tool-link"))
        .expect("metadata")
        .file_type()
        .is_symlink());
    assert_eq!(
        symlink_metadata(destination.join("bin/tool")).expect("metadata").permissions().mode() & 0o111,
        0o111
    );
    remove_dir_all(root).expect("cleanup");
}
#[cfg(unix)]
#[test]
fn materialization_rejects_escaping_links() {
    let root = temporary("escape");
    let source = root.join("source");
    create_dir_all(&source).expect("source");
    write(root.join("outside"), b"outside").expect("outside");
    symlink("../outside", source.join("escape")).expect("symlink");
    let relative = SafePath::new("destination").expect("safe destination");
    let destination = root.join(relative.as_path());
    assert!(matches!(
        materialize(&source, &root, &relative),
        Err(WorkspaceError::Materialization { .. })
    ));
    assert!(!destination.exists());
    remove_dir_all(root).expect("cleanup");
}
#[cfg(unix)]
#[test]
fn materialization_rejects_special_files() {
    let root = temporary("special");
    let source = root.join("source");
    create_dir_all(&source).expect("source");
    let _socket = UnixListener::bind(source.join("socket")).expect("socket");
    let relative = SafePath::new("destination").expect("safe destination");
    let destination = root.join(relative.as_path());
    assert!(matches!(
        materialize(&source, &root, &relative),
        Err(WorkspaceError::Materialization { .. })
    ));
    assert!(!destination.exists());
    remove_dir_all(root).expect("cleanup");
}
#[test]
fn retained_workspace_survives_owner_drop() {
    let parent = temporary("retain");
    let path = RunWorkspace::create(&parent, "run").expect("workspace").retain();
    assert!(path.is_dir());
    remove_dir_all(parent).expect("cleanup");
}
#[cfg(unix)]
#[test]
fn test_materialization_excludes_pixi_envs_before_resolving_links() {
    let root = temporary("pixi-envs-link");
    let source = root.join("source");
    create_dir_all(source.join(".pixi")).expect("Pixi directory");
    symlink("missing", source.join(".pixi/envs")).expect("broken environment link");
    write(source.join(".pixi/config.toml"), b"[repodata-config]\n").expect("Pixi configuration");
    let relative = SafePath::new("destination").expect("safe destination");
    let destination = materialize(&source, &root, &relative).expect("exclude environment link");
    assert!(symlink_metadata(destination.join(".pixi/envs")).is_err());
    assert_eq!(
        read(destination.join(".pixi/config.toml")).expect("copied configuration"),
        b"[repodata-config]\n"
    );
    assert!(symlink_metadata(source.join(".pixi/envs"))
        .expect("original link")
        .file_type()
        .is_symlink());
    remove_dir_all(root).expect("cleanup");
}
#[cfg(unix)]
#[test]
fn test_materialization_excludes_pixi_envs_in_nested_projects() {
    let root = temporary("nested-pixi-envs");
    let source = root.join("source");
    create_dir_all(source.join("nested/.pixi/envs/default/lib")).expect("installed environment");
    symlink("missing", source.join("nested/.pixi/envs/default/lib/broken")).expect("broken installed link");
    write(source.join("nested/pixi.toml"), b"[workspace]\n").expect("nested manifest");
    let relative = SafePath::new("destination").expect("safe destination");
    let destination = materialize(&source, &root, &relative).expect("exclude nested environment");
    assert!(!destination.join("nested/.pixi/envs").exists());
    assert_eq!(
        read(destination.join("nested/pixi.toml")).expect("copied nested manifest"),
        b"[workspace]\n"
    );
    remove_dir_all(root).expect("cleanup");
}
#[test]
fn workspace_creates_isolated_input_layout() {
    let parent = temporary("layout");
    let workspace = RunWorkspace::create(&parent, "run").expect("workspace");
    let first = workspace.input(0).expect("first input");
    let second = workspace.input(1).expect("second input");
    assert!(workspace.configuration().is_dir());
    assert!(first.modules().is_dir());
    assert!(first.environments().is_dir());
    assert!(first.scratch().is_dir());
    assert_ne!(first.root(), second.root());
    let root = workspace.root().to_path_buf();
    workspace.close().expect("close workspace");
    assert!(!root.exists());
    remove_dir_all(parent).expect("cleanup parent");
}
