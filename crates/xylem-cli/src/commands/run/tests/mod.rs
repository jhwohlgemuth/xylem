#![expect(clippy::expect_used, reason = "test fixtures should fail immediately when setup or preparation fails")]
use super::preparation::configure_preview;
use super::*;
use core::sync::atomic::AtomicBool;
use xylem_lib::config::{Config, ConfigOptions};

fn config(failure_policy: FailurePolicy) -> Config {
    let mut config: Config = serde_json::from_value(serde_json::json!({"requirements": [], "modules": []})).expect("preparation configuration");
    config.options = Some(ConfigOptions::init().failure_policy(failure_policy).build());
    config
}
fn request<'request>(
    failure_policy: Option<FailurePolicy>,
    path: &'request Option<PathBuf>,
    values: &'request Option<Vec<String>>,
) -> RunRequest<'request> {
    RunRequest {
        artifact_output: path,
        cancellation: None,
        cli_allow_insecure_http: false,
        cli_allowed: values,
        cli_container_cleanup: None,
        cli_dry_run: false,
        cli_failure_policy: failure_policy,
        cli_gpu_monitor: false,
        cli_input: values,
        cli_no_container_env: false,
        cli_offline: false,
        cli_resume: false,
        cli_skip_verify_checksum: false,
        cli_skip_verify_requirements: false,
        config: path,
        limits: ExecutionLimits::default(),
        module_index: None,
        no_progress: true,
        positional: &[],
        report_output: path,
        reporter: &[],
        retain_workspace: false,
        sandbox_policy: path,
        variable_overrides: VariableOverrides::default(),
    }
}

#[test]
fn test_failure_policy_configuration_is_preserved_without_override() {
    let path = None;
    let values = None;
    let request = request(None, &path, &values);
    let configured = configure_preview(config(FailurePolicy::Continue), &request, request.artifact_output.as_deref()).expect("configure preview");
    assert_eq!(configured.options.map(|options| options.failure_policy), Some(FailurePolicy::Continue));
}
#[test]
fn test_failure_policy_override_replaces_configuration() {
    let path = None;
    let values = None;
    let request = request(Some(FailurePolicy::FailFast), &path, &values);
    let configured = configure_preview(config(FailurePolicy::Continue), &request, request.artifact_output.as_deref()).expect("configure preview");
    assert_eq!(configured.options.map(|options| options.failure_policy), Some(FailurePolicy::FailFast));
}
#[test]
fn test_signal_action_cancels_once_then_exits() {
    let received = AtomicBool::new(false);
    assert_eq!(signal_action(&received), SignalAction::Cancel);
    assert_eq!(signal_action(&received), SignalAction::Exit);
}
