use super::*;
use crate::selection::{InputSelection, SelectionEngine};
use std::fs::{create_dir_all, remove_dir_all, write};
use std::time::{SystemTime, UNIX_EPOCH};
#[cfg(target_os = "linux")]
use std::{ffi::OsString, os::unix::ffi::OsStringExt};
#[cfg(unix)]
use std::{
    fs::set_permissions,
    os::unix::fs::{symlink, PermissionsExt},
    os::unix::net::UnixListener,
};

fn assert_limit(error: SelectionError, resource: &'static str) {
    assert!(matches!(error, SelectionError::Limit { resource: actual, .. } if actual == resource));
}

fn compile_with_limits(
    script: &str,
    inventory: &Inventory,
    variables: &Map<String, Value>,
    limits: SelectionLimits,
) -> Result<Program, SelectionError> {
    Cel.compile(script, inventory, variables, limits)
}

fn resolve_with_limits(
    input: &InputSelection,
    variables: Option<&Map<String, Value>>,
    configuration_path: &Path,
    limits: SelectionLimits,
) -> Result<Vec<Option<String>>, SelectionError> {
    input
        .select
        .validate(limits)
        .and_then(|()| Inventory::discover(&input.select.options, configuration_path, limits))
        .and_then(|inventory| {
            let variables = variables.cloned().unwrap_or_default();
            compile_with_limits(&input.select.script, &inventory, &variables, limits)
                .and_then(|program| Cel.evaluate(&program, &inventory, &variables, limits))
        })
}

fn root(label: &str) -> PathBuf {
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH).expect("system time after epoch").as_nanos();
    std::env::temp_dir().join(format!("xylem-selection-{label}-{}-{nonce}", std::process::id()))
}

fn selection(root: &str, recursive: bool, script: &str) -> InputSelection {
    selection_roots(&[root], recursive, script)
}

fn selection_roots(roots: &[&str], recursive: bool, script: &str) -> InputSelection {
    InputSelection {
        select: SelectionRequest {
            engine: SelectionEngine::Cel,
            options: SelectionOptions {
                recursive,
                roots: roots.iter().map(|root| (*root).to_string()).collect(),
            },
            script: script.to_string(),
        },
    }
}
#[test]
fn cel_conforms_for_every_supported_operator() {
    let fixture = root("operators");
    let inputs = fixture.join("inputs");
    create_dir_all(&inputs).expect("create input tree");
    write(inputs.join("a.txt"), "a").expect("write candidate");
    let cases = [
        ("addition", "1 + 2 == 3"),
        ("conditional", "(true ? 1 : 2) == 1"),
        ("division", "6 / 2 == 3"),
        ("equality", "1 == 1"),
        ("greater", "2 > 1"),
        ("greater-or-equal", "2 >= 2"),
        ("in", "2 in [1, 2]"),
        ("index", "[1, 2][1] == 2"),
        ("less", "1 < 2"),
        ("less-or-equal", "2 <= 2"),
        ("list-addition", "[1] + [2] == [1, 2]"),
        ("logical-and", "true && true"),
        ("logical-not", "!false"),
        ("logical-or", "false || true"),
        ("modulo", "7 % 4 == 3"),
        ("multiplication", "2 * 3 == 6"),
        ("negation", "-2 == 0 - 2"),
        ("not-equal", "1 != 2"),
        ("string-addition", "'a' + 'b' == 'ab'"),
        ("subtraction", "3 - 2 == 1"),
    ];
    cases.into_iter().for_each(|(name, condition)| {
        let request = selection("inputs", false, &format!("({condition}) ? [items[0].path] : []"));
        let resolved = InputSelection::resolve(&request, None, &fixture.join("config.json"))
            .unwrap_or_else(|error| panic!("{name} conformance failed: {error}"));
        assert_eq!(resolved.values.len(), 1, "{name}");
        assert!(resolved.values[0].is_some(), "{name}");
    });
    remove_dir_all(fixture).expect("remove fixture");
}
#[test]
fn cel_conforms_for_supported_functions_macros_and_bindings() {
    let fixture = root("functions-bindings");
    let inputs = fixture.join("inputs");
    create_dir_all(&inputs).expect("create input tree");
    write(inputs.join("a.tif"), "a").expect("write candidate");
    let variables = serde_json::json!({
        "boolean": true,
        "double": 1.25,
        "list": [1, 2],
        "map": {"key": "value"},
        "null_value": null,
        "signed": -9223372036854775807i64,
        "string": "42",
        "unsigned": 18446744073709551615u64
    })
    .as_object()
    .expect("variables object")
    .clone();
    let condition = "variables.null_value == null && variables.boolean && variables.signed == -9223372036854775807 && variables.unsigned == 18446744073709551615u && variables.double == 1.25 && variables.string == '42' && variables.list[1] == 2 && variables.map.key == 'value' && items.all(item, item.depth == 1) && items.exists(item, item.extension == '.tif') && items.exists_one(item, item.name == 'a.tif') && items.filter(item, item.kind == 'file').size() == 1 && items.map(item, item.path)[0] == items[0].path && items.map(item, item.kind == 'file', item.path)[0] == items[0].path && has(items[0].path) && items[0].name.startsWith('a') && items[0].name.endsWith('.tif') && items[0].path.contains('inputs') && size(items) == 1 && int(variables.string) == 42 && uint(42) == 42u && double(42) == 42.0 && string(42) == '42' && options.roots.size() == 1 && !options.recursive && items[0].index == 0 && items[0].relative_path == 'a.tif' && items[0].size == 1";
    let request = selection("inputs", false, &format!("({condition}) ? [items[0].path] : []"));
    let resolved = InputSelection::resolve(&request, Some(&variables), &fixture.join("config.json")).expect("resolve conformance expression");
    let inputs = canonicalize(inputs).expect("canonical input root");
    assert_eq!(resolved.values, [Some(inputs.join("a.tif").display().to_string())]);
    remove_dir_all(fixture).expect("remove fixture");
}
#[test]
fn cel_filters_recursive_candidates_in_stable_order() {
    let fixture = root("filter");
    let inputs = fixture.join("inputs");
    create_dir_all(inputs.join("nested")).expect("create input tree");
    write(inputs.join("z.tif"), "z").expect("write z");
    write(inputs.join("a.txt"), "a").expect("write a");
    write(inputs.join("nested/b.tif"), "bb").expect("write b");
    let request = selection(
        "inputs",
        true,
        "items.filter(item, item.kind == 'file' && item.extension == '.tif').map(item, item.path)",
    );
    let resolved = InputSelection::resolve(&request, None, &fixture.join("config.json")).expect("resolve selection");
    let inputs = canonicalize(inputs).expect("canonical input root");
    assert_eq!(
        resolved.values,
        [
            Some(inputs.join("nested/b.tif").display().to_string()),
            Some(inputs.join("z.tif").display().to_string())
        ]
    );
    assert!(matches!(resolved.source, InputSource::Selection { candidate_count: 4, .. }));
    remove_dir_all(fixture).expect("remove fixture");
}
#[test]
fn cel_rejects_parse_declaration_and_missing_binding_errors() {
    let fixture = root("diagnostics");
    let inputs = fixture.join("inputs");
    create_dir_all(&inputs).expect("create input tree");
    write(inputs.join("a.txt"), "a").expect("write candidate");
    let parse =
        InputSelection::resolve(&selection("inputs", false, "["), None, &fixture.join("config.json")).expect_err("reject malformed expression");
    assert!(matches!(parse, SelectionError::Parse(_)));
    let declaration = InputSelection::resolve(
        &selection("inputs", false, "undeclared ? [items[0].path] : []"),
        None,
        &fixture.join("config.json"),
    )
    .expect_err("reject undeclared identifier");
    assert!(matches!(declaration, SelectionError::Check(_)));
    let binding = InputSelection::resolve(
        &selection("inputs", false, "variables.missing ? [items[0].path] : []"),
        None,
        &fixture.join("config.json"),
    )
    .expect_err("reject missing configured variable");
    assert!(matches!(binding, SelectionError::Evaluation(_)));
    remove_dir_all(fixture).expect("remove fixture");
}
#[test]
fn cel_rejects_undeclared_capabilities() {
    let fixture = root("capability");
    let inputs = fixture.join("inputs");
    create_dir_all(&inputs).expect("create input tree");
    [
        "duration('1s')",
        "env('HOME')",
        "now()",
        "optional.of(1)",
        "random()",
        "readFile('/etc/passwd')",
        "'value'.matches('.*')",
    ]
    .into_iter()
    .for_each(|script| {
        let error = InputSelection::resolve(&selection("inputs", false, script), None, &fixture.join("config.json"))
            .expect_err("reject unsupported function");
        assert!(matches!(&error, SelectionError::Check(_)), "{script}: {error}");
    });
    remove_dir_all(fixture).expect("remove fixture");
}
#[test]
fn cel_rejects_unsupported_bytes_and_result_shapes() {
    let fixture = root("result-shapes");
    let inputs = fixture.join("inputs");
    create_dir_all(&inputs).expect("create input tree");
    write(inputs.join("a.txt"), "a").expect("write candidate");
    let bytes =
        InputSelection::resolve(&selection("inputs", false, "[b'bytes']"), None, &fixture.join("config.json")).expect_err("reject bytes literal");
    assert!(matches!(bytes, SelectionError::Check(_)));
    [
        ("empty", "['']"),
        ("map", "[{}]"),
        ("mixed", "[items[0].path, 1]"),
        ("nul", "['\\u0000']"),
        ("null", "[null]"),
        ("scalar", "items[0].path"),
    ]
    .into_iter()
    .for_each(
        |(name, script)| match InputSelection::resolve(&selection("inputs", false, script), None, &fixture.join("config.json")) {
            | Err(SelectionError::Result(_)) => {}
            | Err(error) => panic!("{name} returned the wrong error: {error}"),
            | Ok(_) => panic!("{name} result unexpectedly succeeded"),
        },
    );
    remove_dir_all(fixture).expect("remove fixture");
}
#[test]
fn cel_supports_the_documented_pure_subset() {
    let fixture = root("subset");
    let inputs = fixture.join("inputs");
    create_dir_all(&inputs).expect("create input tree");
    write(inputs.join("a.tif"), "a").expect("write a");
    write(inputs.join("b.txt"), "bb").expect("write b");
    let request = selection(
        "inputs",
        false,
        "items.size() == 2 && items.all(item, item.depth == 1) && items.exists(item, item.extension == '.tif') && items.exists_one(item, item.name == 'a.tif') && has(items[0].path) && items[0].name.startsWith('a') && items[0].name.endsWith('.tif') && items[0].path.contains('inputs') && string(int(items[0].size)) == '1' && uint(1) == 1u && double(1) == 1.0 ? [items[0].path] : []",
    );
    let resolved = InputSelection::resolve(&request, None, &fixture.join("config.json")).expect("resolve subset");
    let inputs = canonicalize(inputs).expect("canonical input root");
    assert_eq!(resolved.values, [Some(inputs.join("a.tif").display().to_string())]);
    remove_dir_all(fixture).expect("remove fixture");
}
#[test]
fn cel_uses_nested_variables() {
    let fixture = root("variables");
    let inputs = fixture.join("inputs");
    create_dir_all(&inputs).expect("create input tree");
    write(inputs.join("a.tif"), "a").expect("write a");
    write(inputs.join("b.txt"), "b").expect("write b");
    let request = selection(
        "inputs",
        true,
        "items.filter(item, variables.extensions.exists(ext, ext == item.extension) && options.recursive).map(item, item.path).map(path, path)",
    );
    let variables = serde_json::json!({"extensions": [".tif"]}).as_object().expect("variables object").clone();
    let resolved = InputSelection::resolve(&request, Some(&variables), &fixture.join("config.yaml")).expect("resolve variables");
    let inputs = canonicalize(inputs).expect("canonical input root");
    assert_eq!(resolved.values, [Some(inputs.join("a.tif").display().to_string())]);
    remove_dir_all(fixture).expect("remove fixture");
}
#[test]
fn directory_enumeration_stops_at_the_visited_entry_limit() {
    let fixture = root("visited-limit");
    create_dir_all(&fixture).expect("create input tree");
    write(fixture.join("a.txt"), "a").expect("write a");
    write(fixture.join("b.txt"), "b").expect("write b");
    let error = read_entries(&fixture, 0, 1).expect_err("enforce visited entry limit while enumerating");
    assert!(matches!(
        error,
        SelectionError::Limit {
            resource: "visited_entries",
            actual: 2,
            limit: 1
        }
    ));
    remove_dir_all(fixture).expect("remove fixture");
}
#[test]
fn discovery_enforces_every_limit_resource() {
    let fixture = root("discovery-limits");
    let inputs = fixture.join("inputs");
    create_dir_all(inputs.join("nested")).expect("create input tree");
    write(inputs.join("a.txt"), "a").expect("write first candidate");
    write(inputs.join("b.txt"), "b").expect("write second candidate");
    write(inputs.join("nested/c.txt"), "c").expect("write nested candidate");
    let configuration = fixture.join("config.json");
    let cases = [
        (
            "candidates",
            SelectionLimits {
                candidates: 1,
                ..SelectionLimits::default()
            },
            selection("inputs", false, "[]"),
        ),
        (
            "metadata_operations",
            SelectionLimits {
                metadata_operations: 0,
                ..SelectionLimits::default()
            },
            selection("inputs", false, "[]"),
        ),
        (
            "path_bytes",
            SelectionLimits {
                path_bytes: 0,
                ..SelectionLimits::default()
            },
            selection("inputs", false, "[]"),
        ),
        (
            "roots",
            SelectionLimits {
                roots: 1,
                ..SelectionLimits::default()
            },
            selection_roots(&["inputs", "inputs"], false, "[]"),
        ),
        (
            "traversal_depth",
            SelectionLimits {
                traversal_depth: 1,
                ..SelectionLimits::default()
            },
            selection("inputs", true, "[]"),
        ),
    ];
    cases.into_iter().for_each(
        |(resource, limits, request)| match resolve_with_limits(&request, None, &configuration, limits) {
            | Err(error) => assert_limit(error, resource),
            | Ok(_) => panic!("{resource} limit unexpectedly succeeded"),
        },
    );
    remove_dir_all(fixture).expect("remove fixture");
}
#[test]
fn discovery_preserves_root_order_deduplicates_and_exposes_metadata() {
    let fixture = root("root-order");
    let first = fixture.join("first");
    let second = fixture.join("second");
    create_dir_all(&first).expect("create first root");
    create_dir_all(&second).expect("create second root");
    write(first.join("z.bin"), "123").expect("write first candidate");
    write(second.join("a.tif"), "x").expect("write second candidate");
    let second_string = second.display().to_string();
    let options = SelectionOptions {
        recursive: false,
        roots: vec!["first".to_string(), second_string.clone(), second_string],
    };
    let inventory = Inventory::discover(&options, &fixture.join("config.json"), SelectionLimits::default()).expect("discover ordered roots");
    assert_eq!(inventory.items.len(), 2);
    assert_eq!(inventory.items[0].index, 0);
    assert_eq!(inventory.items[0].relative_path, "z.bin");
    assert_eq!(inventory.items[0].extension, ".bin");
    assert_eq!(inventory.items[0].kind, "file");
    assert_eq!(inventory.items[0].size, 3);
    assert_eq!(inventory.items[0].depth, 1);
    assert_eq!(inventory.items[1].index, 1);
    assert_eq!(inventory.items[1].relative_path, "a.tif");
    assert_eq!(inventory.options.roots.len(), 3);
    remove_dir_all(fixture).expect("remove fixture");
}
#[test]
fn discovery_rejects_missing_non_directory_and_remote_roots() {
    let fixture = root("invalid-roots");
    create_dir_all(&fixture).expect("create fixture");
    write(fixture.join("file.txt"), "file").expect("write file root");
    let configuration = fixture.join("config.json");
    let empty = InputSelection::resolve(&selection_roots(&[], false, "[]"), None, &configuration).expect_err("reject empty roots");
    assert!(matches!(empty, SelectionError::Request(_)));
    let blank = InputSelection::resolve(&selection(" ", false, "[]"), None, &configuration).expect_err("reject blank root");
    assert!(matches!(blank, SelectionError::Request(_)));
    let missing = InputSelection::resolve(&selection("missing", false, "[]"), None, &configuration).expect_err("reject missing root");
    assert!(matches!(missing, SelectionError::Discovery { .. }));
    let file = InputSelection::resolve(&selection("file.txt", false, "[]"), None, &configuration).expect_err("reject file root");
    assert!(matches!(file, SelectionError::Request(_)));
    let remote = InputSelection::resolve(&selection("https://example.test/data", false, "[]"), None, &configuration).expect_err("reject remote root");
    assert!(matches!(remote, SelectionError::Request(_)));
    remove_dir_all(fixture).expect("remove fixture");
}
#[cfg(target_os = "linux")]
#[test]
fn discovery_rejects_non_utf8_candidates() {
    let fixture = root("non-utf8");
    let inputs = fixture.join("inputs");
    create_dir_all(&inputs).expect("create input tree");
    write(inputs.join(OsString::from_vec(vec![b'b', 0xff])), "bad").expect("write non-UTF-8 candidate");
    let error =
        InputSelection::resolve(&selection("inputs", false, "[]"), None, &fixture.join("config.json")).expect_err("reject non-UTF-8 candidate");
    assert!(matches!(error, SelectionError::Request(_)));
    remove_dir_all(fixture).expect("remove fixture");
}
#[cfg(unix)]
#[test]
fn discovery_rejects_symlink_roots() {
    let fixture = root("boundary-roots");
    let target = fixture.join("target");
    create_dir_all(&target).expect("create symlink target");
    symlink(&target, fixture.join("linked-root")).expect("create root symlink");
    let linked =
        InputSelection::resolve(&selection("linked-root", false, "[]"), None, &fixture.join("config.json")).expect_err("reject symlink root");
    assert!(matches!(linked, SelectionError::Request(_)));
    remove_dir_all(fixture).expect("remove fixture");
}
#[cfg(unix)]
#[test]
fn discovery_reports_unreadable_directories_and_skips_special_files() {
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH).expect("system time after epoch").as_nanos();
    let fixture = PathBuf::from("/tmp").join(format!("xs-fs-{}-{nonce}", std::process::id()));
    let unreadable = fixture.join("unreadable");
    create_dir_all(&unreadable).expect("create unreadable root");
    write(unreadable.join("hidden.txt"), "hidden").expect("write hidden file");
    set_permissions(&unreadable, std::fs::Permissions::from_mode(0o000)).expect("remove root permissions");
    let permissions_enforced = read_dir(&unreadable).is_err();
    let unreadable_result = InputSelection::resolve(&selection("unreadable", false, "[]"), None, &fixture.join("config.json"));
    set_permissions(&unreadable, std::fs::Permissions::from_mode(0o700)).expect("restore root permissions");
    if permissions_enforced {
        assert!(matches!(unreadable_result, Err(SelectionError::Discovery { .. })));
    }
    let inputs = fixture.join("inputs");
    create_dir_all(&inputs).expect("create input root");
    let socket = UnixListener::bind(inputs.join("socket")).expect("create socket");
    let resolved = InputSelection::resolve(
        &selection("inputs", false, "items.map(item, item.path)"),
        None,
        &fixture.join("config.json"),
    )
    .expect("skip special entry");
    assert_eq!(resolved.values, [None]);
    assert!(matches!(
        resolved.source,
        InputSource::Selection {
            skipped_unsupported_entries: 1,
            ..
        }
    ));
    drop(socket);
    remove_dir_all(fixture).expect("remove fixture");
}
#[cfg(unix)]
#[test]
fn discovery_skips_symlinks_without_following_them() {
    let fixture = root("symlink");
    let inputs = fixture.join("inputs");
    create_dir_all(&inputs).expect("create input tree");
    write(inputs.join("accepted.txt"), "accepted").expect("write accepted");
    write(fixture.join("outside.txt"), "outside").expect("write outside");
    symlink(fixture.join("outside.txt"), inputs.join("link.txt")).expect("create symlink");
    let request = selection("inputs", true, "items.map(item, item.path)");
    let resolved = InputSelection::resolve(&request, None, &fixture.join("config.json")).expect("resolve without following symlink");
    let inputs = canonicalize(inputs).expect("canonical input root");
    assert_eq!(resolved.values, [Some(inputs.join("accepted.txt").display().to_string())]);
    assert!(matches!(
        resolved.source,
        InputSource::Selection {
            candidate_count: 1,
            skipped_unsupported_entries: 1,
            ..
        }
    ));
    remove_dir_all(fixture).expect("remove fixture");
}
#[test]
fn empty_result_preserves_zero_input_execution() {
    let fixture = root("empty");
    let inputs = fixture.join("inputs");
    create_dir_all(&inputs).expect("create input tree");
    let request = selection("inputs", false, "[]");
    let resolved = InputSelection::resolve(&request, None, &fixture.join("config.json")).expect("resolve empty selection");
    assert_eq!(resolved.values, [None]);
    remove_dir_all(fixture).expect("remove fixture");
}
#[test]
fn evaluation_cost_is_deterministic_and_enforced_at_the_boundary() {
    let fixture = root("cost-boundary");
    let inputs = fixture.join("inputs");
    create_dir_all(&inputs).expect("create input tree");
    write(inputs.join("a.txt"), "a").expect("write a");
    write(inputs.join("b.txt"), "b").expect("write b");
    let request = selection("inputs", false, "items.filter(item, item.size > 0).map(item, item.path)");
    let inventory =
        Inventory::discover(&request.select.options, &fixture.join("config.json"), SelectionLimits::default()).expect("discover inventory");
    let variables = Map::new();
    let program = Program::compile(&request.select.script).expect("parse cost expression");
    let scope = inventory.root_estimates(&variables);
    let first = program
        .expression()
        .estimate(&scope, &inventory, &variables, SelectionLimits::default())
        .expect("estimate first cost");
    let second = program
        .expression()
        .estimate(&scope, &inventory, &variables, SelectionLimits::default())
        .expect("estimate second cost");
    assert_eq!(first.cost, second.cost);
    let exact = SelectionLimits {
        evaluation_cost: first.cost,
        ..SelectionLimits::default()
    };
    assert!(compile_with_limits(&request.select.script, &inventory, &variables, exact).is_ok());
    let below = SelectionLimits {
        evaluation_cost: first.cost.saturating_sub(1),
        ..SelectionLimits::default()
    };
    let error = compile_with_limits(&request.select.script, &inventory, &variables, below).expect_err("reject expression above cost boundary");
    assert_limit(error, "evaluation_cost");
    remove_dir_all(fixture).expect("remove fixture");
}
#[test]
fn evaluation_enforces_ast_cost_and_intermediate_limits() {
    let fixture = root("evaluation-limits");
    let inputs = fixture.join("inputs");
    create_dir_all(&inputs).expect("create input tree");
    write(inputs.join("a.txt"), "a").expect("write candidate");
    let request = selection("inputs", false, "[items[0].path]");
    let configuration = fixture.join("config.json");
    let cases = [
        (
            "ast_depth",
            SelectionLimits {
                ast_depth: 1,
                ..SelectionLimits::default()
            },
        ),
        (
            "ast_nodes",
            SelectionLimits {
                ast_nodes: 1,
                ..SelectionLimits::default()
            },
        ),
        (
            "evaluation_cost",
            SelectionLimits {
                evaluation_cost: 1,
                ..SelectionLimits::default()
            },
        ),
        (
            "intermediate_bytes",
            SelectionLimits {
                intermediate_bytes: 1,
                ..SelectionLimits::default()
            },
        ),
        (
            "intermediate_records",
            SelectionLimits {
                intermediate_records: 0,
                ..SelectionLimits::default()
            },
        ),
    ];
    cases
        .into_iter()
        .for_each(|(resource, limits)| match resolve_with_limits(&request, None, &configuration, limits) {
            | Err(error) => assert_limit(error, resource),
            | Ok(_) => panic!("{resource} limit unexpectedly succeeded"),
        });
    remove_dir_all(fixture).expect("remove fixture");
}
#[test]
fn evaluation_enforces_result_and_variable_limits() {
    let item = SelectionItem {
        depth: 1,
        extension: ".txt".to_string(),
        index: 0,
        kind: "file".to_string(),
        name: "a.txt".to_string(),
        path: "/inputs/a.txt".to_string(),
        relative_path: "a.txt".to_string(),
        size: 1,
    };
    let result = cel::to_value(vec![item.path.clone()]).expect("serialize result");
    let records = normalize_result(
        result.clone(),
        core::slice::from_ref(&item),
        SelectionLimits {
            intermediate_records: 0,
            ..SelectionLimits::default()
        },
    )
    .expect_err("enforce result record limit");
    assert_limit(records, "result_records");
    let bytes = normalize_result(
        result,
        core::slice::from_ref(&item),
        SelectionLimits {
            intermediate_bytes: 1,
            ..SelectionLimits::default()
        },
    )
    .expect_err("enforce result byte limit");
    assert_limit(bytes, "result_bytes");
    let variables = serde_json::json!({"key": {"nested": "value"}})
        .as_object()
        .expect("variables object")
        .clone();
    [
        (
            "variable_depth",
            SelectionLimits {
                variable_depth: 0,
                ..SelectionLimits::default()
            },
        ),
        (
            "variable_nodes",
            SelectionLimits {
                variable_nodes: 1,
                ..SelectionLimits::default()
            },
        ),
        (
            "variable_scalar_bytes",
            SelectionLimits {
                variable_scalar_bytes: 0,
                ..SelectionLimits::default()
            },
        ),
    ]
    .into_iter()
    .for_each(|(resource, limits)| {
        let error = validate_variable_bindings(&variables, limits).expect_err("enforce variable limit");
        assert_limit(error, resource);
    });
}
#[test]
fn identical_snapshots_produce_byte_identical_selection_provenance() {
    let fixture = root("determinism");
    let inputs = fixture.join("inputs");
    create_dir_all(&inputs).expect("create input tree");
    write(inputs.join("a.txt"), "a").expect("write candidate");
    let request = selection("inputs", false, "items.map(item, item.path)");
    let first = InputSelection::resolve(&request, None, &fixture.join("config.json")).expect("resolve first snapshot");
    let second = InputSelection::resolve(&request, None, &fixture.join("config.json")).expect("resolve second snapshot");
    assert_eq!(
        serde_json::to_vec(&first).expect("serialize first snapshot"),
        serde_json::to_vec(&second).expect("serialize second snapshot")
    );
    remove_dir_all(fixture).expect("remove fixture");
}
#[test]
fn inventory_digest_tracks_metadata_changes() {
    let fixture = root("digest");
    let inputs = fixture.join("inputs");
    create_dir_all(&inputs).expect("create input tree");
    let candidate = inputs.join("a.tif");
    write(&candidate, "a").expect("write candidate");
    let request = selection("inputs", false, "items.map(item, item.path)");
    let first = InputSelection::resolve(&request, None, &fixture.join("config.json")).expect("resolve first inventory");
    write(&candidate, "changed").expect("change candidate metadata");
    let second = InputSelection::resolve(&request, None, &fixture.join("config.json")).expect("resolve second inventory");
    let digest = |resolved: ResolvedInputs| match resolved.source {
        | InputSource::Selection { inventory_digest, .. } => inventory_digest,
        | _ => panic!("selection provenance"),
    };
    assert_ne!(digest(first), digest(second));
    remove_dir_all(fixture).expect("remove fixture");
}
#[test]
fn later_result_duplicates_are_removed() {
    let fixture = root("duplicates");
    let inputs = fixture.join("inputs");
    create_dir_all(&inputs).expect("create input tree");
    write(inputs.join("a.tif"), "a").expect("write candidate");
    let request = selection("inputs", false, "[items[0].path, items[0].path]");
    let resolved = InputSelection::resolve(&request, None, &fixture.join("config.json")).expect("resolve duplicates");
    let inputs = canonicalize(inputs).expect("canonical input root");
    assert_eq!(resolved.values, [Some(inputs.join("a.tif").display().to_string())]);
    remove_dir_all(fixture).expect("remove fixture");
}
#[test]
fn normalization_properties_hold_for_generated_sequences() {
    let items = ["/inputs/a", "/inputs/b", "/inputs/c"]
        .into_iter()
        .enumerate()
        .map(|(index, path)| SelectionItem {
            depth: 1,
            extension: String::new(),
            index,
            kind: "file".to_string(),
            name: path.rsplit('/').next().expect("candidate name").to_string(),
            path: path.to_string(),
            relative_path: path.rsplit('/').next().expect("relative path").to_string(),
            size: 1,
        })
        .collect::<Vec<_>>();
    (0u64..64).for_each(|seed| {
        let sequence = (0..12)
            .scan(seed, |state, _| {
                *state = state.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
                Some(items[(*state as usize) % items.len()].path.clone())
            })
            .collect::<Vec<_>>();
        let expected = sequence.iter().fold(Vec::new(), |mut unique, path| {
            if !unique.contains(path) {
                unique.push(path.clone());
            }
            unique
        });
        let normalized = normalize_result(
            cel::to_value(&sequence).expect("serialize generated result"),
            &items,
            SelectionLimits::default(),
        )
        .expect("normalize generated result");
        assert_eq!(normalized, expected.iter().cloned().map(Some).collect::<Vec<_>>());
        let normalized_strings = normalized.iter().flatten().cloned().collect::<Vec<_>>();
        let repeated = normalize_result(
            cel::to_value(normalized_strings).expect("serialize normalized result"),
            &items,
            SelectionLimits::default(),
        )
        .expect("renormalize generated result");
        assert_eq!(repeated, normalized);
    });
}
#[test]
fn recursive_candidates_are_sorted_by_relative_path() {
    let fixture = root("relative-order");
    let inputs = fixture.join("inputs");
    create_dir_all(inputs.join("a")).expect("create input tree");
    write(inputs.join("a/child.txt"), "child").expect("write child");
    write(inputs.join("a.txt"), "sibling").expect("write sibling");
    let request = selection("inputs", true, "items.map(item, item.path)");
    let resolved = InputSelection::resolve(&request, None, &fixture.join("config.json")).expect("resolve selection");
    let inputs = canonicalize(inputs).expect("canonical input root");
    assert_eq!(
        resolved.values,
        [
            Some(inputs.join("a").display().to_string()),
            Some(inputs.join("a.txt").display().to_string()),
            Some(inputs.join("a/child.txt").display().to_string())
        ]
    );
    remove_dir_all(fixture).expect("remove fixture");
}
#[test]
fn recursive_traversal_never_emits_a_path_outside_its_root() {
    let fixture = root("containment-property");
    let inputs = fixture.join("inputs");
    (0..8).for_each(|index| {
        let directory = inputs.join(format!("level-{index}")).join("nested");
        create_dir_all(&directory).expect("create generated directory");
        write(directory.join(format!("candidate-{index}.txt")), index.to_string()).expect("write generated candidate");
    });
    write(fixture.join("outside.txt"), "outside").expect("write outside candidate");
    let inventory = Inventory::discover(
        &SelectionOptions {
            recursive: true,
            roots: vec![inputs.display().to_string()],
        },
        &fixture.join("config.json"),
        SelectionLimits::default(),
    )
    .expect("discover generated tree");
    let canonical_root = canonicalize(inputs).expect("canonical input root");
    assert!(!inventory.items.is_empty());
    assert!(inventory.items.iter().all(|item| Path::new(&item.path).starts_with(&canonical_root)));
    remove_dir_all(fixture).expect("remove fixture");
}
#[test]
fn result_must_reference_inventory_candidates() {
    let fixture = root("unknown");
    let inputs = fixture.join("inputs");
    create_dir_all(&inputs).expect("create input tree");
    let request = selection("inputs", false, "['/not/discovered']");
    let error = InputSelection::resolve(&request, None, &fixture.join("config.json")).expect_err("reject unknown result");
    assert!(matches!(error, SelectionError::UnknownCandidate(_)));
    remove_dir_all(fixture).expect("remove fixture");
}
#[test]
fn script_bytes_are_bounded_before_discovery() {
    let fixture = root("script-limit");
    let request = selection("missing", false, &format!("'{}'", "x".repeat(SelectionLimits::default().script_bytes)));
    let error = InputSelection::resolve(&request, None, &fixture.join("config.json")).expect_err("reject oversized script");
    assert!(matches!(
        error,
        SelectionError::Limit {
            resource: "script_bytes",
            ..
        }
    ));
}
#[test]
fn script_result_order_is_preserved() {
    let fixture = root("script-order");
    let inputs = fixture.join("inputs");
    create_dir_all(&inputs).expect("create input tree");
    write(inputs.join("a.txt"), "a").expect("write a");
    write(inputs.join("b.txt"), "b").expect("write b");
    let request = selection("inputs", false, "[items[1].path, items[0].path]");
    let resolved = InputSelection::resolve(&request, None, &fixture.join("config.json")).expect("resolve ordered result");
    let inputs = canonicalize(inputs).expect("canonical input root");
    assert_eq!(
        resolved.values,
        [
            Some(inputs.join("b.txt").display().to_string()),
            Some(inputs.join("a.txt").display().to_string())
        ]
    );
    remove_dir_all(fixture).expect("remove fixture");
}
