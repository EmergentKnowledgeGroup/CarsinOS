use crate::auth::authenticate_request;
use crate::executor::FixedExecutor;
use crate::journal::{Journal, JournalAdmissionOutcome, JournalError, JournalExecutionState};
use crate::state_verifier::{ReadOnlyBeganVerifier, VerificationError};
use carsinos_protocol::execass_recorder::{
    RecorderBindingV1, RecorderHandshakeAttestationV1, RecorderHandshakeChallengeV1,
    RecorderObservationKindV1, RecorderReplyV1, RecorderRequestV1, RECORDER_HANDSHAKE_VERSION,
    RECORDER_PROTOCOL_VERSION,
};
use sha2::Digest;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::Mutex;
use zeroize::Zeroizing;

#[derive(Debug, thiserror::Error)]
pub enum ServiceError {
    #[error("request authentication failed")]
    Authentication,
    #[error("request is not supported by the installed fixed adapter")]
    UnsupportedAdapter,
    #[error("reconciliation identity or consistency window did not match journal truth")]
    InvalidReconciliation,
    #[error(transparent)]
    Verification(#[from] VerificationError),
    #[error(transparent)]
    Journal(#[from] JournalError),
}

pub struct RecorderService {
    channel_key: Zeroizing<[u8; 32]>,
    verifier: ReadOnlyBeganVerifier,
    journal: Mutex<Journal>,
    executor: FixedExecutor,
    #[cfg(feature = "test-support")]
    test_coordination: Option<TestCoordination>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
#[value(rename_all = "snake_case")]
#[allow(clippy::enum_variant_names)] // Names intentionally mirror the locked crash boundaries.
pub enum TestFailpoint {
    BeforeAcceptedFsync,
    AfterAcceptedFsync,
    AfterInvocationStartedFsync,
    AfterProviderLedgerFsync,
    AfterTerminalFsync,
}

#[cfg(feature = "test-support")]
#[derive(Debug, Clone)]
struct TestCoordination {
    failpoint: TestFailpoint,
    root: std::path::PathBuf,
}

impl std::fmt::Debug for RecorderService {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RecorderService")
            .field("verifier", &self.verifier)
            .field("secret_material", &"[REDACTED]")
            .finish_non_exhaustive()
    }
}

impl RecorderService {
    pub fn production(
        channel_key: [u8; 32],
        verifier: ReadOnlyBeganVerifier,
        journal: Journal,
    ) -> Arc<Self> {
        Arc::new(Self {
            channel_key: Zeroizing::new(channel_key),
            verifier,
            journal: Mutex::new(journal),
            executor: FixedExecutor::ExactOverwrite,
            #[cfg(feature = "test-support")]
            test_coordination: None,
        })
    }

    #[cfg(feature = "test-support")]
    pub fn with_fake_provider(
        channel_key: [u8; 32],
        verifier: ReadOnlyBeganVerifier,
        journal: Journal,
        fixture_root: std::path::PathBuf,
    ) -> Arc<Self> {
        Arc::new(Self {
            channel_key: Zeroizing::new(channel_key),
            verifier,
            journal: Mutex::new(journal),
            executor: FixedExecutor::TestFakeProvider {
                fixture_root,
                provider_coordination_root: None,
            },
            test_coordination: None,
        })
    }

    #[cfg(feature = "test-support")]
    pub fn with_fake_provider_coordination(
        channel_key: [u8; 32],
        verifier: ReadOnlyBeganVerifier,
        journal: Journal,
        fixture_root: std::path::PathBuf,
        failpoint: TestFailpoint,
        coordination_root: std::path::PathBuf,
    ) -> Arc<Self> {
        Arc::new(Self {
            channel_key: Zeroizing::new(channel_key),
            verifier,
            journal: Mutex::new(journal),
            executor: FixedExecutor::TestFakeProvider {
                fixture_root,
                provider_coordination_root: (failpoint == TestFailpoint::AfterProviderLedgerFsync)
                    .then(|| coordination_root.clone()),
            },
            test_coordination: Some(TestCoordination {
                failpoint,
                root: coordination_root,
            }),
        })
    }

    pub async fn handle(&self, request: RecorderRequestV1) -> RecorderReplyV1 {
        let request_id = request.request_id().to_owned();
        match self.handle_inner(request).await {
            Ok(reply) => reply,
            Err(error) => RecorderReplyV1::Rejected {
                request_id,
                code: error_code(&error).into(),
            },
        }
    }

    pub(crate) async fn handshake(
        &self,
        challenge: RecorderHandshakeChallengeV1,
        server_nonce: String,
    ) -> Result<RecorderHandshakeAttestationV1, ServiceError> {
        if challenge.handshake_version != RECORDER_HANDSHAKE_VERSION
            || challenge.client_nonce.is_empty()
        {
            return Err(ServiceError::Authentication);
        }
        let authoritative = self.verifier.load_authoritative_binding(now_ms())?;
        let binding = RecorderBindingV1 {
            protocol_version: RECORDER_PROTOCOL_VERSION.into(),
            canonical_root_identity: authoritative.canonical_root_identity,
            installation_id: authoritative.installation_id,
            state_root_generation: authoritative.state_root_generation,
            os_user_identity_digest: authoritative.os_user_identity_digest,
            runtime_host_generation: authoritative.runtime_host_generation,
            runtime_host_instance_id: authoritative.runtime_host_instance_id,
            runtime_fencing_token: authoritative.runtime_fencing_token,
        };
        if challenge.binding != binding {
            return Err(ServiceError::Authentication);
        }
        self.journal
            .lock()
            .await
            .sign_handshake(&challenge, binding, server_nonce)
            .map_err(ServiceError::Journal)
    }

    #[doc(hidden)]
    #[cfg(feature = "test-support")]
    pub async fn test_hold_journal(
        &self,
        entered: tokio::sync::oneshot::Sender<()>,
        release: tokio::sync::oneshot::Receiver<()>,
    ) {
        let _guard = self.journal.lock().await;
        let _ = entered.send(());
        let _ = release.await;
    }

    async fn handle_inner(
        &self,
        request: RecorderRequestV1,
    ) -> Result<RecorderReplyV1, ServiceError> {
        authenticate_request(&request, &self.channel_key)
            .map_err(|_| ServiceError::Authentication)?;
        match request {
            RecorderRequestV1::QueryOnly(query) => {
                let journal = self.journal.lock().await;
                match journal.query(
                    &query.binding.installation_id,
                    query.binding.state_root_generation,
                    &query.attempt_id,
                    query.expected_command_digest.as_deref(),
                )? {
                    Some(observation) => Ok(RecorderReplyV1::Observation {
                        request_id: query.request_id,
                        replayed: true,
                        observation: Box::new(observation.into()),
                    }),
                    None => Ok(RecorderReplyV1::NotFound {
                        request_id: query.request_id,
                    }),
                }
            }
            RecorderRequestV1::Reconcile(reconcile) => {
                let now = now_ms();
                if reconcile.consistency_window_end_ms > now {
                    return Err(ServiceError::InvalidReconciliation);
                }
                let mut journal = self.journal.lock().await;
                let Some(prior) = journal.query(
                    &reconcile.binding.installation_id,
                    reconcile.binding.state_root_generation,
                    &reconcile.attempt_id,
                    Some(&reconcile.expected_command_digest),
                )?
                else {
                    return Ok(RecorderReplyV1::NotFound {
                        request_id: reconcile.request_id,
                    });
                };
                if prior.canonical_root_identity != reconcile.binding.canonical_root_identity
                    || prior.os_user_identity_digest != reconcile.binding.os_user_identity_digest
                    || prior.reconciliation_key_digest.as_deref()
                        != Some(reconcile.reconciliation_key_digest.as_str())
                {
                    return Err(ServiceError::InvalidReconciliation);
                }
                if prior.source
                    == carsinos_protocol::execass_recorder::RecorderObservationSourceV1::Reconciliation
                    && (matches!(
                        prior.kind,
                        RecorderObservationKindV1::Present | RecorderObservationKindV1::Absent
                    ) || (prior.reconciliation_window_start_ms
                        == Some(reconcile.consistency_window_start_ms)
                        && prior.reconciliation_window_end_ms
                            == Some(reconcile.consistency_window_end_ms)))
                {
                    return Ok(RecorderReplyV1::Observation {
                        request_id: reconcile.request_id,
                        replayed: true,
                        observation: Box::new(prior.into()),
                    });
                }
                if prior.source
                    == carsinos_protocol::execass_recorder::RecorderObservationSourceV1::Execution
                    && matches!(
                        prior.kind,
                        RecorderObservationKindV1::Present | RecorderObservationKindV1::Absent
                    )
                {
                    return Ok(RecorderReplyV1::Observation {
                        request_id: reconcile.request_id,
                        replayed: true,
                        observation: Box::new(prior.into()),
                    });
                }
                let exact_reservations =
                    if prior.provider_identity == crate::EXACT_OVERWRITE_PROVIDER_IDENTITY {
                        Some(self.verifier.verify_reconciliation_reservations(
                            &prior.attempt_id,
                            &prior.logical_effect_id,
                            &prior.provider_request_digest,
                            now,
                        )?)
                    } else {
                        None
                    };
                let outcome = if prior.kind == RecorderObservationKindV1::Accepted {
                    let evidence = serde_json::json!({
                        "attempt_id": reconcile.attempt_id,
                        "basis": "accepted_without_invocation_started",
                        "technical_resource_actuals": [],
                    });
                    let bytes = serde_json::to_vec(&evidence)
                        .map_err(|_| ServiceError::InvalidReconciliation)?;
                    crate::executor::ReconciliationOutcome {
                        kind: RecorderObservationKindV1::Absent,
                        response_digest: format!(
                            "sha256:{}",
                            crate::hex_encode(&sha2::Sha256::digest(
                                b"accepted-without-invocation-started"
                            ))
                        ),
                        evidence_payload_digest: format!(
                            "sha256:{}",
                            crate::hex_encode(&sha2::Sha256::digest(bytes))
                        ),
                        remote_effect_id: None,
                        technical_resource_actuals: Vec::new(),
                    }
                } else {
                    match self
                        .executor
                        .reconcile(
                            &reconcile,
                            &prior.provider_identity,
                            &prior.provider_version,
                            exact_reservations.as_deref().unwrap_or(&[]),
                        )
                        .await
                    {
                        Ok(outcome) => outcome,
                        Err(_) => {
                            let payload = serde_json::json!({
                                "attempt_id": reconcile.attempt_id,
                                "basis": "inconclusive_provider_query",
                                "technical_resource_actuals": [],
                            });
                            crate::executor::ReconciliationOutcome {
                                kind: RecorderObservationKindV1::Unknown,
                                response_digest: format!(
                                    "sha256:{}",
                                    crate::hex_encode(&sha2::Sha256::digest(
                                        b"inconclusive-provider-query"
                                    ))
                                ),
                                evidence_payload_digest: format!(
                                    "sha256:{}",
                                    crate::hex_encode(&sha2::Sha256::digest(
                                        serde_json::to_vec(&payload)
                                            .map_err(|_| ServiceError::InvalidReconciliation)?
                                    ))
                                ),
                                remote_effect_id: None,
                                technical_resource_actuals: Vec::new(),
                            }
                        }
                    }
                };
                let observation = journal.record_reconciliation(
                    &reconcile.binding.installation_id,
                    reconcile.binding.state_root_generation,
                    &reconcile.attempt_id,
                    &reconcile.expected_command_digest,
                    outcome.kind,
                    outcome.response_digest,
                    outcome.evidence_payload_digest,
                    outcome.remote_effect_id,
                    outcome.technical_resource_actuals,
                    reconcile.consistency_window_start_ms,
                    reconcile.consistency_window_end_ms,
                    now_ms(),
                )?;
                Ok(RecorderReplyV1::Observation {
                    request_id: reconcile.request_id,
                    replayed: false,
                    observation: Box::new(observation.into()),
                })
            }
            RecorderRequestV1::ExecuteOnce(command) => {
                if !self.executor.supports(&command) {
                    return Err(ServiceError::UnsupportedAdapter);
                }
                let permit = {
                    let mut journal = self.journal.lock().await;
                    match journal.execution_state(&command)? {
                        JournalExecutionState::Replay(existing) => {
                            return Ok(RecorderReplyV1::Observation {
                                request_id: command.request_id,
                                replayed: true,
                                observation: Box::new((*existing).into()),
                            });
                        }
                        JournalExecutionState::RequiresLiveVerification => {}
                    }
                    // Fresh and Accepted-only attempts are verified only after
                    // acquiring the journal mutex. A queued caller therefore
                    // cannot cross a stop, takeover, or fence change.
                    let now = now_ms();
                    let admission = self.verifier.verify(&command, now)?;
                    self.test_hit(TestFailpoint::BeforeAcceptedFsync);
                    match journal.admit_verified_execution(admission, now, now_ms(), || {
                        self.test_hit(TestFailpoint::AfterAcceptedFsync);
                    })? {
                        JournalAdmissionOutcome::FreshStarted(permit) => permit,
                        JournalAdmissionOutcome::Replay(existing) => {
                            return Ok(RecorderReplyV1::Observation {
                                request_id: command.request_id,
                                replayed: true,
                                observation: Box::new((*existing).into()),
                            });
                        }
                    }
                };
                self.test_hit(TestFailpoint::AfterInvocationStartedFsync);
                // The journal lock is deliberately released before provider I/O.
                let outcome = match self.executor.invoke((*permit).into_admission()).await {
                    Ok(outcome) => outcome,
                    Err(_) => crate::executor::InvocationOutcome {
                        kind: RecorderObservationKindV1::Unknown,
                        response_digest: format!(
                            "sha256:{}",
                            crate::hex_encode(&sha2::Sha256::digest(b"ambiguous-provider-error"))
                        ),
                        evidence_payload_digest: format!(
                            "sha256:{}",
                            crate::hex_encode(&sha2::Sha256::digest(
                                b"ambiguous-provider-error-evidence"
                            ))
                        ),
                        remote_effect_id: None,
                        provider_error_class: None,
                        technical_resource_actuals: Vec::new(),
                    },
                };
                self.test_hit(TestFailpoint::AfterProviderLedgerFsync);
                let terminal = self.journal.lock().await.record_terminal(
                    &command,
                    outcome.kind,
                    outcome.response_digest,
                    outcome.evidence_payload_digest,
                    outcome.remote_effect_id,
                    outcome.provider_error_class,
                    outcome.technical_resource_actuals,
                    now_ms(),
                )?;
                self.test_hit(TestFailpoint::AfterTerminalFsync);
                Ok(RecorderReplyV1::Observation {
                    request_id: command.request_id,
                    replayed: false,
                    observation: Box::new(terminal.into()),
                })
            }
        }
    }

    #[cfg(feature = "test-support")]
    fn test_hit(&self, failpoint: TestFailpoint) {
        let Some(coordination) = &self.test_coordination else {
            return;
        };
        if coordination.failpoint != failpoint {
            return;
        }
        let name = format!("{:?}", failpoint).to_ascii_lowercase();
        let reached = coordination.root.join(format!("{name}.reached"));
        let resume = coordination.root.join(format!("{name}.continue"));
        let _ = std::fs::create_dir_all(&coordination.root);
        std::fs::write(&reached, std::process::id().to_string())
            .expect("writing test-only recorder failpoint marker");
        while !resume.exists() {
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }

    #[cfg(not(feature = "test-support"))]
    fn test_hit(&self, _failpoint: TestFailpoint) {}
}

fn error_code(error: &ServiceError) -> &'static str {
    match error {
        ServiceError::Authentication => "authentication_failed",
        ServiceError::UnsupportedAdapter => "adapter_not_installed",
        ServiceError::InvalidReconciliation => "reconciliation_identity_mismatch",
        ServiceError::Verification(VerificationError::Expired) => "deadline_expired",
        ServiceError::Verification(_) => "began_not_proven",
        ServiceError::Journal(JournalError::CommandConflict) => "command_conflict",
        ServiceError::Journal(_) => "journal_failed",
    }
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(i64::MAX)
}
