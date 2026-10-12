use super::*;
use crate::journal::{ConfigurationIdentity, InputSummary, ModuleSummary, SandboxEvidence, JOURNAL_SCHEMA_VERSION};
use acorn_core::time::UnitOfTime;

fn summary(state: ExecutionState, started: u64, updated: u64) -> RunSummary {
    RunSummary {
        artifacts: None,
        configuration_acquisitions: Vec::new(),
        schema_version: JOURNAL_SCHEMA_VERSION,
        run_id: "run".to_string(),
        configuration: ConfigurationIdentity {
            path: "workflow.json".to_string(),
            id: None,
            version: None,
        },
        dry_run: false,
        sandbox: SandboxEvidence::off(),
        started_at: Milliseconds::new(0),
        finished_at: Some(Milliseconds::new(updated)),
        duration: Some(Milliseconds::new(updated)),
        status: RunStatus::Succeeded,
        error: None,
        gpu_monitoring: None,
        containers: Vec::new(),
        inputs: vec![InputSummary {
            index: 0,
            input_id: "input-000000".to_string(),
            value: None,
            state,
            error: None,
            started_at: Some(Milliseconds::new(started)),
            updated_at: Milliseconds::new(updated),
            modules: vec![ModuleSummary {
                acquisition: None,
                index: 0,
                name: "module".to_string(),
                module_path: vec!["module".to_string()],
                state,
                error: None,
                started_at: Some(Milliseconds::new(started)),
                updated_at: Milliseconds::new(updated),
            }],
        }],
        memory: None,
    }
}

#[test]
fn test_history_projects_remaining_module_duration() {
    let completed = summary(ExecutionState::Succeeded, 10, 110);
    let history = DurationHistory::from_summaries([&completed]);
    let pending = summary(ExecutionState::Pending, 0, 0);
    let projection = history.project(&pending, 50, 1).expect("history should project");
    assert_eq!(projection.confidence, ProjectionConfidence::Historical);
    assert_eq!(projection.eta, Milliseconds::new(100));
}
