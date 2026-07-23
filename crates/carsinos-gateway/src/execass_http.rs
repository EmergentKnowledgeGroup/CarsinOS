//! Thin, versioned ExecAss HTTP adapters over the canonical storage/runtime authorities.

use super::{
    current_time_ms, require_bearer_auth_with_error, require_roles_with_audit, AppState,
    AuthContext, ROLE_OPERATOR_ADMIN, ROLE_OPERATOR_READONLY,
};
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use carsinos_protocol::execass as wire;
use carsinos_storage::execass as storage;
use serde::Deserialize;

type WireError = (StatusCode, Json<wire::ApiError>);
type WireResult<T> = std::result::Result<Json<T>, WireError>;

fn correlation_id(headers: &HeaderMap) -> String {
    headers
        .get("x-request-id")
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("execass-request")
        .to_owned()
}

fn summary_delivery_correlation_id(headers: &HeaderMap) -> String {
    headers
        .get("x-request-id")
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string())
}

fn error(
    headers: &HeaderMap,
    status: StatusCode,
    code: wire::ApiErrorCode,
    message: &str,
    retryable: bool,
) -> WireError {
    (
        status,
        Json(wire::ApiError {
            code,
            safe_human_message: message.to_owned(),
            retryable,
            correlation_id: correlation_id(headers),
            safe_for_display: true,
            exposes_sensitive_metadata: false,
        }),
    )
}

fn authenticate(
    headers: &HeaderMap,
    state: &AppState,
    _write: bool,
    action: &str,
    resource: &str,
) -> std::result::Result<AuthContext, WireError> {
    let auth = require_bearer_auth_with_error(headers, state).map_err(|(status, _)| {
        error(
            headers,
            status,
            wire::ApiErrorCode::AuthenticationRequired,
            "Authentication is required.",
            false,
        )
    })?;
    // ExecAss is a single-owner product. These legacy role labels only
    // authenticate the local transport; mutation authority comes from the
    // exact native-owner proof verified by each write coordinator.
    let roles = &[ROLE_OPERATOR_ADMIN, ROLE_OPERATOR_READONLY];
    require_roles_with_audit(headers, state, &auth, roles, action, resource).map_err(
        |(status, _)| {
            error(
                headers,
                status,
                wire::ApiErrorCode::AuthorityDenied,
                "The authenticated owner principal does not have this bounded capability.",
                false,
            )
        },
    )?;
    Ok(auth)
}

fn store<'a>(
    headers: &HeaderMap,
    state: &'a AppState,
) -> Result<&'a storage::ExecAssStore, WireError> {
    state.execass_store.as_deref().ok_or_else(|| {
        error(
            headers,
            StatusCode::SERVICE_UNAVAILABLE,
            wire::ApiErrorCode::ExternalDependency,
            "ExecAss runtime storage is unavailable.",
            true,
        )
    })
}

fn phase(value: storage::DelegationPhase) -> wire::DelegationPhase {
    match value {
        storage::DelegationPhase::Accepted => wire::DelegationPhase::Accepted,
        storage::DelegationPhase::Planning => wire::DelegationPhase::Planning,
        storage::DelegationPhase::InMotion => wire::DelegationPhase::InMotion,
        storage::DelegationPhase::WaitingForUser => wire::DelegationPhase::WaitingForUser,
        storage::DelegationPhase::WaitingExternal => wire::DelegationPhase::WaitingExternal,
        storage::DelegationPhase::Recovering => wire::DelegationPhase::Recovering,
        storage::DelegationPhase::Completed => wire::DelegationPhase::Completed,
        storage::DelegationPhase::PartiallyCompleted => wire::DelegationPhase::PartiallyCompleted,
        storage::DelegationPhase::Failed => wire::DelegationPhase::Failed,
    }
}

fn run_control(value: storage::RunControlState) -> wire::RunControlState {
    match value {
        storage::RunControlState::Running => wire::RunControlState::Running,
        storage::RunControlState::StopRequested => wire::RunControlState::StopRequested,
        storage::RunControlState::Stopped => wire::RunControlState::Stopped,
    }
}

fn outcome_summary(value: storage::DelegationPhase) -> String {
    match value {
        storage::DelegationPhase::Completed => "Completed with verifier-backed evidence.",
        storage::DelegationPhase::PartiallyCompleted => {
            "Partially completed; inspect unmet outcome evidence."
        }
        storage::DelegationPhase::Failed => "Failed; inspect recovery and receipt evidence.",
        storage::DelegationPhase::WaitingForUser => "Waiting for your reply or decision.",
        storage::DelegationPhase::WaitingExternal => "Waiting on an external dependency.",
        storage::DelegationPhase::Recovering => "Recovering within the existing owner authority.",
        _ => "Work is active.",
    }
    .to_owned()
}

fn delegation_summary(record: &storage::DelegationRecord) -> wire::DelegationSummary {
    wire::DelegationSummary {
        delegation_id: record.delegation_id.clone(),
        phase: phase(record.phase),
        run_control: run_control(record.run_control),
        state_revision: record.state_revision,
        intent_summary: record.normalized_original_intent.clone(),
        outcome_summary: outcome_summary(record.phase),
        policy_revision: record.policy_revision,
        pending_decision: None,
        pending_external_wait: record.external_wait_json.clone(),
        stop_epoch: record.stop_epoch,
        created_at_ms: record.created_at,
        updated_at_ms: record.updated_at,
        acknowledged_at_ms: record.acknowledged_at,
        terminal_at_ms: record.terminal_at,
        authoritative_deep_link: format!("/execass/delegations/{}", record.delegation_id),
    }
}

fn receipt(value: storage::ApiReceiptRead, delegation_id: &str) -> wire::ReceiptSummary {
    wire::ReceiptSummary {
        receipt_id: value.receipt_id,
        scope: wire::ReceiptScope::Delegation {
            delegation_id: delegation_id.to_owned(),
            delegation_sequence: value.delegation_sequence,
        },
        global_sequence: value.global_sequence,
        receipt_kind: value.receipt_kind,
        subject_kind: value.subject_kind,
        subject_id: value.subject_id,
        subject_revision: value.subject_revision,
        occurred_at_ms: value.occurred_at,
        committed_at_ms: value.committed_at,
        evidence_refs: value
            .evidence
            .into_iter()
            .map(|evidence| wire::ReceiptEvidenceSummary {
                authority_kind: evidence.authority_kind,
                source_id: evidence.source_id,
                authoritative_revision: evidence.authoritative_revision,
                authority_link_id: evidence.authority_link_id,
                observation_digest: evidence.observation_digest,
                deep_link: evidence.deep_link,
            })
            .collect(),
        receipt_digest: value.receipt_digest,
        delegation_previous_receipt_digest: value.previous_receipt_digest,
        global_previous_receipt_digest: value.global_previous_receipt_digest,
        key_id: value.key_id,
        key_generation: value.key_generation,
        integrity_tag: value.integrity_tag,
        previous_key_integrity_tag: value.previous_key_integrity_tag,
        safe_summary: value.safe_summary,
    }
}

pub(super) async fn get_summary(
    headers: HeaderMap,
    State(state): State<AppState>,
) -> WireResult<wire::SummaryResponse> {
    let auth = authenticate(
        &headers,
        &state,
        false,
        "execass.summary.read",
        "execass:summary",
    )?;
    let now = current_time_ms();
    let request_identity = format!(
        "{}:{}",
        auth.principal_id,
        summary_delivery_correlation_id(&headers)
    );
    let delivery_id = format!(
        "summary-delivery-{}",
        sha256_hex(request_identity.as_bytes())
    );
    let runtime = state.execass_confirmation_runtime.as_ref().ok_or_else(|| {
        error(
            &headers,
            StatusCode::SERVICE_UNAVAILABLE,
            wire::ApiErrorCode::ExternalDependency,
            "ExecAss summary integrity runtime is unavailable.",
            true,
        )
    })?;
    let (projection, outcome) = runtime
        .read_api_summary(
            &storage::ExecAssProjectionQuery::new(now),
            &storage::SummaryDeliveryMetadata {
                delivery_id,
                request_identity,
                delivered_at: now,
            },
        )
        .map_err(|_| {
            error(
                &headers,
                StatusCode::INTERNAL_SERVER_ERROR,
                wire::ApiErrorCode::ReceiptIntegrityQuarantined,
                "The authoritative summary could not be rendered safely.",
                true,
            )
        })?;
    let delivery = match outcome {
        storage::SummaryDeliveryOutcome::Recorded(value)
        | storage::SummaryDeliveryOutcome::Replayed(value) => value,
        storage::SummaryDeliveryOutcome::Conflict => {
            return Err(error(
                &headers,
                StatusCode::CONFLICT,
                wire::ApiErrorCode::IdempotencyConflict,
                "The summary request identity conflicts with a prior delivery.",
                false,
            ))
        }
    };
    Ok(Json(summary_response(projection, delivery).map_err(
        |_| {
            error(
                &headers,
                StatusCode::INTERNAL_SERVER_ERROR,
                wire::ApiErrorCode::InternalSafeFailure,
                "The authoritative summary contained an invalid attention binding.",
                false,
            )
        },
    )?))
}

pub(super) async fn post_summary_ack(
    headers: HeaderMap,
    State(state): State<AppState>,
    Json(request): Json<wire::SummaryAckRequest>,
) -> WireResult<wire::SummaryAckResponse> {
    authenticate(
        &headers,
        &state,
        true,
        "execass.summary.ack",
        "execass:summary",
    )?;
    require_idempotency(&headers, &request.idempotency_key)?;
    let items = request
        .displayed
        .delivered
        .iter()
        .map(|item| {
            let (namespace, _) = item.item_id.split_once(':').ok_or_else(|| {
                error(
                    &headers,
                    StatusCode::BAD_REQUEST,
                    wire::ApiErrorCode::InvalidRequest,
                    "A delivered summary item is not namespaced.",
                    false,
                )
            })?;
            let projection_kind =
                storage::SummaryProjectionKind::parse(namespace).ok_or_else(|| {
                    error(
                        &headers,
                        StatusCode::BAD_REQUEST,
                        wire::ApiErrorCode::InvalidRequest,
                        "A delivered summary item has an unsupported pane.",
                        false,
                    )
                })?;
            Ok(storage::SummaryDeliveredItem {
                item_id: item.item_id.clone(),
                revision: item.revision,
                projection_kind,
            })
        })
        .collect::<Result<Vec<_>, WireError>>()?;
    let runtime = state.execass_confirmation_runtime.as_ref().ok_or_else(|| {
        error(
            &headers,
            StatusCode::SERVICE_UNAVAILABLE,
            wire::ApiErrorCode::ExternalDependency,
            "ExecAss summary integrity runtime is unavailable.",
            true,
        )
    })?;
    let acknowledged_at_ms = current_time_ms();
    let outcome = runtime
        .acknowledge_api_summary(
            &request.displayed.cursor,
            &request.idempotency_key,
            acknowledged_at_ms,
            items,
        )
        .map_err(|_| {
            error(
                &headers,
                StatusCode::INTERNAL_SERVER_ERROR,
                wire::ApiErrorCode::InternalSafeFailure,
                "The displayed summary could not be acknowledged safely.",
                true,
            )
        })?;
    match outcome {
        storage::SummaryAcknowledgementOutcome::Acknowledged(_)
        | storage::SummaryAcknowledgementOutcome::Replayed(_) => {
            Ok(Json(wire::SummaryAckResponse {
                acknowledged: true,
                displayed: request.displayed,
                acknowledged_at_ms,
            }))
        }
        storage::SummaryAcknowledgementOutcome::Conflict => Err(error(
            &headers,
            StatusCode::CONFLICT,
            wire::ApiErrorCode::IdempotencyConflict,
            "The acknowledgement does not match the exact displayed summary.",
            false,
        )),
        storage::SummaryAcknowledgementOutcome::NotDelivered => Err(error(
            &headers,
            StatusCode::NOT_FOUND,
            wire::ApiErrorCode::NotFound,
            "The displayed summary cursor was not found.",
            false,
        )),
    }
}

fn require_idempotency(headers: &HeaderMap, expected: &str) -> Result<(), WireError> {
    let supplied = headers
        .get("idempotency-key")
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    if supplied != expected || expected.trim().is_empty() {
        return Err(error(
            headers,
            StatusCode::BAD_REQUEST,
            wire::ApiErrorCode::InvalidRequest,
            "Idempotency-Key must match the request body.",
            false,
        ));
    }
    Ok(())
}

fn local_owner_intake_proof(headers: &HeaderMap) -> Result<wire::LocalOwnerIntakeProof, WireError> {
    let encoded = headers
        .get("x-execass-owner-proof")
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| {
            error(
                headers,
                StatusCode::FORBIDDEN,
                wire::ApiErrorCode::AuthorityDenied,
                "Verified native-owner authority is required.",
                false,
            )
        })?;
    let bytes = URL_SAFE_NO_PAD.decode(encoded.as_bytes()).map_err(|_| {
        error(
            headers,
            StatusCode::FORBIDDEN,
            wire::ApiErrorCode::AuthorityDenied,
            "The native-owner proof is invalid.",
            false,
        )
    })?;
    serde_json::from_slice(&bytes).map_err(|_| {
        error(
            headers,
            StatusCode::FORBIDDEN,
            wire::ApiErrorCode::AuthorityDenied,
            "The native-owner proof is invalid.",
            false,
        )
    })
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LocalOwnerMutationAuthorization {
    binding: wire::LocalOwnerMutationBinding,
    proof: wire::LocalOwnerMutationProof,
}

fn local_owner_mutation_authorization(
    headers: &HeaderMap,
) -> Result<LocalOwnerMutationAuthorization, WireError> {
    let encoded = headers
        .get("x-execass-owner-proof")
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| {
            error(
                headers,
                StatusCode::FORBIDDEN,
                wire::ApiErrorCode::AuthorityDenied,
                "Verified native-owner authority is required.",
                false,
            )
        })?;
    let bytes = URL_SAFE_NO_PAD.decode(encoded.as_bytes()).map_err(|_| {
        error(
            headers,
            StatusCode::FORBIDDEN,
            wire::ApiErrorCode::AuthorityDenied,
            "The native-owner proof is invalid.",
            false,
        )
    })?;
    serde_json::from_slice(&bytes).map_err(|_| {
        error(
            headers,
            StatusCode::FORBIDDEN,
            wire::ApiErrorCode::AuthorityDenied,
            "The native-owner proof is invalid.",
            false,
        )
    })
}

pub(super) async fn post_intake(
    headers: HeaderMap,
    State(state): State<AppState>,
    Json(request): Json<wire::IntakeRequest>,
) -> WireResult<wire::IntakeResponse> {
    authenticate(
        &headers,
        &state,
        true,
        "execass.intake.submit",
        "execass:intake",
    )?;
    require_idempotency(&headers, &request.idempotency_key)?;
    let proof = local_owner_intake_proof(&headers)?;
    let actor = state
        .execass_actor_gate
        .verify_local_owner_intake(&proof, &request.text)
        .map_err(|_| {
            error(
                &headers,
                StatusCode::FORBIDDEN,
                wire::ApiErrorCode::AuthorityDenied,
                "The native-owner proof does not authorize this exact request.",
                false,
            )
        })?;
    let runtime = state.execass_confirmation_runtime.as_ref().ok_or_else(|| {
        error(
            &headers,
            StatusCode::SERVICE_UNAVAILABLE,
            wire::ApiErrorCode::ExternalDependency,
            "ExecAss confirmation authority is unavailable.",
            true,
        )
    })?;
    let policy_revision = runtime
        .read_global_control_status()
        .map_err(|_| {
            error(
                &headers,
                StatusCode::INTERNAL_SERVER_ERROR,
                wire::ApiErrorCode::InternalSafeFailure,
                "The active policy revision could not be read safely.",
                true,
            )
        })?
        .current_policy_revision;
    let immediate_response = builtin_conversational_response(&request.text);
    // Only exact, server-owned, no-tool exchanges are answered inline. Every
    // compound or unknown request still fails toward durable admission so
    // wording never becomes a permission or purpose classifier.
    let assessment = if immediate_response.is_some() {
        crate::execass_intake::ExecutionShapeAssessment::new(
            crate::execass_intake::ImmediateResponseShape::NonEmpty,
        )
    } else {
        crate::execass_intake::ExecutionShapeAssessment::new(
            crate::execass_intake::ImmediateResponseShape::Absent,
        )
        .with_ambiguity()
    };
    let outcome = crate::execass_intake::ExecAssIntakeService
        .route_verified_owner_intake(
            &state,
            &actor,
            &request,
            &assessment,
            policy_revision,
            current_time_ms(),
        )
        .await
        .map_err(|_| {
            error(
                &headers,
                StatusCode::INTERNAL_SERVER_ERROR,
                wire::ApiErrorCode::InternalSafeFailure,
                "The owner request could not be admitted safely.",
                true,
            )
        })?;
    intake_response(&headers, &state, outcome, immediate_response)
}

fn builtin_conversational_response(text: &str) -> Option<&'static str> {
    match text.trim().to_ascii_lowercase().as_str() {
        "hi" | "hello" | "hey" | "good morning" | "good afternoon" | "good evening" => {
            Some("Hello! What can I help you with?")
        }
        "thanks" | "thank you" => Some("You're welcome."),
        "who are you" | "what are you" => Some(
            "I'm ExecAss, your assistant in this CarsinOS instance. What would you like me to do?",
        ),
        _ => None,
    }
}

fn intake_response(
    headers: &HeaderMap,
    state: &AppState,
    outcome: crate::execass_intake::VerifiedOwnerIntakeOutcome,
    immediate_response: Option<&'static str>,
) -> WireResult<wire::IntakeResponse> {
    use crate::execass_intake::{FollowUpAmendmentWriteOutcome, VerifiedOwnerIntakeOutcome};
    use crate::GatewayFoundationAdmissionOutcome;
    let (delegation_id, created) = match outcome {
        VerifiedOwnerIntakeOutcome::Durable {
            admission:
                GatewayFoundationAdmissionOutcome::Admitted(
                    storage::FoundationDispatchAdmissionOutcome::Admitted(outcome),
                ),
            ..
        } => match *outcome {
            storage::FoundationWriteOutcome::Created(bundle) => {
                (bundle.delegation.delegation_id, true)
            }
            storage::FoundationWriteOutcome::Replayed(bundle) => {
                (bundle.delegation.delegation_id, false)
            }
            storage::FoundationWriteOutcome::Conflict { .. } => {
                return Err(error(
                    headers,
                    StatusCode::CONFLICT,
                    wire::ApiErrorCode::IdempotencyConflict,
                    "The idempotency key is already bound to different intake material.",
                    false,
                ));
            }
        },
        VerifiedOwnerIntakeOutcome::Amendment { outcome, .. } => match outcome {
            FollowUpAmendmentWriteOutcome::Applied { delegation_id } => (delegation_id, true),
            FollowUpAmendmentWriteOutcome::Replayed { delegation_id } => (delegation_id, false),
        },
        VerifiedOwnerIntakeOutcome::WrongAttachment { .. } => {
            return Err(error(
                headers,
                StatusCode::CONFLICT,
                wire::ApiErrorCode::RevisionConflict,
                "The requested delegation attachment is missing or has changed.",
                false,
            ));
        }
        VerifiedOwnerIntakeOutcome::Durable {
            admission:
                GatewayFoundationAdmissionOutcome::Admitted(
                    storage::FoundationDispatchAdmissionOutcome::DangerConfirmationRequired,
                ),
            ..
        } => {
            return Err(error(
                headers,
                StatusCode::CONFLICT,
                wire::ApiErrorCode::DecisionAssuranceRequired,
                "This exact dangerous action requires one owner confirmation before execution.",
                false,
            ));
        }
        VerifiedOwnerIntakeOutcome::Durable { .. } => {
            return Err(error(
                headers,
                StatusCode::CONFLICT,
                wire::ApiErrorCode::InvalidRequest,
                "The request needs a concrete mechanical clarification before work can begin.",
                false,
            ));
        }
        VerifiedOwnerIntakeOutcome::Conversational(_) => {
            let response_text = immediate_response.ok_or_else(|| {
                error(
                    headers,
                    StatusCode::INTERNAL_SERVER_ERROR,
                    wire::ApiErrorCode::InternalSafeFailure,
                    "The immediate conversational response was unavailable.",
                    true,
                )
            })?;
            return Ok(Json(wire::IntakeResponse::Conversational {
                response_text: response_text.to_owned(),
                request_audit_ref: format!("security-audit:{}", correlation_id(headers)),
            }));
        }
        VerifiedOwnerIntakeOutcome::SynchronousReadOnly(_) => {
            return Err(error(
                headers,
                StatusCode::CONFLICT,
                wire::ApiErrorCode::InvalidRequest,
                "This transport cannot safely render an immediate conversational result.",
                false,
            ));
        }
    };
    let detail = store(headers, state)?
        .read_api_delegation_detail(&delegation_id)
        .map_err(|_| {
            error(
                headers,
                StatusCode::INTERNAL_SERVER_ERROR,
                wire::ApiErrorCode::InternalSafeFailure,
                "The admitted delegation could not be projected safely.",
                true,
            )
        })?
        .ok_or_else(|| {
            error(
                headers,
                StatusCode::INTERNAL_SERVER_ERROR,
                wire::ApiErrorCode::InternalSafeFailure,
                "The admitted delegation is unavailable.",
                true,
            )
        })?;
    Ok(Json(wire::IntakeResponse::Delegation {
        delegation: Box::new(delegation_summary(&detail.delegation)),
        created,
    }))
}

fn summary_response(
    projection: storage::ExecAssExecutiveProjection,
    delivery: storage::SummaryDeliveryRecord,
) -> std::result::Result<wire::SummaryResponse, &'static str> {
    let observed_at = projection.observed_at_ms;
    let needs_you = projection
        .needs_you
        .into_iter()
        .map(|item| {
            let (subject, authoritative_deep_link) = match item.subject {
                storage::AttentionProjectionSubject::Delegation {
                    delegation_id,
                    delegation_revision,
                } => {
                    let link = item
                        .decision_id
                        .as_ref()
                        .map(|id| format!("/execass/decisions/{id}"))
                        .unwrap_or_else(|| format!("/execass/delegations/{delegation_id}"));
                    (
                        wire::AttentionSubject::Delegation {
                            delegation_id,
                            delegation_revision,
                        },
                        link,
                    )
                }
                storage::AttentionProjectionSubject::RuntimeHost {
                    generation,
                    host_instance_id,
                    fencing_token,
                } => {
                    let evidence = item
                        .runtime_recovery
                        .as_ref()
                        .ok_or("runtime-host attention is missing recovery evidence")?;
                    if evidence.predecessor_generation != generation
                        || evidence.predecessor_host_instance_id != host_instance_id
                        || evidence.predecessor_fencing_token != fencing_token
                    {
                        return Err("runtime-host attention subject/evidence mismatch");
                    }
                    (
                        wire::AttentionSubject::RuntimeHost {
                            runtime_host_generation: generation,
                            runtime_host_instance_id: host_instance_id,
                            runtime_fencing_token: fencing_token,
                            runtime_actual_state: runtime_actual_state(
                                evidence.predecessor_actual_state,
                            ),
                            runtime_end_reason: evidence.predecessor_end_reason.clone(),
                            active_work_binding_digest: evidence.active_work_binding_digest.clone(),
                        },
                        format!(
                            "/execass/runtime-host?generation={generation}&receipt_id={}",
                            evidence.receipt_id
                        ),
                    )
                }
            };
            Ok(wire::AttentionItem {
                attention_id: item.attention_id,
                kind: match item.kind {
                    storage::NeedsYouKind::Confirmation => wire::AttentionKind::Confirmation,
                    storage::NeedsYouKind::Clarification => wire::AttentionKind::Clarification,
                    storage::NeedsYouKind::Reply => wire::AttentionKind::Reply,
                    storage::NeedsYouKind::RecoveryChoice => wire::AttentionKind::RecoveryChoice,
                    storage::NeedsYouKind::RuntimePaused => wire::AttentionKind::RuntimePaused,
                },
                decision_kind: item.decision_kind.map(projection_decision_kind),
                subject,
                decision_id: item.decision_id,
                reason: item.reason,
                recommendation: item.recommendation,
                alternatives_or_actions: item.alternatives,
                assurance_required: if item.required_assurance.contains("owner") {
                    wire::AssuranceRequirement::VerifiedOwnerResolution
                } else {
                    wire::AssuranceRequirement::MechanicalResolution
                },
                deadline_reminder_state: match item.deadline_ms {
                    Some(deadline) if deadline <= observed_at => "due".into(),
                    Some(_) => "pending".into(),
                    None => "not_scheduled".into(),
                },
                deadline_at_ms: item.deadline_ms,
                decision_revision: item.decision_revision,
                authoritative_deep_link,
            })
        })
        .collect::<std::result::Result<Vec<_>, &'static str>>()?;
    Ok(wire::SummaryResponse {
        needs_you,
        in_motion: projection
            .in_motion
            .into_iter()
            .map(|item| wire::DelegationSummary {
                delegation_id: item.delegation_id.clone(),
                phase: projection_phase(item.underlying_phase),
                run_control: match item.state {
                    storage::InMotionState::Draining => wire::RunControlState::StopRequested,
                    storage::InMotionState::Stopped => wire::RunControlState::Stopped,
                    _ => wire::RunControlState::Running,
                },
                state_revision: item.delegation_revision,
                intent_summary: format!("Delegation {}", item.delegation_id),
                outcome_summary: match item.state {
                    storage::InMotionState::Active => "Work is active.",
                    storage::InMotionState::Recovering => "Recovering within existing authority.",
                    storage::InMotionState::WaitingExternal => "Waiting on an external dependency.",
                    storage::InMotionState::Draining => "Stopping at the declared safe boundary.",
                    storage::InMotionState::Stopped => "Stopped.",
                }
                .into(),
                policy_revision: item.policy_revision,
                pending_decision: None,
                pending_external_wait: item.external_wait_json,
                stop_epoch: item.stop_epoch,
                created_at_ms: item.created_at_ms,
                updated_at_ms: item.updated_at_ms,
                acknowledged_at_ms: item.acknowledged_at_ms,
                terminal_at_ms: None,
                authoritative_deep_link: format!("/execass/delegations/{}", item.delegation_id),
            })
            .collect(),
        done: projection
            .done_since_you_checked
            .into_iter()
            .map(|item| wire::DelegationSummary {
                delegation_id: item.delegation_id.clone(),
                phase: match item.outcome {
                    storage::DoneOutcome::Completed => wire::DelegationPhase::Completed,
                    storage::DoneOutcome::PartiallyCompleted => {
                        wire::DelegationPhase::PartiallyCompleted
                    }
                    storage::DoneOutcome::Failed => wire::DelegationPhase::Failed,
                },
                run_control: match item.run_control.as_str() {
                    "stop_requested" => wire::RunControlState::StopRequested,
                    "stopped" => wire::RunControlState::Stopped,
                    _ => wire::RunControlState::Running,
                },
                state_revision: item.delegation_revision,
                intent_summary: format!("Delegation {}", item.delegation_id),
                outcome_summary: match item.outcome {
                    storage::DoneOutcome::Completed => "Completed with verifier-backed evidence.",
                    storage::DoneOutcome::PartiallyCompleted => {
                        "Partially completed; inspect unmet outcome evidence."
                    }
                    storage::DoneOutcome::Failed => "Failed; inspect terminal evidence.",
                }
                .into(),
                policy_revision: item.policy_revision,
                pending_decision: None,
                pending_external_wait: None,
                stop_epoch: item.stop_epoch,
                created_at_ms: item.created_at_ms,
                updated_at_ms: item.terminal_at_ms,
                acknowledged_at_ms: item.acknowledged_at_ms,
                terminal_at_ms: Some(item.terminal_at_ms),
                authoritative_deep_link: format!("/execass/delegations/{}", item.delegation_id),
            })
            .collect(),
        next: projection
            .next
            .into_iter()
            .map(|item| wire::NextItem {
                next_item_id: item.item_id,
                kind: match item.kind {
                    storage::NextKind::RoutineOccurrence => wire::NextItemKind::Routine,
                    storage::NextKind::RecoveryReevaluation => wire::NextItemKind::FollowUp,
                    storage::NextKind::DangerousConfirmationExpiry => wire::NextItemKind::Deadline,
                },
                delegation_id: item.delegation_id,
                due_at_ms: Some(item.due_at_ms),
                scheduled_for_ms: matches!(item.kind, storage::NextKind::RoutineOccurrence)
                    .then_some(item.due_at_ms),
                summary: match item.kind {
                    storage::NextKind::RoutineOccurrence => "Scheduled routine occurrence.",
                    storage::NextKind::RecoveryReevaluation => "Recovery reevaluation is due.",
                    storage::NextKind::DangerousConfirmationExpiry => {
                        "Dangerous-action confirmation expires at this time."
                    }
                }
                .into(),
                authoritative_deep_link: item.deep_link.target_id,
            })
            .collect(),
        receipts: projection
            .receipts
            .items
            .into_iter()
            .map(projection_receipt)
            .collect(),
        displayed: wire::SummaryCursor {
            cursor: delivery.displayed_cursor,
            displayed_at_ms: delivery.delivered_at,
            delivered: delivery
                .items
                .into_iter()
                .map(|item| wire::DeliveredItem {
                    item_id: item.item_id,
                    revision: item.revision,
                })
                .collect(),
        },
    })
}

fn projection_receipt(value: storage::ReceiptProjectionItem) -> wire::ReceiptSummary {
    let scope = match (value.delegation_id, value.delegation_sequence) {
        (Some(delegation_id), Some(delegation_sequence)) => wire::ReceiptScope::Delegation {
            delegation_id,
            delegation_sequence,
        },
        (None, None) => wire::ReceiptScope::RuntimeHost {
            runtime_host_aggregate_id: "execass-runtime-host".into(),
        },
        _ => unreachable!("canonical receipt projection has a mixed receipt scope"),
    };
    wire::ReceiptSummary {
        receipt_id: value.receipt_id,
        scope,
        global_sequence: value.global_sequence,
        receipt_kind: serialized_enum(&value.receipt_kind),
        subject_kind: serialized_enum(&value.subject_kind),
        subject_id: value.subject_id,
        subject_revision: value.subject_revision,
        occurred_at_ms: value.occurred_at_ms,
        committed_at_ms: value.committed_at_ms,
        evidence_refs: value
            .evidence
            .into_iter()
            .map(|evidence| wire::ReceiptEvidenceSummary {
                authority_kind: serialized_enum(&evidence.authority_kind),
                source_id: evidence.source_id,
                authoritative_revision: evidence.authoritative_revision,
                authority_link_id: evidence.authority_link_id,
                observation_digest: evidence.observation_digest,
                deep_link: evidence.deep_link.target_id,
            })
            .collect(),
        receipt_digest: value.receipt_digest,
        delegation_previous_receipt_digest: value.delegation_previous_receipt_digest,
        global_previous_receipt_digest: value.global_previous_receipt_digest,
        key_id: value.key_id,
        key_generation: value.key_generation,
        integrity_tag: value.integrity_tag,
        previous_key_integrity_tag: value.previous_key_integrity_tag,
        safe_summary: value.redacted_summary,
    }
}

fn serialized_enum<T: serde::Serialize>(value: &T) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned))
        .unwrap_or_else(|| "unknown".into())
}

fn projection_phase(value: storage::ProjectionDelegationPhase) -> wire::DelegationPhase {
    match value {
        storage::ProjectionDelegationPhase::Accepted => wire::DelegationPhase::Accepted,
        storage::ProjectionDelegationPhase::Planning => wire::DelegationPhase::Planning,
        storage::ProjectionDelegationPhase::InMotion => wire::DelegationPhase::InMotion,
        storage::ProjectionDelegationPhase::WaitingForUser => wire::DelegationPhase::WaitingForUser,
        storage::ProjectionDelegationPhase::WaitingExternal => {
            wire::DelegationPhase::WaitingExternal
        }
        storage::ProjectionDelegationPhase::Recovering => wire::DelegationPhase::Recovering,
        storage::ProjectionDelegationPhase::Completed => wire::DelegationPhase::Completed,
        storage::ProjectionDelegationPhase::PartiallyCompleted => {
            wire::DelegationPhase::PartiallyCompleted
        }
        storage::ProjectionDelegationPhase::Failed => wire::DelegationPhase::Failed,
    }
}

fn projection_decision_kind(value: storage::ProjectionDecisionKind) -> wire::DecisionKind {
    match value {
        storage::ProjectionDecisionKind::Clarification => wire::DecisionKind::Clarification,
        storage::ProjectionDecisionKind::DangerousActionConfirmation => {
            wire::DecisionKind::DangerousActionConfirmation
        }
        storage::ProjectionDecisionKind::OwnerConfiguredCheckpoint => {
            wire::DecisionKind::OwnerConfiguredCheckpoint
        }
        storage::ProjectionDecisionKind::RecoveryChoice => wire::DecisionKind::RecoveryChoice,
        storage::ProjectionDecisionKind::DuplicateRiskRetry => {
            wire::DecisionKind::DuplicateRiskRetry
        }
        storage::ProjectionDecisionKind::Stop => wire::DecisionKind::Stop,
        storage::ProjectionDecisionKind::PolicyChange => wire::DecisionKind::PolicyChange,
    }
}

fn runtime_actual_state(value: storage::RuntimeActualState) -> wire::RuntimeHostActualState {
    match value {
        storage::RuntimeActualState::Stopped => wire::RuntimeHostActualState::Stopped,
        storage::RuntimeActualState::Starting => wire::RuntimeHostActualState::Starting,
        storage::RuntimeActualState::RunningAppBound => {
            wire::RuntimeHostActualState::RunningAppBound
        }
        storage::RuntimeActualState::Handoff => wire::RuntimeHostActualState::Handoff,
        storage::RuntimeActualState::RunningBackground => {
            wire::RuntimeHostActualState::RunningBackground
        }
        storage::RuntimeActualState::Draining => wire::RuntimeHostActualState::Draining,
        storage::RuntimeActualState::Faulted => wire::RuntimeHostActualState::Faulted,
    }
}

pub(super) async fn list_delegations(
    headers: HeaderMap,
    State(state): State<AppState>,
    Query(query): Query<wire::DelegationListQuery>,
) -> WireResult<wire::DelegationListResponse> {
    authenticate(
        &headers,
        &state,
        false,
        "execass.delegations.list",
        "execass:delegations",
    )?;
    let limit = query.limit.unwrap_or(50);
    let limit = u16::try_from(limit)
        .ok()
        .filter(|value| *value > 0 && *value <= 100)
        .ok_or_else(|| {
            error(
                &headers,
                StatusCode::BAD_REQUEST,
                wire::ApiErrorCode::InvalidRequest,
                "Delegation list limit must be between 1 and 100.",
                false,
            )
        })?;
    let phase_filter = query.phase.map(storage_phase);
    let run_filter = query.run_control.map(storage_run_control);
    let page = store(&headers, &state)?
        .list_api_delegations(
            &storage::ApiDelegationListQuery {
                phase: phase_filter,
                run_control: run_filter,
                limit,
                cursor: query.cursor,
            },
            &cursor_key(&state),
        )
        .map_err(|_| {
            error(
                &headers,
                StatusCode::BAD_REQUEST,
                wire::ApiErrorCode::InvalidRequest,
                "The delegation cursor or filters are invalid.",
                false,
            )
        })?;
    let mut items = Vec::with_capacity(page.entries.len());
    for entry in page.entries {
        let detail = store(&headers, &state)?
            .read_api_delegation_detail(&entry.delegation_id)
            .map_err(|_| {
                error(
                    &headers,
                    StatusCode::INTERNAL_SERVER_ERROR,
                    wire::ApiErrorCode::InternalSafeFailure,
                    "The delegation summary could not be read safely.",
                    true,
                )
            })?
            .ok_or_else(|| {
                error(
                    &headers,
                    StatusCode::CONFLICT,
                    wire::ApiErrorCode::RevisionConflict,
                    "A delegation changed while its page was rendered.",
                    true,
                )
            })?;
        let mut summary = delegation_summary(&detail.delegation);
        if let Some(decision_id) = &detail.delegation.pending_decision_id {
            summary.pending_decision = store(&headers, &state)?
                .read_api_current_decision(decision_id)
                .map_err(|_| {
                    error(
                        &headers,
                        StatusCode::INTERNAL_SERVER_ERROR,
                        wire::ApiErrorCode::InternalSafeFailure,
                        "The pending decision could not be read safely.",
                        true,
                    )
                })?
                .map(|value| {
                    decision_summary(value, &detail.delegation.normalized_original_intent)
                });
        }
        items.push(summary);
    }
    Ok(Json(wire::DelegationListResponse {
        items,
        next_cursor: page.next_cursor,
    }))
}

fn storage_phase(value: wire::DelegationPhase) -> storage::DelegationPhase {
    match value {
        wire::DelegationPhase::Accepted => storage::DelegationPhase::Accepted,
        wire::DelegationPhase::Planning => storage::DelegationPhase::Planning,
        wire::DelegationPhase::InMotion => storage::DelegationPhase::InMotion,
        wire::DelegationPhase::WaitingForUser => storage::DelegationPhase::WaitingForUser,
        wire::DelegationPhase::WaitingExternal => storage::DelegationPhase::WaitingExternal,
        wire::DelegationPhase::Recovering => storage::DelegationPhase::Recovering,
        wire::DelegationPhase::Completed => storage::DelegationPhase::Completed,
        wire::DelegationPhase::PartiallyCompleted => storage::DelegationPhase::PartiallyCompleted,
        wire::DelegationPhase::Failed => storage::DelegationPhase::Failed,
    }
}

fn storage_run_control(value: wire::RunControlState) -> storage::RunControlState {
    match value {
        wire::RunControlState::Running => storage::RunControlState::Running,
        wire::RunControlState::StopRequested => storage::RunControlState::StopRequested,
        wire::RunControlState::Stopped => storage::RunControlState::Stopped,
    }
}

pub(super) async fn get_delegation_receipts(
    Path(delegation_id): Path<String>,
    headers: HeaderMap,
    State(state): State<AppState>,
) -> WireResult<wire::DelegationReceiptsResponse> {
    authenticate(
        &headers,
        &state,
        false,
        "execass.delegation.receipts",
        &format!("execass:delegation:{delegation_id}"),
    )?;
    let page = store(&headers, &state)?
        .read_api_delegation_receipts(&delegation_id)
        .map_err(|_| {
            error(
                &headers,
                StatusCode::INTERNAL_SERVER_ERROR,
                wire::ApiErrorCode::ReceiptIntegrityQuarantined,
                "The receipt chain could not be verified.",
                false,
            )
        })?
        .ok_or_else(|| {
            error(
                &headers,
                StatusCode::NOT_FOUND,
                wire::ApiErrorCode::NotFound,
                "The delegation was not found.",
                false,
            )
        })?;
    Ok(Json(wire::DelegationReceiptsResponse {
        delegation_id: delegation_id.clone(),
        receipts: page
            .receipts
            .into_iter()
            .map(|value| receipt(value, &delegation_id))
            .collect(),
        receipt_chain_head: page.chain_head,
    }))
}

pub(super) async fn get_delegation(
    Path(delegation_id): Path<String>,
    headers: HeaderMap,
    State(state): State<AppState>,
) -> WireResult<wire::DelegationDetailResponse> {
    authenticate(
        &headers,
        &state,
        false,
        "execass.delegation.read",
        &format!("execass:delegation:{delegation_id}"),
    )?;
    let detail = store(&headers, &state)?
        .read_api_delegation_detail(&delegation_id)
        .map_err(|_| {
            error(
                &headers,
                StatusCode::INTERNAL_SERVER_ERROR,
                wire::ApiErrorCode::InternalSafeFailure,
                "The delegation could not be read safely.",
                true,
            )
        })?
        .ok_or_else(|| {
            error(
                &headers,
                StatusCode::NOT_FOUND,
                wire::ApiErrorCode::NotFound,
                "The delegation was not found.",
                false,
            )
        })?;
    let plan_summary = detail
        .current_plan
        .as_ref()
        .map(|value| value.plan_summary.clone())
        .unwrap_or_else(|| "No plan has been committed yet.".into());
    let outcome_criteria = detail
        .criteria
        .iter()
        .map(|criterion| wire::OutcomeCriterionSummary {
            criterion_id: criterion.criterion_id.clone(),
            material: criterion.material,
            expected_predicate: criterion.expected_predicate_json.clone(),
            verifier_type: verifier_type(criterion.verifier_type).to_owned(),
            verifier_result: wire::VerifierResult::Unknown,
            authoritative_evidence_ref: criterion.authoritative_source_kind.clone(),
        })
        .collect();
    let manifest_revision = detail
        .current_plan
        .as_ref()
        .map(|value| value.plan_revision)
        .unwrap_or(0);
    let manifest_digest = detail
        .current_plan
        .as_ref()
        .map(|value| value.manifest_digest.clone())
        .unwrap_or_default();
    let actions = detail
        .actions
        .iter()
        .map(|action| wire::ActionSummary {
            action_id: action.action_id.clone(),
            branch_state: branch_state(&action.status),
            manifest_revision,
            manifest_digest: manifest_digest.clone(),
            required_decision_kind: None,
            requires_assurance: wire::AssuranceRequirement::MechanicalResolution,
            danger_assessments: Vec::new(),
            technical_resources: Vec::new(),
            safe_boundary_description: action.safe_summary.clone(),
        })
        .collect();
    let continuations = detail
        .continuations
        .iter()
        .map(|continuation| wire::ContinuationSummary {
            continuation_id: continuation.continuation_id.clone(),
            delegation_id: continuation.delegation_id.clone(),
            status: continuation_status(continuation.status),
            plan_revision: continuation.target_plan_revision,
            policy_revision: detail.delegation.policy_revision,
            scheduled_for_ms: None,
            claimed_at_ms: continuation
                .lease_owner
                .as_ref()
                .map(|_| continuation.updated_at),
            completed_at_ms: continuation.completed_at,
            safe_summary: format!(
                "{} continuation",
                match continuation.branch_kind {
                    storage::ActionBranchKind::Ordinary => "ordinary",
                    storage::ActionBranchKind::Recovery => "recovery",
                }
            ),
        })
        .collect();
    let effects = detail
        .effects
        .iter()
        .map(|effect| {
            let action_id = detail
                .continuations
                .iter()
                .find(|value| value.continuation_id == effect.continuation_id)
                .map(|value| value.action_id.clone())
                .unwrap_or_else(|| "unknown-action".into());
            wire::EffectSummary {
                effect_id: effect.logical_effect_id.clone(),
                action_id,
                status: effect_status(effect.state),
                provider_idempotency_key: None,
                external_reference: effect.provider_identity.clone(),
                occurred_at_ms: Some(effect.updated_at),
                safe_summary: format!("Effect state: {:?}.", effect.state).to_ascii_lowercase(),
            }
        })
        .collect();
    let completion_verifiers = detail
        .verifiers
        .iter()
        .map(|verifier| {
            let criterion = detail
                .criteria
                .iter()
                .find(|value| value.criterion_id == verifier.criterion_id);
            wire::VerifierSummary {
                verifier_id: verifier.verifier_result_id.clone(),
                verifier_type: criterion
                    .map(|value| verifier_type(value.verifier_type))
                    .unwrap_or("unknown")
                    .to_owned(),
                criterion_id: verifier.criterion_id.clone(),
                result: verifier_result(&verifier.result),
                authoritative_evidence_ref: verifier.evidence_digest.clone(),
                assessed_at_ms: verifier.verified_at,
                safe_summary: format!("Verifier result revision {}.", verifier.result_revision),
            }
        })
        .collect();
    let recovery = detail.recovery.last().map(|value| wire::RecoverySummary {
        recovery_id: value.recovery_evaluation_id.clone(),
        action_id: value.logical_effect_id.clone(),
        objective_retry_safety_facts: Vec::new(),
        outcome_unknown: value.directive.contains("unknown"),
        automatic_retry_permitted: value.directive.contains("retry"),
        safe_summary: format!(
            "Recovery directive revision {}: {}.",
            value.evaluation_revision, value.directive
        ),
    });
    let mut internal_record_refs = Vec::new();
    if let Some(plan) = &detail.current_plan {
        internal_record_refs.push(format!("plan:{}", plan.plan_id));
    }
    internal_record_refs.extend(
        detail
            .criteria
            .iter()
            .map(|value| format!("criterion:{}", value.criterion_id)),
    );
    internal_record_refs.extend(
        detail
            .actions
            .iter()
            .map(|value| format!("action:{}", value.action_id)),
    );
    let mut summary = delegation_summary(&detail.delegation);
    if let Some(decision_id) = &detail.delegation.pending_decision_id {
        summary.pending_decision = store(&headers, &state)?
            .read_api_current_decision(decision_id)
            .map_err(|_| {
                error(
                    &headers,
                    StatusCode::INTERNAL_SERVER_ERROR,
                    wire::ApiErrorCode::InternalSafeFailure,
                    "The pending decision could not be read safely.",
                    true,
                )
            })?
            .map(|value| decision_summary(value, &detail.delegation.normalized_original_intent));
    }
    Ok(Json(wire::DelegationDetailResponse {
        detail: wire::DelegationDetail {
            delegation: summary,
            original_intent: detail.delegation.normalized_original_intent.clone(),
            immutable_intake_evidence_ref: format!(
                "sha256:{}",
                sha256_hex(detail.delegation.intake_evidence_json.as_bytes())
            ),
            ingress_source: detail.delegation.ingress_source.clone(),
            source_correlation_id: detail.delegation.source_correlation_id.clone(),
            plan_summary,
            outcome_criteria,
            authority_snapshot_ref: detail.delegation.authority_provenance_id.clone(),
            technical_resource_summary:
                "See action and receipt evidence for bounded technical resources.".into(),
            internal_record_refs,
            actions,
            continuations,
            effects,
            recovery,
            completion_verifiers,
            receipt_chain_head: detail.receipt_chain_head,
        },
    }))
}

pub(super) async fn post_resolve_decision(
    Path(decision_id): Path<String>,
    headers: HeaderMap,
    State(state): State<AppState>,
    Json(request): Json<wire::ResolveDecisionRequest>,
) -> WireResult<wire::ResolveDecisionResponse> {
    authenticate(
        &headers,
        &state,
        true,
        "execass.decision.resolve",
        &format!("execass:decision:{decision_id}"),
    )?;
    require_idempotency(&headers, &request.idempotency_key)?;
    if request.local_proof_binding.decision_id != decision_id
        || request.local_proof_binding.decision_revision
            != u64::try_from(request.decision_revision).unwrap_or_default()
        || request.local_proof_binding.decision_result != request.result
        || request.local_proof_binding.idempotency_key != request.idempotency_key
        || request.local_proof_binding.revision_text_digest
            != optional_text_digest(request.revision_text.as_deref())
        || request.local_proof_binding.challenge_response_digest
            != optional_text_digest(request.challenge_response.as_deref())
    {
        return Err(error(
            &headers,
            StatusCode::BAD_REQUEST,
            wire::ApiErrorCode::InvalidRequest,
            "The decision request does not match its exact signed binding.",
            false,
        ));
    }
    let selected = store(&headers, &state)?
        .read_decision_resolution_binding(
            &decision_id,
            &request
                .local_proof_binding
                .response_selected_logical_action_id,
        )
        .map_err(|_| {
            error(
                &headers,
                StatusCode::CONFLICT,
                wire::ApiErrorCode::DecisionChallengeExpired,
                "The dangerous-action confirmation is no longer current.",
                false,
            )
        })?
        .ok_or_else(|| {
            error(
                &headers,
                StatusCode::NOT_FOUND,
                wire::ApiErrorCode::NotFound,
                "The current decision or selected action was not found.",
                false,
            )
        })?;
    let current = carsinos_core::execass_actor::CurrentDecisionBinding {
        decision_id: selected.decision_id.clone(),
        decision_revision: u64::try_from(selected.decision_revision).map_err(|_| {
            error(
                &headers,
                StatusCode::INTERNAL_SERVER_ERROR,
                wire::ApiErrorCode::InternalSafeFailure,
                "The current decision revision is invalid.",
                false,
            )
        })?,
        normalized_intent_digest: carsinos_core::execass_actor::owner_normalized_intent_digest(
            &selected.normalized_intent,
        )
        .ok_or_else(|| {
            error(
                &headers,
                StatusCode::INTERNAL_SERVER_ERROR,
                wire::ApiErrorCode::InternalSafeFailure,
                "The current decision intent is invalid.",
                false,
            )
        })?,
        policy_revision: selected.policy_revision,
        canonical_manifest_digest: selected.manifest_digest.clone(),
        selected_logical_action_id: selected.selected_logical_action_id.clone(),
        presented_action_digest: selected.exact_selected_action_digest.clone(),
        declared_consequence_digest: selected.declared_consequence_digest.clone(),
        challenge_digest: selected.challenge_nonce_digest.clone(),
        expires_at_ms: selected.expires_at,
    };
    let response = carsinos_core::execass_actor::DecisionResponseEvidence {
        decision_id: current.decision_id.clone(),
        decision_revision: current.decision_revision,
        normalized_intent_digest: current.normalized_intent_digest.clone(),
        policy_revision: current.policy_revision,
        canonical_manifest_digest: current.canonical_manifest_digest.clone(),
        selected_logical_action_id: request
            .local_proof_binding
            .response_selected_logical_action_id
            .clone(),
        presented_action_digest: current.presented_action_digest.clone(),
        declared_consequence_digest: current.declared_consequence_digest.clone(),
        challenge_digest: current.challenge_digest.clone(),
        decision_result: request.result,
        observed_at_ms: request.local_proof_binding.observed_at_ms,
        request_correlation_id: request.local_proof.request_correlation_id.clone(),
        source_message_id: None,
        callback_fresh: true,
    };
    let event = state
        .execass_actor_gate
        .verify_local_decision_with_binding(
            &request.local_proof,
            &current,
            &response,
            &request.local_proof_binding,
        )
        .map_err(|_| {
            error(
                &headers,
                StatusCode::FORBIDDEN,
                wire::ApiErrorCode::AuthorityDenied,
                "The native-owner proof does not authorize this exact decision response.",
                false,
            )
        })?;
    let runtime = state.execass_confirmation_runtime.as_ref().ok_or_else(|| {
        error(
            &headers,
            StatusCode::SERVICE_UNAVAILABLE,
            wire::ApiErrorCode::ExternalDependency,
            "ExecAss confirmation authority is unavailable.",
            true,
        )
    })?;
    runtime
        .resolve_typed(
            &event,
            &decision_id,
            &request
                .local_proof_binding
                .response_selected_logical_action_id,
        )
        .map_err(|_| {
            error(
                &headers,
                StatusCode::CONFLICT,
                wire::ApiErrorCode::RevisionConflict,
                "The decision changed before the exact response was committed.",
                true,
            )
        })?;
    let detail = store(&headers, &state)?
        .read_api_delegation_detail(&selected.delegation_id)
        .map_err(|_| {
            error(
                &headers,
                StatusCode::INTERNAL_SERVER_ERROR,
                wire::ApiErrorCode::InternalSafeFailure,
                "The resolved delegation could not be projected safely.",
                true,
            )
        })?
        .ok_or_else(|| {
            error(
                &headers,
                StatusCode::NOT_FOUND,
                wire::ApiErrorCode::NotFound,
                "The resolved delegation was not found.",
                false,
            )
        })?;
    let decision = store(&headers, &state)?
        .read_api_current_decision(&decision_id)
        .map_err(|_| {
            error(
                &headers,
                StatusCode::INTERNAL_SERVER_ERROR,
                wire::ApiErrorCode::InternalSafeFailure,
                "The resolved decision could not be projected safely.",
                true,
            )
        })?
        .ok_or_else(|| {
            error(
                &headers,
                StatusCode::NOT_FOUND,
                wire::ApiErrorCode::NotFound,
                "The resolved decision was not found.",
                false,
            )
        })?;
    let continuation_id = detail
        .continuations
        .iter()
        .find(|value| value.causation_id == decision_id)
        .map(|value| value.continuation_id.clone());
    Ok(Json(wire::ResolveDecisionResponse {
        decision: decision_summary(decision, &detail.delegation.normalized_original_intent),
        delegation: delegation_summary(&detail.delegation),
        continuation_id,
    }))
}

fn optional_text_digest(value: Option<&str>) -> Option<String> {
    value.map(|value| sha256_hex(value.as_bytes()))
}

#[derive(Deserialize)]
struct StoredPolicySnapshot {
    configured: bool,
    profile: Option<wire::AutonomyProfile>,
    #[serde(default)]
    rules: Vec<wire::PolicyRule>,
    effective_operational_summary: Option<String>,
}

pub(super) async fn get_policy(
    headers: HeaderMap,
    State(state): State<AppState>,
) -> WireResult<wire::PolicyResponse> {
    authenticate(
        &headers,
        &state,
        false,
        "execass.policy.read",
        "execass:policy",
    )?;
    let record = store(&headers, &state)?
        .current_execass_policy()
        .map_err(|_| {
            error(
                &headers,
                StatusCode::INTERNAL_SERVER_ERROR,
                wire::ApiErrorCode::InternalSafeFailure,
                "The current policy could not be read safely.",
                true,
            )
        })?;
    let snapshot: StoredPolicySnapshot = serde_json::from_str(&record.policy_snapshot_json)
        .map_err(|_| {
            error(
                &headers,
                StatusCode::INTERNAL_SERVER_ERROR,
                wire::ApiErrorCode::InternalSafeFailure,
                "The current policy projection is invalid.",
                false,
            )
        })?;
    Ok(Json(wire::PolicyResponse {
        policy_id: "execass-policy".into(),
        revision: record.policy_revision,
        profile: snapshot.profile,
        rules: snapshot.rules,
        effective_operational_summary: snapshot.effective_operational_summary.unwrap_or_else(
            || {
                if snapshot.configured {
                    "Owner-configured operational policy is active.".into()
                } else {
                    "No owner policy is configured yet.".into()
                }
            },
        ),
        configured: snapshot.configured,
        updated_at_ms: record.created_at,
    }))
}

pub(super) async fn put_policy(
    headers: HeaderMap,
    State(state): State<AppState>,
    Json(request): Json<wire::PolicyUpdateRequest>,
) -> WireResult<wire::PolicyUpdateResponse> {
    authenticate(
        &headers,
        &state,
        true,
        "execass.policy.update",
        "execass:policy",
    )?;
    require_idempotency(&headers, &request.idempotency_key)?;
    if request.expected_policy_revision <= 0 {
        return Err(error(
            &headers,
            StatusCode::BAD_REQUEST,
            wire::ApiErrorCode::InvalidRequest,
            "The expected policy revision is invalid.",
            false,
        ));
    }
    let runtime = state.execass_confirmation_runtime.as_ref().ok_or_else(|| {
        error(
            &headers,
            StatusCode::SERVICE_UNAVAILABLE,
            wire::ApiErrorCode::ExternalDependency,
            "ExecAss mutation runtime is unavailable.",
            true,
        )
    })?;
    let snapshot_json = serde_json::json!({
        "configured": true,
        "profile": request.proposed_profile,
        "rules": request.proposed_rules,
        "effective_operational_summary": request.change_summary,
    })
    .to_string();
    let snapshot_digest = runtime
        .policy_snapshot_digest(&snapshot_json)
        .map_err(|_| {
            error(
                &headers,
                StatusCode::BAD_REQUEST,
                wire::ApiErrorCode::InvalidRequest,
                "The policy snapshot cannot be stored safely.",
                false,
            )
        })?;
    let authorization = local_owner_mutation_authorization(&headers)?;
    let expected_binding = wire::LocalOwnerMutationBinding {
        operation: wire::OwnerMutationOperation::PolicyUpdate,
        method: "PUT".to_string(),
        path: "/api/v1/execass/policy".to_string(),
        request_correlation_id: correlation_id(&headers),
        idempotency_key: request.idempotency_key.clone(),
        expected_revision: request.expected_policy_revision,
        canonical_body_digest: sha256_hex(&serde_json::to_vec(&request).map_err(|_| {
            error(
                &headers,
                StatusCode::BAD_REQUEST,
                wire::ApiErrorCode::InvalidRequest,
                "The policy request is invalid.",
                false,
            )
        })?),
        safe_snapshot_digest: snapshot_digest,
        created_at_ms: authorization.binding.created_at_ms,
    };
    if authorization.binding != expected_binding {
        return Err(error(
            &headers,
            StatusCode::FORBIDDEN,
            wire::ApiErrorCode::AuthorityDenied,
            "The native-owner proof does not authorize this exact policy request.",
            false,
        ));
    }
    let verified = state
        .execass_actor_gate
        .verify_local_owner_mutation(&authorization.proof, &expected_binding)
        .map_err(|_| {
            error(
                &headers,
                StatusCode::FORBIDDEN,
                wire::ApiErrorCode::AuthorityDenied,
                "The native-owner proof does not authorize this exact policy request.",
                false,
            )
        })?;
    let actor = verified.owner_actor_assurance().ok_or_else(|| {
        error(
            &headers,
            StatusCode::FORBIDDEN,
            wire::ApiErrorCode::AuthorityDenied,
            "Verified native-owner authority is required.",
            false,
        )
    })?;
    let record = match runtime
        .coordinate_verified_policy_update(
            actor,
            &authorization.proof.authenticated_client_id,
            &expected_binding,
            &snapshot_json,
        )
        .map_err(|_| {
            error(
                &headers,
                StatusCode::SERVICE_UNAVAILABLE,
                wire::ApiErrorCode::ReceiptIntegrityQuarantined,
                "The policy update could not be committed safely.",
                true,
            )
        })? {
        storage::ExecAssPolicyUpdateOutcome::Updated { policy, .. }
        | storage::ExecAssPolicyUpdateOutcome::Replayed { policy, .. } => policy,
        storage::ExecAssPolicyUpdateOutcome::Stale { .. } => {
            return Err(error(
                &headers,
                StatusCode::CONFLICT,
                wire::ApiErrorCode::RevisionConflict,
                "The policy revision changed before this update.",
                false,
            ))
        }
        storage::ExecAssPolicyUpdateOutcome::Conflict => {
            return Err(error(
                &headers,
                StatusCode::CONFLICT,
                wire::ApiErrorCode::IdempotencyConflict,
                "The policy idempotency key already names different material.",
                false,
            ))
        }
    };
    let policy = policy_response(record).map_err(|_| {
        error(
            &headers,
            StatusCode::INTERNAL_SERVER_ERROR,
            wire::ApiErrorCode::InternalSafeFailure,
            "The updated policy could not be rendered safely.",
            false,
        )
    })?;
    Ok(Json(wire::PolicyUpdateResponse {
        updated_at_ms: policy.updated_at_ms,
        policy,
    }))
}

fn policy_response(
    record: storage::ExecAssPolicyRevisionRecord,
) -> anyhow::Result<wire::PolicyResponse> {
    let snapshot: StoredPolicySnapshot = serde_json::from_str(&record.policy_snapshot_json)?;
    Ok(wire::PolicyResponse {
        policy_id: "execass-policy".into(),
        revision: record.policy_revision,
        profile: snapshot.profile,
        rules: snapshot.rules,
        effective_operational_summary: snapshot.effective_operational_summary.unwrap_or_else(
            || {
                if snapshot.configured {
                    "Owner-configured operational policy is active.".into()
                } else {
                    "No owner policy is configured yet.".into()
                }
            },
        ),
        configured: snapshot.configured,
        updated_at_ms: record.created_at,
    })
}

pub(super) async fn get_runtime_host(
    headers: HeaderMap,
    State(state): State<AppState>,
) -> WireResult<wire::RuntimeHostStatusResponse> {
    authenticate(
        &headers,
        &state,
        false,
        "execass.runtime_host.read",
        "execass:runtime-host",
    )?;
    let status = store(&headers, &state)?
        .execass_runtime_host_status(current_time_ms())
        .map_err(|_| {
            error(
                &headers,
                StatusCode::INTERNAL_SERVER_ERROR,
                wire::ApiErrorCode::InternalSafeFailure,
                "The runtime-host state could not be read safely.",
                true,
            )
        })?;
    let desired_mode = match status
        .config
        .as_ref()
        .map(|value| value.desired_mode)
        .unwrap_or(storage::RuntimeDesiredMode::AppBound)
    {
        storage::RuntimeDesiredMode::AppBound => wire::RuntimeHostDesiredMode::AppBound,
        storage::RuntimeDesiredMode::Background => wire::RuntimeHostDesiredMode::Background,
    };
    let actual_state = match status.actual_state {
        storage::RuntimeActualState::Stopped => wire::RuntimeHostActualState::Stopped,
        storage::RuntimeActualState::Starting => wire::RuntimeHostActualState::Starting,
        storage::RuntimeActualState::RunningAppBound => {
            wire::RuntimeHostActualState::RunningAppBound
        }
        storage::RuntimeActualState::Handoff => wire::RuntimeHostActualState::Handoff,
        storage::RuntimeActualState::RunningBackground => {
            wire::RuntimeHostActualState::RunningBackground
        }
        storage::RuntimeActualState::Draining => wire::RuntimeHostActualState::Draining,
        storage::RuntimeActualState::Faulted => wire::RuntimeHostActualState::Faulted,
    };
    Ok(Json(wire::RuntimeHostStatusResponse {
        desired_mode,
        actual_state,
        ownership_mode: "single_execass_runtime_host".into(),
        process_id: None,
        started_at_ms: status.live_lease.as_ref().map(|value| value.acquired_at),
        fencing_generation: status
            .live_lease
            .as_ref()
            .map(|value| value.generation)
            .unwrap_or(0),
        state_root_version: status
            .live_lease
            .as_ref()
            .map(|value| format!("execass-v1.1-root-{}", value.state_root_generation))
            .unwrap_or_else(|| "execass-v1.1".into()),
        restart_reason: None,
        health: "authoritative".into(),
    }))
}

pub(super) async fn put_runtime_host(
    headers: HeaderMap,
    State(state): State<AppState>,
    Json(request): Json<wire::RuntimeHostConfigRequest>,
) -> WireResult<wire::RuntimeHostConfigResponse> {
    authenticate(
        &headers,
        &state,
        true,
        "execass.runtime_host.update",
        "execass:runtime-host",
    )?;
    require_idempotency(&headers, &request.idempotency_key)?;
    let mode = match request.desired_mode {
        wire::RuntimeHostDesiredMode::AppBound => storage::RuntimeDesiredMode::AppBound,
        wire::RuntimeHostDesiredMode::Background => storage::RuntimeDesiredMode::Background,
    };
    if request.start_at_login && mode != storage::RuntimeDesiredMode::Background {
        return Err(error(
            &headers,
            StatusCode::BAD_REQUEST,
            wire::ApiErrorCode::InvalidRequest,
            "Start at login requires background runtime mode.",
            false,
        ));
    }
    let runtime = state.execass_confirmation_runtime.as_ref().ok_or_else(|| {
        error(
            &headers,
            StatusCode::SERVICE_UNAVAILABLE,
            wire::ApiErrorCode::ExternalDependency,
            "ExecAss mutation runtime is unavailable.",
            true,
        )
    })?;
    let safe_settings_json = "{}";
    let snapshot_digest = runtime
        .runtime_settings_digest(mode, request.start_at_login, safe_settings_json)
        .map_err(|_| {
            error(
                &headers,
                StatusCode::BAD_REQUEST,
                wire::ApiErrorCode::InvalidRequest,
                "The runtime settings cannot be stored safely.",
                false,
            )
        })?;
    let authorization = local_owner_mutation_authorization(&headers)?;
    let expected_binding = wire::LocalOwnerMutationBinding {
        operation: wire::OwnerMutationOperation::RuntimeHostConfigUpdate,
        method: "PUT".to_string(),
        path: "/api/v1/execass/runtime-host".to_string(),
        request_correlation_id: correlation_id(&headers),
        idempotency_key: request.idempotency_key.clone(),
        expected_revision: request.expected_settings_revision,
        canonical_body_digest: sha256_hex(&serde_json::to_vec(&request).map_err(|_| {
            error(
                &headers,
                StatusCode::BAD_REQUEST,
                wire::ApiErrorCode::InvalidRequest,
                "The runtime settings request is invalid.",
                false,
            )
        })?),
        safe_snapshot_digest: snapshot_digest,
        created_at_ms: authorization.binding.created_at_ms,
    };
    if authorization.binding != expected_binding {
        return Err(error(
            &headers,
            StatusCode::FORBIDDEN,
            wire::ApiErrorCode::AuthorityDenied,
            "The native-owner proof does not authorize these exact runtime settings.",
            false,
        ));
    }
    let verified = state
        .execass_actor_gate
        .verify_local_owner_mutation(&authorization.proof, &expected_binding)
        .map_err(|_| {
            error(
                &headers,
                StatusCode::FORBIDDEN,
                wire::ApiErrorCode::AuthorityDenied,
                "The native-owner proof does not authorize these exact runtime settings.",
                false,
            )
        })?;
    let actor = verified.owner_actor_assurance().ok_or_else(|| {
        error(
            &headers,
            StatusCode::FORBIDDEN,
            wire::ApiErrorCode::AuthorityDenied,
            "Verified native-owner authority is required.",
            false,
        )
    })?;
    let mut status = match runtime
        .coordinate_verified_runtime_settings_update(
            actor,
            &authorization.proof.authenticated_client_id,
            &expected_binding,
            mode,
            request.start_at_login,
            safe_settings_json,
        )
        .map_err(|_| {
            error(
                &headers,
                StatusCode::SERVICE_UNAVAILABLE,
                wire::ApiErrorCode::ReceiptIntegrityQuarantined,
                "The runtime settings could not be committed safely.",
                true,
            )
        })? {
        storage::ExecAssRuntimeSettingsUpdateOutcome::Updated { status, .. }
        | storage::ExecAssRuntimeSettingsUpdateOutcome::Replayed { status, .. } => status,
        storage::ExecAssRuntimeSettingsUpdateOutcome::Stale { .. } => {
            return Err(error(
                &headers,
                StatusCode::CONFLICT,
                wire::ApiErrorCode::RevisionConflict,
                "The runtime settings revision changed before this update.",
                false,
            ))
        }
        storage::ExecAssRuntimeSettingsUpdateOutcome::Conflict => {
            return Err(error(
                &headers,
                StatusCode::CONFLICT,
                wire::ApiErrorCode::IdempotencyConflict,
                "The runtime-settings idempotency key already names different material.",
                false,
            ))
        }
    };
    let config = status.config.as_ref().ok_or_else(|| {
        error(
            &headers,
            StatusCode::INTERNAL_SERVER_ERROR,
            wire::ApiErrorCode::InternalSafeFailure,
            "The committed runtime settings disappeared.",
            false,
        )
    })?;
    #[cfg(windows)]
    {
        use crate::windows_task_scheduler::{DesiredMode, ReconcileRequest, SchedulerOutcome};
        let installed_executable =
            crate::windows_task_scheduler::current_installed_gateway_executable();
        let scheduler_receipt = if config.start_at_login {
            let executable = installed_executable.ok_or_else(|| {
                error(
                    &headers,
                    StatusCode::SERVICE_UNAVAILABLE,
                    wire::ApiErrorCode::ExternalDependency,
                    "The installed runtime-host path is unavailable for start-at-login.",
                    true,
                )
            })?;
            Some(crate::windows_task_scheduler::reconcile_current_user(
                ReconcileRequest {
                    desired_mode: match config.desired_mode {
                        storage::RuntimeDesiredMode::AppBound => DesiredMode::AppBound,
                        storage::RuntimeDesiredMode::Background => DesiredMode::Background,
                    },
                    start_at_login: true,
                    installed_gateway_executable: executable,
                },
            ))
        } else if installed_executable.is_some() {
            Some(crate::windows_task_scheduler::remove_current_user())
        } else {
            // Developer and test binaries must never inspect, alter, or remove
            // the production user's scheduled task. There is nothing to
            // reconcile for start_at_login=false outside the installed host.
            None
        };
        if let Some(scheduler_receipt) = scheduler_receipt {
            if !matches!(
                scheduler_receipt.outcome,
                SchedulerOutcome::Created
                    | SchedulerOutcome::Repaired
                    | SchedulerOutcome::Unchanged
                    | SchedulerOutcome::Disabled
                    | SchedulerOutcome::Removed
            ) {
                tracing::warn!(receipt = ?scheduler_receipt, "Windows runtime-host scheduler reconciliation failed closed");
                return Err(error(
                    &headers,
                    StatusCode::SERVICE_UNAVAILABLE,
                    wire::ApiErrorCode::ExternalDependency,
                    "The Windows start-at-login task could not be reconciled safely. The owner setting was retained for an exact repair retry.",
                    true,
                ));
            }
            tracing::info!(receipt = ?scheduler_receipt, "Windows runtime-host scheduler state reconciled");
        }
    }

    let needs_handoff = matches!(
        (status.actual_state, config.desired_mode),
        (
            storage::RuntimeActualState::RunningAppBound,
            storage::RuntimeDesiredMode::Background
        ) | (
            storage::RuntimeActualState::RunningBackground,
            storage::RuntimeDesiredMode::AppBound
        )
    );
    if needs_handoff {
        let host = state.execass_runtime_host.as_ref().ok_or_else(|| {
            error(
                &headers,
                StatusCode::SERVICE_UNAVAILABLE,
                wire::ApiErrorCode::ExternalDependency,
                "The current runtime-host lease is unavailable for mode handoff.",
                true,
            )
        })?;
        let store = state.execass_store.as_ref().ok_or_else(|| {
            error(
                &headers,
                StatusCode::SERVICE_UNAVAILABLE,
                wire::ApiErrorCode::ExternalDependency,
                "The current runtime-host store is unavailable for mode handoff.",
                true,
            )
        })?;
        store
            .transition_runtime_host(
                host,
                storage::RuntimeHostTransition::BeginHandoff,
                current_time_ms(),
            )
            .and_then(|_| {
                store.transition_runtime_host(
                    host,
                    storage::RuntimeHostTransition::ReachDesiredMode,
                    current_time_ms(),
                )
            })
            .map_err(|_| {
                error(
                    &headers,
                    StatusCode::SERVICE_UNAVAILABLE,
                    wire::ApiErrorCode::ExternalDependency,
                    "The runtime host could not complete its fenced mode handoff.",
                    true,
                )
            })?;
        status = store
            .execass_runtime_host_status(current_time_ms())
            .map_err(|_| {
                error(
                    &headers,
                    StatusCode::INTERNAL_SERVER_ERROR,
                    wire::ApiErrorCode::InternalSafeFailure,
                    "The runtime-host state could not be read after handoff.",
                    true,
                )
            })?;
    }
    let config = status.config.as_ref().ok_or_else(|| {
        error(
            &headers,
            StatusCode::INTERNAL_SERVER_ERROR,
            wire::ApiErrorCode::InternalSafeFailure,
            "The committed runtime settings disappeared after handoff.",
            false,
        )
    })?;
    Ok(Json(wire::RuntimeHostConfigResponse {
        status: runtime_host_status_response(&status),
        start_at_login: config.start_at_login,
        bounded_settings_revision: config.settings_revision,
    }))
}

fn runtime_host_status_response(
    status: &storage::ExecAssRuntimeHostStatus,
) -> wire::RuntimeHostStatusResponse {
    let desired_mode = match status
        .config
        .as_ref()
        .map(|value| value.desired_mode)
        .unwrap_or(storage::RuntimeDesiredMode::AppBound)
    {
        storage::RuntimeDesiredMode::AppBound => wire::RuntimeHostDesiredMode::AppBound,
        storage::RuntimeDesiredMode::Background => wire::RuntimeHostDesiredMode::Background,
    };
    let actual_state = match status.actual_state {
        storage::RuntimeActualState::Stopped => wire::RuntimeHostActualState::Stopped,
        storage::RuntimeActualState::Starting => wire::RuntimeHostActualState::Starting,
        storage::RuntimeActualState::RunningAppBound => {
            wire::RuntimeHostActualState::RunningAppBound
        }
        storage::RuntimeActualState::Handoff => wire::RuntimeHostActualState::Handoff,
        storage::RuntimeActualState::RunningBackground => {
            wire::RuntimeHostActualState::RunningBackground
        }
        storage::RuntimeActualState::Draining => wire::RuntimeHostActualState::Draining,
        storage::RuntimeActualState::Faulted => wire::RuntimeHostActualState::Faulted,
    };
    wire::RuntimeHostStatusResponse {
        desired_mode,
        actual_state,
        ownership_mode: "single_execass_runtime_host".into(),
        process_id: None,
        started_at_ms: status.live_lease.as_ref().map(|value| value.acquired_at),
        fencing_generation: status
            .live_lease
            .as_ref()
            .map(|value| value.generation)
            .unwrap_or(0),
        state_root_version: status
            .live_lease
            .as_ref()
            .map(|value| format!("execass-v1.1-root-{}", value.state_root_generation))
            .unwrap_or_else(|| "execass-v1.1".into()),
        restart_reason: None,
        health: "authoritative".into(),
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    format!("{:x}", Sha256::digest(bytes))
}

fn cursor_key(state: &AppState) -> storage::ApiCursorKey {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(
        [
            b"carsinos.execass.api.cursor.v1:".as_slice(),
            state.auth_token.as_bytes(),
        ]
        .concat(),
    );
    let mut key = [0_u8; 32];
    key.copy_from_slice(&digest);
    storage::ApiCursorKey(key)
}

fn verifier_type(value: storage::VerifierType) -> &'static str {
    match value {
        storage::VerifierType::Artifact => "artifact",
        storage::VerifierType::AuthoritativeState => "authoritative_state",
        storage::VerifierType::ProviderState => "provider_state",
        storage::VerifierType::Delivery => "delivery",
        storage::VerifierType::ProcessExit => "process_exit",
        storage::VerifierType::DatabasePredicate => "database_predicate",
        storage::VerifierType::HumanBoundSupersession => "human_bound_supersession",
    }
}

fn branch_state(value: &str) -> wire::BranchState {
    match value {
        "runnable" => wire::BranchState::Runnable,
        "executing" => wire::BranchState::Executing,
        "waiting" => wire::BranchState::Waiting,
        "uncertain" => wire::BranchState::Uncertain,
        _ => wire::BranchState::Terminal,
    }
}

fn continuation_status(value: storage::ContinuationStatus) -> wire::ContinuationStatus {
    match value {
        storage::ContinuationStatus::Runnable => wire::ContinuationStatus::Runnable,
        storage::ContinuationStatus::Executing => wire::ContinuationStatus::Claimed,
        storage::ContinuationStatus::Waiting | storage::ContinuationStatus::Uncertain => {
            wire::ContinuationStatus::Waiting
        }
        storage::ContinuationStatus::Terminal => wire::ContinuationStatus::Completed,
        storage::ContinuationStatus::Superseded => wire::ContinuationStatus::Superseded,
    }
}

fn effect_status(value: storage::LogicalEffectState) -> wire::EffectStatus {
    match value {
        storage::LogicalEffectState::Planned => wire::EffectStatus::Planned,
        storage::LogicalEffectState::Claimed | storage::LogicalEffectState::Invoking => {
            wire::EffectStatus::Claimed
        }
        storage::LogicalEffectState::Succeeded | storage::LogicalEffectState::ReconciledPresent => {
            wire::EffectStatus::Succeeded
        }
        storage::LogicalEffectState::Failed | storage::LogicalEffectState::ReconciledAbsent => {
            wire::EffectStatus::Failed
        }
        storage::LogicalEffectState::OutcomeUnknown => wire::EffectStatus::OutcomeUnknown,
    }
}

fn verifier_result(value: &str) -> wire::VerifierResult {
    match value {
        "pass" => wire::VerifierResult::Pass,
        "fail" => wire::VerifierResult::Fail,
        _ => wire::VerifierResult::Unknown,
    }
}

fn decision_summary(
    value: storage::ApiDecisionRead,
    normalized_intent: &str,
) -> wire::DecisionSummary {
    let local_owner_proof_challenge = if value.decision.status == storage::DecisionStatus::Pending {
        let proof_clock =
            if value.decision.decision_kind == storage::DecisionKind::DangerousActionConfirmation {
                value
                    .challenge_nonce_digest
                    .clone()
                    .zip(value.challenge_expires_at)
            } else {
                carsinos_core::execass_actor::owner_resolution_challenge_nonce_digest(
                    value.decision.idempotency_key.as_bytes(),
                )
                .map(|digest| (digest, i64::MAX))
            };
        match (
            value.decision.confirmed_logical_action_identity.clone(),
            proof_clock,
        ) {
            (selected_logical_action_id, Some((challenge_digest, expires_at_ms)))
                if !selected_logical_action_id.trim().is_empty() =>
            {
                Some(wire::DecisionProofChallenge {
                    decision_id: value.decision.decision_id.clone(),
                    decision_revision: u64::try_from(value.decision.decision_revision)
                        .unwrap_or_default(),
                    normalized_intent_digest:
                        carsinos_core::execass_actor::owner_normalized_intent_digest(
                            normalized_intent,
                        )
                        .unwrap_or_default(),
                    policy_revision: value.decision.policy_revision,
                    canonical_manifest_digest: value.decision.manifest_digest.clone(),
                    selected_logical_action_id,
                    presented_action_digest: sha256_hex(
                        value.exact_presented_action_json.as_bytes(),
                    ),
                    declared_consequence_digest: sha256_hex(value.consequence.as_bytes()),
                    challenge_digest,
                    expires_at_ms,
                })
            }
            _ => None,
        }
    } else {
        None
    };
    let challenge = match (value.challenge_id, value.challenge_expires_at) {
        (Some(challenge_id), Some(expires_at_ms)) => Some(wire::DecisionChallenge {
            decision_revision: value.decision.decision_revision,
            exact_presented_action_or_alternative: value.exact_presented_action_json.clone(),
            declared_consequence: value.consequence.clone(),
            nonce_or_token: challenge_id,
            expires_at_ms,
        }),
        _ => None,
    };
    let accepted_confirmation_grant =
        value
            .accepted_grant
            .map(|grant| wire::AcceptedConfirmationGrant {
                delegation_id: value.decision.delegation_id.clone(),
                normalized_intent: normalized_intent.to_owned(),
                confirmed_logical_action_identity: value
                    .decision
                    .confirmed_logical_action_identity
                    .clone(),
                canonical_action_envelope_or_selector: grant
                    .canonical_action_envelope_or_selector_json,
                payload_and_material_operands_digest: grant.payload_and_material_operands_digest,
                connector_or_tool_identity_and_version: grant
                    .connector_tool_identity_and_version
                    .unwrap_or_else(|| "local-runtime".into()),
                declared_consequence: grant.declared_consequence,
            });
    let resolved_owner = value
        .resolved_owner
        .map(|owner| wire::OwnerResolutionSummary {
            ingress: if owner.authenticated_ingress.contains("telegram")
                || owner.authenticated_ingress.contains("discord")
            {
                wire::OwnerResolutionIngress::AuthenticatedRemoteOwnerChannel
            } else {
                wire::OwnerResolutionIngress::LocalOwnerSession
            },
            verified_evidence_ref: owner.verified_evidence_ref,
        });
    let alternatives = serde_json::from_str(&value.alternatives_json).unwrap_or_default();
    wire::DecisionSummary {
        decision_id: value.decision.decision_id.clone(),
        delegation_id: value.decision.delegation_id.clone(),
        revision: value.decision.decision_revision,
        status: decision_status(value.decision.status),
        kind: decision_kind(value.decision.decision_kind),
        result: value.decision.result.map(decision_result),
        assurance_required: wire::AssuranceRequirement::VerifiedOwnerResolution,
        recommendation: value.recommendation,
        why_now: "This input is required before the pending branch can continue.".into(),
        consequence: value.consequence,
        alternatives,
        exact_manifest_digest: value.decision.manifest_digest,
        technical_resources: Vec::new(),
        challenge,
        accepted_confirmation_grant,
        resolved_owner,
        requested_at_ms: value.decision.requested_at,
        resolved_at_ms: value.decision.resolved_at,
        authoritative_deep_link: format!("/execass/decisions/{}", value.decision.decision_id),
        local_owner_proof_challenge,
    }
}

fn decision_kind(value: storage::DecisionKind) -> wire::DecisionKind {
    match value {
        storage::DecisionKind::Clarification => wire::DecisionKind::Clarification,
        storage::DecisionKind::DangerousActionConfirmation => {
            wire::DecisionKind::DangerousActionConfirmation
        }
        storage::DecisionKind::OwnerConfiguredCheckpoint => {
            wire::DecisionKind::OwnerConfiguredCheckpoint
        }
        storage::DecisionKind::RecoveryChoice => wire::DecisionKind::RecoveryChoice,
        storage::DecisionKind::DuplicateRiskRetry => wire::DecisionKind::DuplicateRiskRetry,
        storage::DecisionKind::Stop => wire::DecisionKind::Stop,
        storage::DecisionKind::PolicyChange => wire::DecisionKind::PolicyChange,
    }
}

fn decision_status(value: storage::DecisionStatus) -> wire::DecisionStatus {
    match value {
        storage::DecisionStatus::Pending => wire::DecisionStatus::Pending,
        storage::DecisionStatus::Resolved => wire::DecisionStatus::Resolved,
        storage::DecisionStatus::Superseded => wire::DecisionStatus::Superseded,
        storage::DecisionStatus::Expired => wire::DecisionStatus::Expired,
    }
}

fn decision_result(value: storage::DecisionResult) -> wire::DecisionResult {
    match value {
        storage::DecisionResult::ConfirmAndContinue => wire::DecisionResult::ConfirmAndContinue,
        storage::DecisionResult::Revise => wire::DecisionResult::Revise,
        storage::DecisionResult::Decline => wire::DecisionResult::Decline,
        storage::DecisionResult::Stop => wire::DecisionResult::Stop,
    }
}
