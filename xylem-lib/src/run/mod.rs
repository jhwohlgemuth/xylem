//! Shared workflow cancellation primitives.
extern crate alloc;

use acorn_cmd::CancellationToken;
use alloc::sync::Arc;
use core::sync::atomic::{AtomicU8, Ordering};

/// Canonical durable run bundles and history discovery.
pub mod store;
/// Lightweight host and process-tree sampling.
pub mod telemetry;

/// Shared two-stage cancellation handle.
#[derive(Clone, Debug, Default)]
pub struct RunCancellation {
    requests: Arc<AtomicU8>,
    token: CancellationToken,
}

impl RunCancellation {
    /// Force managed child termination immediately.
    pub fn force(&self) {
        self.requests.store(2, Ordering::Release);
        self.token.cancel();
    }
    /// Return whether graceful cancellation was requested.
    pub fn is_requested(&self) -> bool {
        self.requests.load(Ordering::Acquire) > 0
    }
    /// Request graceful cancellation first and force termination on repetition.
    pub fn request(&self) {
        if self.requests.fetch_add(1, Ordering::AcqRel) > 0 {
            self.token.cancel();
        }
    }
    /// Borrow the ACORN token used by active managed commands.
    pub fn token(&self) -> CancellationToken {
        self.token.clone()
    }
}

#[cfg(test)]
mod tests;
