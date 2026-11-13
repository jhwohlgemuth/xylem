use crate::completion::{CompletionDecision, CompletionPlan};
use crate::script::Module;
use std::collections::HashMap;
use std::fs::{create_dir_all, remove_dir_all, write};
use std::path::{Path, PathBuf};

fn fixture(root: &Path) -> (Module, HashMap<String, String>) {
    let root = root.display().to_string();
    let module = serde_json::from_value(serde_json::json!({
        "completion": {
            "marker": "{{ OUTPUT_DIRECTORY }}/.xylem/probe.json",
            "outputs": ["{{ OUTPUT_DIRECTORY }}/*.txt"]
        },
        "name": "probe",
        "programmingLanguage": "generic",
        "template": { "arguments": [], "command": "true" },
        "type": "script",
        "uri": "file:./probe"
    }))
    .expect("completion fixture should deserialize");
    let variables = HashMap::from([
        ("_XYLEM_OUTPUT_DIRECTORY".to_string(), root),
        ("_XYLEM_VALUE".to_string(), "one".to_string()),
    ]);
    (module, variables)
}
fn root(label: &str) -> PathBuf {
    std::env::temp_dir().join(format!("xylem-completion-{label}-{}", std::process::id()))
}

#[test]
fn test_complete_outputs_and_matching_marker_skip() {
    let root = root("skip");
    create_dir_all(&root).expect("create output root");
    write(root.join("result.txt"), "complete").expect("write output");
    let (module, variables) = fixture(&root);
    let plan = CompletionPlan::inspect(&module, &variables, false, None)
        .expect("inspect completion")
        .expect("completion plan");
    assert_eq!(plan.decision, CompletionDecision::Run);
    plan.write().expect("write marker");
    let resumed = CompletionPlan::inspect(&module, &variables, true, None)
        .expect("inspect resumed completion")
        .expect("completion plan");
    assert_eq!(resumed.decision, CompletionDecision::SkipExisting);
    remove_dir_all(root).expect("remove fixture");
}
#[test]
fn test_fingerprint_is_stable_for_equivalent_encodings_and_tracks_inputs() {
    let root = root("identity");
    let (module, variables) = fixture(&root);
    let identity = CompletionPlan::resolve(&module, &variables, None)
        .expect("resolve completion")
        .expect("completion")
        .identity;
    let yaml = serde_norway::to_string(&module).expect("serialize module as YAML");
    let equivalent: Module = serde_norway::from_str(&yaml).expect("deserialize equivalent YAML module");
    let equivalent_identity = CompletionPlan::resolve(&equivalent, &variables, None)
        .expect("resolve equivalent completion")
        .expect("completion")
        .identity;
    assert_eq!(identity, equivalent_identity);

    let mut changed_module = module.clone();
    changed_module.name = "changed".to_string();
    let changed_module_identity = CompletionPlan::resolve(&changed_module, &variables, None)
        .expect("resolve changed module")
        .expect("completion")
        .identity;
    assert_ne!(identity, changed_module_identity);

    let mut changed_template = module.clone();
    if let Some(crate::script::TemplateAttribute::Template(template)) = &mut changed_template.template {
        template.command = "false".to_string();
    }
    let changed_template_identity = CompletionPlan::resolve(&changed_template, &variables, None)
        .expect("resolve changed template")
        .expect("completion")
        .identity;
    assert_ne!(identity, changed_template_identity);

    let mut changed_variables = variables.clone();
    changed_variables.insert("_XYLEM_VALUE".to_string(), "two".to_string());
    let changed_variable_identity = CompletionPlan::resolve(&module, &changed_variables, None)
        .expect("resolve changed variables")
        .expect("completion")
        .identity;
    assert_ne!(identity, changed_variable_identity);

    let mut changed_declaration = module;
    changed_declaration.completion.as_mut().expect("completion").outputs = vec![format!("{}/*.tif", root.display())];
    let changed_declaration_identity = CompletionPlan::resolve(&changed_declaration, &variables, None)
        .expect("resolve changed declaration")
        .expect("completion")
        .identity;
    assert_ne!(identity, changed_declaration_identity);
}
#[test]
fn test_input_basename_refreshes_legacy_completion_markers_then_resumes() {
    let root = root("basename-cutover");
    create_dir_all(&root).expect("create output root");
    write(root.join("result.txt"), "complete").expect("write output");
    let (module, mut variables) = fixture(&root);
    variables.insert("_XYLEM_INPUT".to_string(), "/data/basin".to_string());
    variables.insert("_XYLEM_PARENT_DIRECTORY".to_string(), "basin".to_string());
    let legacy = CompletionPlan::inspect(&module, &variables, false, None)
        .expect("inspect legacy variables")
        .expect("completion plan");
    legacy.write().expect("write legacy marker");
    variables.remove("_XYLEM_PARENT_DIRECTORY");
    variables.insert("_XYLEM_INPUT_BASENAME".to_string(), "basin".to_string());
    let resumed = CompletionPlan::inspect(&module, &variables, true, None)
        .expect("inspect after hard cutover")
        .expect("completion plan");
    assert_ne!(resumed.identity, legacy.identity);
    assert_eq!(resumed.decision, CompletionDecision::Run);
    resumed.write().expect("write refreshed marker");
    let repeated = CompletionPlan::inspect(&module, &variables, true, None)
        .expect("inspect refreshed marker")
        .expect("completion plan");
    assert_eq!(repeated.decision, CompletionDecision::SkipExisting);
    variables.insert("_XYLEM_INPUT_BASENAME".to_string(), "different".to_string());
    let different = CompletionPlan::inspect(&module, &variables, true, None)
        .expect("inspect distinct value")
        .expect("completion plan");
    assert_eq!(different.decision, CompletionDecision::Run);
    variables.insert("_XYLEM_INPUT".to_string(), "/data/different".to_string());
    let changed_input = CompletionPlan::inspect(&module, &variables, true, None)
        .expect("inspect changed input")
        .expect("completion plan");
    assert_eq!(changed_input.decision, CompletionDecision::Run);
    remove_dir_all(root).expect("remove fixture");
}
#[test]
fn test_missing_empty_and_stale_outputs_run() {
    let root = root("incomplete");
    create_dir_all(&root).expect("create output root");
    let (module, variables) = fixture(&root);
    let missing = CompletionPlan::inspect(&module, &variables, true, None)
        .expect("inspect missing output")
        .expect("completion plan");
    assert_eq!(missing.decision, CompletionDecision::Run);
    write(root.join("result.txt"), "").expect("write empty output");
    missing.write().expect("write marker");
    let empty = CompletionPlan::inspect(&module, &variables, true, None)
        .expect("inspect empty output")
        .expect("completion plan");
    assert_eq!(empty.decision, CompletionDecision::Run);
    write(root.join("result.txt"), "complete").expect("write output");
    let mut changed = variables;
    changed.insert("_XYLEM_VALUE".to_string(), "two".to_string());
    let stale = CompletionPlan::inspect(&module, &changed, true, None)
        .expect("inspect stale marker")
        .expect("completion plan");
    assert_eq!(stale.decision, CompletionDecision::Run);
    remove_dir_all(root).expect("remove fixture");
}
#[test]
fn test_omitted_marker_uses_the_first_output_literal_prefix() {
    let root = root("default-marker");
    let (mut module, variables) = fixture(&root);
    module.completion.as_mut().expect("completion").marker = None;
    let completion = CompletionPlan::resolve(&module, &variables, None)
        .expect("resolve completion")
        .expect("completion");
    assert_eq!(completion.marker, root.join(".xylem/probe.json"));
}
#[test]
fn test_paths_are_normalized_lexically() {
    let root = root("normalize");
    let declared = root.join("nested/..");
    let (module, variables) = fixture(&declared);
    let completion = CompletionPlan::resolve(&module, &variables, None)
        .expect("resolve completion")
        .expect("completion");
    assert_eq!(completion.marker, root.join(".xylem/probe.json"));
    assert_eq!(completion.outputs, [root.join("*.txt")]);
}
#[cfg(unix)]
#[test]
fn test_paths_cannot_escape_the_filesystem_root() {
    let root = root("root-escape");
    let (mut module, variables) = fixture(&root);
    module.completion.as_mut().expect("completion").marker = Some("/../outside.json".to_string());
    let error = CompletionPlan::resolve(&module, &variables, None).expect_err("root escape must fail");
    assert!(error.to_string().contains("cannot be lexically normalized without escaping its root"));
}
#[test]
fn test_paths_outside_declared_roots_are_rejected() {
    let root = root("containment");
    let (mut module, variables) = fixture(&root);
    module.completion.as_mut().expect("completion").marker = Some("../outside.json".to_string());
    assert!(CompletionPlan::inspect(&module, &variables, true, None).is_err());
}
#[test]
fn test_resolved_outputs_are_sorted_and_deduplicated() {
    let root = root("order");
    let (mut module, variables) = fixture(&root);
    module.completion.as_mut().expect("completion").outputs = vec![
        "{{ OUTPUT_DIRECTORY }}/z*.txt".to_string(),
        "{{ OUTPUT_DIRECTORY }}/a*.txt".to_string(),
        "{{ OUTPUT_DIRECTORY }}/z*.txt".to_string(),
    ];
    let completion = CompletionPlan::resolve(&module, &variables, None)
        .expect("resolve completion")
        .expect("completion");
    assert_eq!(completion.outputs, [root.join("a*.txt"), root.join("z*.txt")]);
}
#[cfg(unix)]
#[test]
fn test_symlinked_outputs_cannot_escape_the_declared_root() {
    use std::os::unix::fs::symlink;

    let output_root = root("symlink-root");
    let outside = root("symlink-outside");
    create_dir_all(&output_root).expect("create output root");
    create_dir_all(&outside).expect("create outside root");
    write(outside.join("result.txt"), "complete").expect("write outside output");
    symlink(&outside, output_root.join("escape")).expect("create directory symlink");
    let (mut module, variables) = fixture(&output_root);
    module.completion.as_mut().expect("completion").outputs = vec!["{{ OUTPUT_DIRECTORY }}/escape/*.txt".to_string()];
    let error = CompletionPlan::inspect(&module, &variables, true, None).expect_err("symlink escape must fail");
    assert!(error.to_string().contains("resolves outside every declared output root"));
    remove_dir_all(output_root).expect("remove output root");
    remove_dir_all(outside).expect("remove outside root");
}
