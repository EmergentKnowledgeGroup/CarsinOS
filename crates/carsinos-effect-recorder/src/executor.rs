use crate::state_verifier::VerifiedExecuteOnceAdmission;
use carsinos_protocol::execass_recorder::{
    ProviderFailureClassV1, ReconcileV1, RecorderObservationKindV1, TechnicalResourceActualV1,
};
#[cfg(feature = "test-support")]
use sha2::{Digest, Sha256};
#[cfg(feature = "test-support")]
use std::path::PathBuf;

#[derive(Debug)]
pub(crate) struct InvocationOutcome {
    pub kind: RecorderObservationKindV1,
    pub response_digest: String,
    pub evidence_payload_digest: String,
    pub remote_effect_id: Option<String>,
    /// Adapter-derived only. Definite execution absence requires one; an
    /// ambiguous post-invocation adapter error remains outcome `Unknown` with
    /// no class.
    pub provider_error_class: Option<ProviderFailureClassV1>,
    pub technical_resource_actuals: Vec<TechnicalResourceActualV1>,
}

#[derive(Debug)]
pub(crate) struct ReconciliationOutcome {
    pub kind: RecorderObservationKindV1,
    pub response_digest: String,
    pub evidence_payload_digest: String,
    pub remote_effect_id: Option<String>,
    pub technical_resource_actuals: Vec<TechnicalResourceActualV1>,
}

#[derive(Debug, Clone)]
pub(crate) enum FixedExecutor {
    ExactOverwrite,
    #[cfg(feature = "test-support")]
    TestFakeProvider {
        fixture_root: PathBuf,
        provider_coordination_root: Option<PathBuf>,
    },
}

impl FixedExecutor {
    pub(crate) fn supports(
        &self,
        command: &carsinos_protocol::execass_recorder::ExecuteOnceV1,
    ) -> bool {
        #[cfg(not(feature = "test-support"))]
        let _ = command;
        match self {
            Self::ExactOverwrite => crate::exact_overwrite::supports_exact_overwrite(command),
            #[cfg(feature = "test-support")]
            Self::TestFakeProvider { .. } => {
                command.adapter_identity == "ea213.fake-provider.v1"
                    && command.provider_identity == "fake-provider"
                    && command.provider_version == "v1"
                    && fixed_fake_provider_artifact_digest()
                        .is_some_and(|digest| command.adapter_artifact_digest == digest)
                    && command.operand_envelope.secret_handles.is_empty()
                    && command.operand_envelope.non_secret == serde_json::json!({"fixture": true})
            }
        }
    }

    pub(crate) async fn invoke(
        &self,
        admission: VerifiedExecuteOnceAdmission,
    ) -> anyhow::Result<InvocationOutcome> {
        #[cfg(not(feature = "test-support"))]
        let _ = admission;
        match self {
            Self::ExactOverwrite => Ok(crate::exact_overwrite::invoke_exact_overwrite(
                admission.command(),
                admission.technical_resource_reservations(),
            )),
            #[cfg(feature = "test-support")]
            Self::TestFakeProvider {
                fixture_root,
                provider_coordination_root,
            } => {
                invoke_fake_provider(
                    fixture_root,
                    admission.command(),
                    provider_coordination_root.as_deref(),
                )
                .await
            }
        }
    }

    pub(crate) async fn reconcile(
        &self,
        request: &ReconcileV1,
        provider_identity: &str,
        provider_version: &str,
        reservations: &[crate::state_verifier::VerifiedTechnicalResourceReservation],
    ) -> anyhow::Result<ReconciliationOutcome> {
        #[cfg(not(feature = "test-support"))]
        let _ = (request, provider_identity, provider_version);
        match self {
            Self::ExactOverwrite => {
                if provider_identity != crate::exact_overwrite::EXACT_OVERWRITE_PROVIDER_IDENTITY
                    || provider_version != crate::exact_overwrite::EXACT_OVERWRITE_PROVIDER_VERSION
                {
                    anyhow::bail!("journal identity is not supported by the fixed adapter");
                }
                crate::exact_overwrite::reconcile_exact_overwrite(
                    &request.reconciliation_key,
                    reservations,
                )
            }
            #[cfg(feature = "test-support")]
            Self::TestFakeProvider { fixture_root, .. } => {
                let _ = reservations;
                if provider_identity != "fake-provider" || provider_version != "v1" {
                    anyhow::bail!("journal identity is not supported by the fixed adapter");
                }
                reconcile_fake_provider(fixture_root, request).await
            }
        }
    }
}

#[cfg(feature = "test-support")]
async fn invoke_fake_provider(
    fixture_root: &std::path::Path,
    command: &carsinos_protocol::execass_recorder::ExecuteOnceV1,
    provider_coordination_root: Option<&std::path::Path>,
) -> anyhow::Result<InvocationOutcome> {
    use anyhow::Context;
    let binary = fixed_fake_provider_path().context("locating fixed fake-provider artifact")?;
    let bytes = std::fs::read(&binary).context("reading fixed fake-provider artifact")?;
    let artifact_digest = format!("sha256:{}", crate::hex_encode(&Sha256::digest(bytes)));
    if command.adapter_artifact_digest != artifact_digest {
        anyhow::bail!("fixed fake-provider artifact digest mismatch");
    }
    let mut process = tokio::process::Command::new(&binary);
    process
        .arg("invoke")
        .arg("--fixture-root")
        .arg(fixture_root)
        .arg("--attempt-id")
        .arg(&command.attempt_id)
        .arg("--idempotency-key")
        .arg(command.provider_idempotency_key.as_deref().unwrap_or(""))
        .arg("--reconciliation-key")
        .arg(command.reconciliation_key.as_deref().unwrap_or(""));
    if let Some(root) = provider_coordination_root {
        process.arg("--pause-after-ledger-fsync-root").arg(root);
    }
    let output = process
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .output()
        .await
        .context("launching fixed fake provider")?;
    if !output.status.success() {
        anyhow::bail!("fixed fake provider failed");
    }
    let response: serde_json::Value =
        serde_json::from_slice(&output.stdout).context("decoding fixed fake-provider response")?;
    let remote_effect_id = response
        .get("remote_effect_id")
        .and_then(|value| value.as_str())
        .context("fake-provider response omitted remote effect ID")?
        .to_owned();
    let evidence_payload = serde_json::json!({
        "attempt_id": command.attempt_id,
        "remote_effect_id": remote_effect_id,
        "technical_resource_actuals": [],
    });
    Ok(InvocationOutcome {
        kind: RecorderObservationKindV1::Present,
        response_digest: format!(
            "sha256:{}",
            crate::hex_encode(&Sha256::digest(&output.stdout))
        ),
        evidence_payload_digest: format!(
            "sha256:{}",
            crate::hex_encode(&Sha256::digest(serde_json::to_vec(&evidence_payload)?))
        ),
        remote_effect_id: Some(remote_effect_id),
        provider_error_class: None,
        technical_resource_actuals: Vec::new(),
    })
}

#[cfg(feature = "test-support")]
fn fixed_fake_provider_path() -> Option<PathBuf> {
    let binary_name = if cfg!(windows) {
        "ea213-fake-provider.exe"
    } else {
        "ea213-fake-provider"
    };
    let current = std::env::current_exe().ok()?;
    let parent = current.parent()?;
    let direct = parent.join(binary_name);
    if direct.is_file() {
        return Some(direct);
    }
    parent
        .file_name()
        .is_some_and(|name| name == "deps")
        .then(|| parent.parent().map(|root| root.join(binary_name)))
        .flatten()
        .filter(|path| path.is_file())
}

#[cfg(feature = "test-support")]
fn fixed_fake_provider_artifact_digest() -> Option<String> {
    let bytes = std::fs::read(fixed_fake_provider_path()?).ok()?;
    Some(format!(
        "sha256:{}",
        crate::hex_encode(&Sha256::digest(bytes))
    ))
}

#[cfg(feature = "test-support")]
async fn reconcile_fake_provider(
    fixture_root: &std::path::Path,
    request: &ReconcileV1,
) -> anyhow::Result<ReconciliationOutcome> {
    use anyhow::Context;
    let binary = fixed_fake_provider_path().context("locating fixed fake-provider artifact")?;
    let output = tokio::process::Command::new(binary)
        .arg("query")
        .arg("--fixture-root")
        .arg(fixture_root)
        .arg("--reconciliation-key")
        .arg(&request.reconciliation_key)
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .output()
        .await
        .context("querying fixed fake provider")?;
    if !output.status.success() {
        anyhow::bail!("fixed fake-provider query failed");
    }
    let response: serde_json::Value =
        serde_json::from_slice(&output.stdout).context("decoding fake-provider query response")?;
    let found = response
        .get("found")
        .and_then(serde_json::Value::as_bool)
        .context("fake-provider query omitted found")?;
    let remote_effect_id = response
        .get("remote_effect_id")
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned);
    if found != remote_effect_id.is_some() {
        anyhow::bail!("fake-provider returned internally inconsistent evidence");
    }
    let response_digest = format!(
        "sha256:{}",
        crate::hex_encode(&Sha256::digest(&output.stdout))
    );
    let evidence_payload = serde_json::json!({
        "attempt_id": request.attempt_id,
        "reconciliation_key_digest": request.reconciliation_key_digest,
        "found": found,
        "remote_effect_id": remote_effect_id,
        "technical_resource_actuals": [],
    });
    let evidence_payload_digest = format!(
        "sha256:{}",
        crate::hex_encode(&Sha256::digest(serde_json::to_vec(&evidence_payload)?))
    );
    Ok(ReconciliationOutcome {
        kind: if found {
            RecorderObservationKindV1::Present
        } else {
            RecorderObservationKindV1::Absent
        },
        response_digest,
        evidence_payload_digest,
        remote_effect_id,
        technical_resource_actuals: Vec::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use carsinos_protocol::execass_recorder::{
        ExecuteOnceV1, OpaqueOperandEnvelopeV1, RecorderBindingV1, RECORDER_PROTOCOL_VERSION,
    };

    fn command() -> ExecuteOnceV1 {
        let mut command = ExecuteOnceV1 {
            binding: RecorderBindingV1 {
                protocol_version: RECORDER_PROTOCOL_VERSION.into(),
                canonical_root_identity: "root".into(),
                installation_id: "installation".into(),
                state_root_generation: 1,
                os_user_identity_digest: "user".into(),
                runtime_host_generation: 1,
                runtime_host_instance_id: "host".into(),
                runtime_fencing_token: 1,
            },
            request_id: "request".into(),
            claim_event_id: "claim".into(),
            claim_receipt_id: "receipt".into(),
            continuation_fencing_token: 1,
            delegation_id: "delegation".into(),
            continuation_id: "continuation".into(),
            action_id: "action".into(),
            logical_effect_id: "effect".into(),
            internal_idempotency_key: "internal".into(),
            attempt_id: "attempt".into(),
            attempt_number: 1,
            provider_identity: "fake-provider".into(),
            provider_version: "v1".into(),
            adapter_identity: "ea213.fake-provider.v1".into(),
            adapter_artifact_digest: "artifact".into(),
            provider_request_digest: String::new(),
            provider_idempotency_key: Some("provider-key".into()),
            reconciliation_key: Some("reconcile-key".into()),
            manifest_digest: "manifest".into(),
            payload_digest: "payload".into(),
            operand_envelope: OpaqueOperandEnvelopeV1 {
                non_secret: serde_json::json!({"fixture": true}),
                secret_handles: vec![],
            },
            deadline_ms: i64::MAX,
            client_nonce: "nonce".into(),
            command_mac: String::new(),
        };
        command.provider_request_digest = command.derived_provider_request_digest().unwrap();
        command
    }

    #[test]
    fn exact_overwrite_executor_rejects_unrelated_caller_material() {
        assert!(!FixedExecutor::ExactOverwrite.supports(&command()));
    }

    #[cfg(feature = "test-support")]
    #[test]
    fn fake_identity_artifact_and_operands_are_exact() {
        let mut command = command();
        command.adapter_artifact_digest = fixed_fake_provider_artifact_digest().unwrap();
        let fake = FixedExecutor::TestFakeProvider {
            fixture_root: PathBuf::from("unused"),
            provider_coordination_root: None,
        };
        assert!(fake.supports(&command));
        type Mutation = Box<dyn Fn(&mut ExecuteOnceV1)>;
        let mutations: Vec<Mutation> = vec![
            Box::new(|value| value.adapter_identity.push_str("-wrong")),
            Box::new(|value| value.adapter_artifact_digest.push('0')),
            Box::new(|value| value.provider_version.push_str("-wrong")),
            Box::new(|value| value.operand_envelope.non_secret = serde_json::json!({})),
        ];
        for mutate in mutations {
            let mut changed = command.clone();
            mutate(&mut changed);
            assert!(!fake.supports(&changed));
        }
    }
}
