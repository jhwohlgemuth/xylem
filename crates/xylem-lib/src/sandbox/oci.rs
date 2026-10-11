extern crate alloc;

use super::artifact::ArtifactManager;
use super::{
    private, reconcile, EffectiveSandboxPolicy, IsolationClass, OciSandboxSession, SandboxCapabilities, SandboxError, SandboxLease, SandboxProvider,
    SandboxProviderClient, SandboxProviderKind, SandboxReconciliationOutcome, SandboxResourceTracker, SandboxSession, WorkloadAcquisition,
};
use alloc::sync::Arc;

pub(super) struct OciProvider<'a> {
    policy: &'a EffectiveSandboxPolicy,
}
impl private::Sealed for OciProvider<'_> {}
impl SandboxProvider for OciProvider<'_> {
    fn discover(&self) -> Result<SandboxCapabilities, SandboxError> {
        self.policy
            .runtime()
            .and_then(|client| SandboxProviderClient::discover(&client))
            .map(|()| SandboxCapabilities {
                isolation: IsolationClass::SharedKernel,
                network_deny: true,
                provider: SandboxProviderKind::Oci,
                read_only_root: true,
                resource_limits: true,
                rootless: true,
            })
            .and_then(|capabilities| match capabilities.isolation >= self.policy.policy.minimum_isolation {
                | true => Ok(capabilities),
                | false => Err(SandboxError::Runtime(format!(
                    "provider isolation {:?} does not satisfy required {:?}",
                    capabilities.isolation, self.policy.policy.minimum_isolation
                ))),
            })
    }
    fn reconcile(&self, leases: &[SandboxLease]) -> SandboxReconciliationOutcome {
        reconcile::resources(self.policy, leases)
    }
    fn open(&self, run_id: &str, acquisition: WorkloadAcquisition) -> Result<Arc<dyn SandboxSession>, SandboxError> {
        match run_id.is_empty() {
            | true => Err(SandboxError::Validation("sandbox run identifier must not be empty".to_string())),
            | false => self.policy.prepare_workload(run_id, acquisition).map(|()| {
                Arc::new(OciSandboxSession {
                    artifacts: ArtifactManager::new(
                        run_id.to_string(),
                        self.policy.policy.paths.artifact_roots.clone(),
                        self.policy.policy.resources.clone(),
                    ),
                    policy: (*self.policy).clone(),
                    resources: Arc::new(SandboxResourceTracker::default()),
                    run_id: run_id.to_string(),
                }) as Arc<dyn SandboxSession>
            }),
        }
    }
}
impl<'a> OciProvider<'a> {
    pub(super) fn new(policy: &'a EffectiveSandboxPolicy) -> Self {
        Self { policy }
    }
}
