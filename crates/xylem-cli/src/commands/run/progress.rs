use core::time::Duration;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use xylem_lib::progress::{ProgressObserver, ProgressPhase, ProgressState, ProgressUpdate};

pub(super) struct WorkflowProgress {
    preparation: ProgressBar,
    execution: ProgressBar,
}
impl ProgressObserver for WorkflowProgress {
    fn update(&self, update: ProgressUpdate) {
        let bar = match update.phase {
            | ProgressPhase::Preparation => &self.preparation,
            | ProgressPhase::Execution => &self.execution,
        };
        match update.state {
            | ProgressState::Cancelled => {
                bar.set_position(bar.position().saturating_add(1));
                bar.set_message(format!("{} cancelled", update.module_path.join("::")));
            }
            | ProgressState::Started => {
                let module = update.module_path.join("::");
                let message = match update.input {
                    | Some(input) => format!("{module} · {input}"),
                    | None => module,
                };
                bar.set_message(message);
            }
            | ProgressState::Finished | ProgressState::Skipped | ProgressState::SkippedExisting => {
                bar.set_position(bar.position().saturating_add(1));
            }
            | ProgressState::Failed => {
                bar.set_position(bar.position().saturating_add(1));
                bar.set_message(format!("{} failed", update.module_path.join("::")));
            }
        }
    }
}
impl WorkflowProgress {
    pub(super) fn new(preparation_total: u64, execution_total: u64, disabled: bool) -> Self {
        match disabled {
            | true => Self {
                preparation: ProgressBar::hidden(),
                execution: ProgressBar::hidden(),
            },
            | false => {
                let multi = MultiProgress::new();
                let style = ProgressStyle::with_template("{spinner:.cyan} {msg:28} [{bar:32.cyan/blue}] {pos}/{len}")
                    .unwrap_or_else(|_| ProgressStyle::default_bar())
                    .progress_chars("=>-");
                let preparation = multi.add(ProgressBar::new(preparation_total));
                preparation.set_style(style.clone());
                preparation.set_message("Preparing modules");
                preparation.enable_steady_tick(Duration::from_millis(100));
                let execution = multi.add(ProgressBar::new(execution_total));
                execution.set_style(style);
                execution.set_message("Waiting to execute");
                execution.enable_steady_tick(Duration::from_millis(100));
                Self { preparation, execution }
            }
        }
    }
    pub(super) fn finish_execution(&self, succeeded: bool) {
        match succeeded {
            | true => self.execution.finish_with_message("Workflow complete"),
            | false => self.execution.abandon_with_message("Workflow failed"),
        }
    }
    pub(super) fn finish_preparation(&self, succeeded: bool) {
        match succeeded {
            | true => self.preparation.finish_with_message("Modules prepared"),
            | false => self.preparation.abandon_with_message("Module preparation failed"),
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::expect_used,
        clippy::indexing_slicing,
        clippy::panic,
        clippy::unwrap_used,
        clippy::arithmetic_side_effects
    )]
    use super::*;

    #[test]
    fn test_disabled_progress_uses_hidden_bars() {
        let progress = WorkflowProgress::new(2, 4, true);
        assert!(progress.preparation.is_hidden());
        assert!(progress.execution.is_hidden());
    }
}
