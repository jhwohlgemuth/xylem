use super::*;
use clap::CommandFactory;

fn parse(arguments: &[&str]) -> Arguments {
    Arguments::try_parse_from(arguments).expect("arguments should parse")
}

#[test]
fn test_allowed_accepts_repeated_and_comma_delimited_values() {
    let args = parse(&["xylem", "run", "--allowed", "one.example,two.example:8443", "--allowed", "three.example"]);
    match args.command {
        | Some(Commands::Run { allowed, .. }) => assert_eq!(
            allowed,
            Some(vec![
                "one.example".to_string(),
                "two.example:8443".to_string(),
                "three.example".to_string(),
            ])
        ),
        | _ => panic!("run command should parse"),
    }
}

#[test]
fn test_artifact_and_report_outputs_are_distinct() {
    let args = parse(&["xylem", "run", "--artifact-output", "artifacts", "--report-output", "reports"]);
    match args.command {
        | Some(Commands::Run {
            artifact_output,
            report_output,
            ..
        }) => {
            assert_eq!(artifact_output, Some(PathBuf::from("artifacts")));
            assert_eq!(report_output, Some(PathBuf::from("reports")));
        }
        | _ => panic!("run command should parse"),
    }
    match parse(&["xylem", "run", "--output", "reports"]).command {
        | Some(Commands::Run { report_output, .. }) => {
            assert_eq!(report_output, Some(PathBuf::from("reports")));
        }
        | _ => panic!("run command should parse"),
    }
}

#[test]
fn test_container_policy_flags_parse() {
    match parse(&["xylem", "run", "--container-cleanup", "on-success", "--no-container-env"]).command {
        | Some(Commands::Run {
            container_cleanup,
            no_container_env,
            ..
        }) => {
            assert_eq!(container_cleanup, Some(ContainerCleanup::OnSuccess));
            assert!(no_container_env);
        }
        | _ => panic!("run command should parse"),
    }
}

#[test]
fn test_global_env_and_progress_flags_parse_before_or_after_run() {
    let before = parse(&["xylem", "--env", "custom.env", "--no-progress", "run"]);
    assert_eq!(before.env, Some(PathBuf::from("custom.env")));
    assert!(before.no_progress);
    let after = parse(&["xylem", "run", "--env", "custom.env", "--no-progress"]);
    assert_eq!(after.env, Some(PathBuf::from("custom.env")));
    assert!(after.no_progress);
}

#[test]
fn test_gpu_monitor_is_explicit() {
    match parse(&["xylem", "run", "--gpu-monitor"]).command {
        | Some(Commands::Run { gpu_monitor, .. }) => assert!(gpu_monitor),
        | _ => panic!("run command should parse"),
    }
    match parse(&["xylem", "run"]).command {
        | Some(Commands::Run { gpu_monitor, .. }) => assert!(!gpu_monitor),
        | _ => panic!("run command should parse"),
    }
}

#[test]
fn test_help_documents_thread_contract() {
    let help = Arguments::command().render_long_help().to_string();
    assert!(help.contains("-t, --threads <N>"));
    assert!(help.contains("[default: 10]"));
    assert!(help.contains("does not limit child tools"));
}

#[test]
fn test_reporters_are_typed_repeatable_and_default_to_console() {
    match parse(&["xylem", "run"]).command {
        | Some(Commands::Run { reporter, .. }) => assert_eq!(reporter, vec![ReporterType::Console]),
        | _ => panic!("run command should parse"),
    }
    match parse(&["xylem", "run", "--reporter", "json,console", "--reporter", "json"]).command {
        | Some(Commands::Run { reporter, .. }) => assert_eq!(reporter, vec![ReporterType::Json, ReporterType::Console, ReporterType::Json]),
        | _ => panic!("run command should parse"),
    }
    assert!(Arguments::try_parse_from(["xylem", "run", "--reporter", "unknown"]).is_err());
}

#[test]
fn test_resume_is_explicit() {
    match parse(&["xylem", "run", "--resume"]).command {
        | Some(Commands::Run { resume, .. }) => assert!(resume),
        | _ => panic!("run command should parse"),
    }
    match parse(&["xylem", "run"]).command {
        | Some(Commands::Run { resume, .. }) => assert!(!resume),
        | _ => panic!("run command should parse"),
    }
}

#[test]
fn test_run_configuration_is_positional_and_config_flag_is_rejected() {
    match parse(&["xylem", "run", "workflow.json"]).command {
        | Some(Commands::Run { config, .. }) => assert_eq!(config, Some(PathBuf::from("workflow.json"))),
        | _ => panic!("run command should parse"),
    }
    assert!(Arguments::try_parse_from(["xylem", "run", "--config", "workflow.json"]).is_err());
}

#[test]
fn test_threads_accept_short_and_long_flags() {
    assert_eq!(parse(&["xylem", "-t", "1", "run"]).threads.threads(), 1);
    assert_eq!(parse(&["xylem", "run", "--threads", "24"]).threads.threads(), 24);
}

#[test]
fn test_threads_default_to_ten() {
    assert_eq!(parse(&["xylem"]).threads.threads(), 10);
}

#[test]
fn test_threads_reject_zero_non_numeric_and_missing_values() {
    for arguments in [
        &["xylem", "--threads", "0"][..],
        &["xylem", "--threads", "many"][..],
        &["xylem", "--threads"][..],
    ] {
        assert!(Arguments::try_parse_from(arguments).is_err(), "{arguments:?} should fail");
    }
    let zero = Arguments::try_parse_from(["xylem", "--threads", "0"])
        .expect_err("zero should fail")
        .to_string();
    assert!(zero.contains("thread count must be greater than zero"));
}

#[cfg(feature = "tui")]
#[test]
fn test_tui_configuration_is_optional_and_positional() {
    match parse(&["xylem", "tui", "workflow.jsonc"]).command {
        | Some(Commands::Tui { config }) => assert_eq!(config, Some(PathBuf::from("workflow.jsonc"))),
        | _ => panic!("tui command should parse"),
    }
    match parse(&["xylem", "tui"]).command {
        | Some(Commands::Tui { config }) => assert_eq!(config, None),
        | _ => panic!("tui command should parse"),
    }
}
