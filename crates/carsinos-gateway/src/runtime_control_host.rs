//! Authenticated native status/attach and app-bound graceful-close control.
//!
//! This handler never terminates a process. A successful close request sends a
//! typed authorization to the gateway's parent supervision layer, which owns
//! the actual drain and graceful server shutdown sequence.

use carsinos_runtime_control::{
    ActiveWorkStatusV1, AttachReplyV1, AttachRequestV1, CloseConfirmationBindingV1,
    GracefulShutdownDispositionV1, GracefulShutdownRejectionReasonV1, GracefulShutdownReplyV1,
    GracefulShutdownRequestV1, RuntimeActualStateV1, RuntimeControlHandler,
    RuntimeControlRejectionCodeV1, RuntimeControlRequestContextV1, RuntimeDesiredModeV1,
    ShutdownAuthorizationV1, StatusReplyV1,
};
use carsinos_storage::execass::{
    ExecAssActiveWorkStatus, ExecAssStore, RuntimeActualState, RuntimeDesiredMode,
    RuntimeHostLeaseRecord,
};
use sha2::{Digest, Sha256};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::mpsc::UnboundedSender;

const CLOSE_CHALLENGE_DOMAIN: &[u8] = b"carsinos.runtime-control.close-challenge.v1";
const SHUTDOWN_AUTHORIZATION_DOMAIN: &[u8] = b"carsinos.runtime-control.shutdown-authorization.v1";

pub(crate) struct GatewayRuntimeControlHandler {
    store: ExecAssStore,
    host: RuntimeHostLeaseRecord,
    close_control: CloseControl,
}

impl GatewayRuntimeControlHandler {
    /// Parent-main integration seam. The receiver owns graceful drain/stop;
    /// this native handler owns only authorization and exactly-once signaling.
    pub(crate) fn new_with_shutdown_authorization_sender(
        store: ExecAssStore,
        host: RuntimeHostLeaseRecord,
        shutdown_authorization_tx: UnboundedSender<ShutdownAuthorizationV1>,
    ) -> Self {
        Self {
            store,
            host,
            close_control: CloseControl::new(Some(shutdown_authorization_tx)),
        }
    }

    /// Supervision may reject a just-issued authorization if the exact work
    /// snapshot changed before the durable pause transaction. Only that exact
    /// authorization can reopen the close state for a fresh request.
    pub(crate) fn reject_shutdown_authorization(&self, authorization_id: &str) {
        if let Ok(mut state) = self.close_control.state.lock() {
            if state.authorized_authorization_id.as_deref() == Some(authorization_id) {
                state.authorized_authorization_id = None;
                state.pending = None;
            }
        }
    }

    fn current_status(&self) -> Result<StatusReplyV1, RuntimeControlRejectionCodeV1> {
        self.current_status_and_binding().map(|(status, _)| status)
    }

    fn current_status_and_binding(
        &self,
    ) -> Result<(StatusReplyV1, String), RuntimeControlRejectionCodeV1> {
        let trusted_now = trusted_now_ms()?;
        let snapshot = self
            .store
            .execass_runtime_close_snapshot(trusted_now)
            .map_err(|_| RuntimeControlRejectionCodeV1::Unavailable)?;
        let live = snapshot
            .host
            .live_lease
            .filter(|live| live == &self.host)
            .ok_or(RuntimeControlRejectionCodeV1::Unavailable)?;
        let (desired_mode, start_at_login) = snapshot
            .host
            .config
            .as_ref()
            .map(|config| (config.desired_mode, config.start_at_login))
            .unwrap_or((RuntimeDesiredMode::AppBound, false));
        let status = StatusReplyV1 {
            runtime_host_generation: live.generation,
            runtime_host_instance_id: live.host_instance_id,
            actual_state: map_actual_state(snapshot.host.actual_state),
            desired_mode: match desired_mode {
                RuntimeDesiredMode::AppBound => RuntimeDesiredModeV1::AppBound,
                RuntimeDesiredMode::Background => RuntimeDesiredModeV1::Background,
            },
            start_at_login,
            active_work: map_active_work(snapshot.active_work),
        };
        Ok((status, snapshot.active_work_binding_digest))
    }
}

#[async_trait::async_trait]
impl RuntimeControlHandler for GatewayRuntimeControlHandler {
    async fn status(&self) -> Result<StatusReplyV1, RuntimeControlRejectionCodeV1> {
        self.current_status()
    }

    async fn attach(
        &self,
        _request: AttachRequestV1,
    ) -> Result<AttachReplyV1, RuntimeControlRejectionCodeV1> {
        let status = self.current_status()?;
        Ok(AttachReplyV1 {
            runtime_host_generation: status.runtime_host_generation,
            runtime_host_instance_id: status.runtime_host_instance_id,
        })
    }

    async fn graceful_shutdown(
        &self,
        context: RuntimeControlRequestContextV1,
        request: GracefulShutdownRequestV1,
    ) -> Result<GracefulShutdownReplyV1, RuntimeControlRejectionCodeV1> {
        let (status, active_work_binding_digest) = self.current_status_and_binding()?;
        self.close_control
            .decide(&status, &active_work_binding_digest, context, request)
    }
}

struct CloseControl {
    state: Mutex<CloseControlState>,
    shutdown_authorization_tx: Option<UnboundedSender<ShutdownAuthorizationV1>>,
}

impl CloseControl {
    fn new(shutdown_authorization_tx: Option<UnboundedSender<ShutdownAuthorizationV1>>) -> Self {
        Self {
            state: Mutex::new(CloseControlState::default()),
            shutdown_authorization_tx,
        }
    }

    fn decide(
        &self,
        status: &StatusReplyV1,
        active_work_binding_digest: &str,
        context: RuntimeControlRequestContextV1,
        request: GracefulShutdownRequestV1,
    ) -> Result<GracefulShutdownReplyV1, RuntimeControlRejectionCodeV1> {
        if status.desired_mode == RuntimeDesiredModeV1::Background {
            return Ok(rejected(GracefulShutdownRejectionReasonV1::BackgroundMode));
        }
        if status.actual_state != RuntimeActualStateV1::RunningAppBound {
            return Ok(rejected(
                GracefulShutdownRejectionReasonV1::NotRunningAppBound,
            ));
        }
        if request.runtime_host_generation != status.runtime_host_generation
            || request.runtime_host_instance_id != status.runtime_host_instance_id
        {
            return Ok(rejected(
                GracefulShutdownRejectionReasonV1::StaleHostBinding,
            ));
        }

        let mut state = self
            .state
            .lock()
            .map_err(|_| RuntimeControlRejectionCodeV1::Unavailable)?;
        if state.authorized_authorization_id.is_some() {
            return Ok(rejected(
                GracefulShutdownRejectionReasonV1::AlreadyAuthorized,
            ));
        }

        if status.active_work.active {
            if let Some(confirmation) = request.confirmation.as_ref() {
                let Some(pending) = state.pending.as_ref() else {
                    return Ok(rejected(
                        GracefulShutdownRejectionReasonV1::InvalidConfirmation,
                    ));
                };
                if confirmation != &pending.binding
                    || request.client_instance_id != pending.client_instance_id
                    || request.runtime_host_generation != pending.runtime_host_generation
                    || request.runtime_host_instance_id != pending.runtime_host_instance_id
                {
                    return Ok(rejected(
                        GracefulShutdownRejectionReasonV1::InvalidConfirmation,
                    ));
                }
                if status.active_work != pending.active_work
                    || active_work_binding_digest != pending.active_work_binding_digest
                {
                    return Ok(rejected(
                        GracefulShutdownRejectionReasonV1::CloseStateChanged,
                    ));
                }
            } else {
                let pending_matches = state.pending.as_ref().is_some_and(|pending| {
                    pending.client_instance_id == request.client_instance_id
                        && pending.runtime_host_generation == request.runtime_host_generation
                        && pending.runtime_host_instance_id == request.runtime_host_instance_id
                        && pending.active_work == status.active_work
                        && pending.active_work_binding_digest == active_work_binding_digest
                });
                if !pending_matches {
                    state.pending = Some(PendingClose {
                        binding: close_binding(
                            &context,
                            &request,
                            &status.active_work,
                            active_work_binding_digest,
                        ),
                        client_instance_id: request.client_instance_id.clone(),
                        runtime_host_generation: request.runtime_host_generation,
                        runtime_host_instance_id: request.runtime_host_instance_id.clone(),
                        active_work: status.active_work.clone(),
                        active_work_binding_digest: active_work_binding_digest.to_owned(),
                    });
                }
                let Some(pending) = state.pending.as_ref() else {
                    return Err(RuntimeControlRejectionCodeV1::Unavailable);
                };
                return Ok(GracefulShutdownReplyV1 {
                    disposition: GracefulShutdownDispositionV1::ConfirmationRequired {
                        active_work: pending.active_work.clone(),
                        consequence: close_consequence(pending.active_work.active_work_count),
                        binding: pending.binding.clone(),
                    },
                });
            }
        } else if request.confirmation.is_some() {
            return Ok(rejected(
                GracefulShutdownRejectionReasonV1::CloseStateChanged,
            ));
        }

        let authorization = shutdown_authorization(
            &context,
            &request,
            &status.active_work,
            active_work_binding_digest,
        );
        let Some(sender) = self.shutdown_authorization_tx.as_ref() else {
            return Ok(rejected(
                GracefulShutdownRejectionReasonV1::SupervisorUnavailable,
            ));
        };
        if sender.send(authorization.clone()).is_err() {
            return Ok(rejected(
                GracefulShutdownRejectionReasonV1::SupervisorUnavailable,
            ));
        }
        state.authorized_authorization_id = Some(authorization.authorization_id.clone());
        state.pending = None;
        Ok(GracefulShutdownReplyV1 {
            disposition: GracefulShutdownDispositionV1::Authorized { authorization },
        })
    }
}

#[derive(Default)]
struct CloseControlState {
    pending: Option<PendingClose>,
    authorized_authorization_id: Option<String>,
}

struct PendingClose {
    binding: CloseConfirmationBindingV1,
    client_instance_id: String,
    runtime_host_generation: i64,
    runtime_host_instance_id: String,
    active_work: ActiveWorkStatusV1,
    active_work_binding_digest: String,
}

fn rejected(reason: GracefulShutdownRejectionReasonV1) -> GracefulShutdownReplyV1 {
    GracefulShutdownReplyV1 {
        disposition: GracefulShutdownDispositionV1::Rejected { reason },
    }
}

fn close_binding(
    context: &RuntimeControlRequestContextV1,
    request: &GracefulShutdownRequestV1,
    active_work: &ActiveWorkStatusV1,
    active_work_binding_digest: &str,
) -> CloseConfirmationBindingV1 {
    let mut digest = Sha256::new();
    digest.update(CLOSE_CHALLENGE_DOMAIN);
    update_digest(&mut digest, &context.request_id);
    update_digest(&mut digest, &context.nonce);
    digest.update(context.issued_at_ms.to_be_bytes());
    update_digest(&mut digest, &request.client_instance_id);
    digest.update(request.runtime_host_generation.to_be_bytes());
    update_digest(&mut digest, &request.runtime_host_instance_id);
    digest.update(active_work.active_work_count.to_be_bytes());
    digest.update(active_work.nonterminal_delegation_count.to_be_bytes());
    digest.update(active_work.nonterminal_continuation_count.to_be_bytes());
    digest.update(active_work.nonterminal_effect_count.to_be_bytes());
    update_digest(&mut digest, active_work_binding_digest);
    CloseConfirmationBindingV1 {
        challenge: encode_hex(&digest.finalize()),
        original_request_id: context.request_id.clone(),
        original_nonce: context.nonce.clone(),
        original_issued_at_ms: context.issued_at_ms,
    }
}

fn shutdown_authorization(
    context: &RuntimeControlRequestContextV1,
    request: &GracefulShutdownRequestV1,
    active_work: &ActiveWorkStatusV1,
    active_work_binding_digest: &str,
) -> ShutdownAuthorizationV1 {
    let mut digest = Sha256::new();
    digest.update(SHUTDOWN_AUTHORIZATION_DOMAIN);
    update_digest(&mut digest, &context.request_id);
    update_digest(&mut digest, &context.nonce);
    digest.update(context.issued_at_ms.to_be_bytes());
    update_digest(&mut digest, &request.client_instance_id);
    digest.update(request.runtime_host_generation.to_be_bytes());
    update_digest(&mut digest, &request.runtime_host_instance_id);
    if let Some(binding) = request.confirmation.as_ref() {
        update_digest(&mut digest, &binding.challenge);
    }
    digest.update([u8::from(active_work.active)]);
    digest.update(active_work.active_work_count.to_be_bytes());
    update_digest(&mut digest, active_work_binding_digest);
    ShutdownAuthorizationV1 {
        authorization_id: encode_hex(&digest.finalize()),
        request_id: context.request_id.clone(),
        nonce: context.nonce.clone(),
        issued_at_ms: context.issued_at_ms,
        client_instance_id: request.client_instance_id.clone(),
        runtime_host_generation: request.runtime_host_generation,
        runtime_host_instance_id: request.runtime_host_instance_id.clone(),
        confirmed_active_work_binding_digest: active_work
            .active
            .then(|| active_work_binding_digest.to_owned()),
        confirmed_active_work_count: active_work.active_work_count,
    }
}

fn update_digest(digest: &mut Sha256, value: &str) {
    digest.update((value.len() as u64).to_be_bytes());
    digest.update(value.as_bytes());
}

fn encode_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

fn close_consequence(active_work_count: i64) -> String {
    let noun = if active_work_count == 1 {
        "active local work item"
    } else {
        "active local work items"
    };
    format!("Closing now will pause {active_work_count} {noun} and stop the app-bound runtime.")
}

fn map_actual_state(state: RuntimeActualState) -> RuntimeActualStateV1 {
    match state {
        RuntimeActualState::Stopped => RuntimeActualStateV1::Stopped,
        RuntimeActualState::Starting => RuntimeActualStateV1::Starting,
        RuntimeActualState::RunningAppBound => RuntimeActualStateV1::RunningAppBound,
        RuntimeActualState::Handoff => RuntimeActualStateV1::Handoff,
        RuntimeActualState::RunningBackground => RuntimeActualStateV1::RunningBackground,
        RuntimeActualState::Draining => RuntimeActualStateV1::Draining,
        RuntimeActualState::Faulted => RuntimeActualStateV1::Faulted,
    }
}

fn map_active_work(status: ExecAssActiveWorkStatus) -> ActiveWorkStatusV1 {
    ActiveWorkStatusV1 {
        active: status.active,
        active_work_count: status.active_work_count,
        nonterminal_delegation_count: status.nonterminal_delegation_count,
        nonterminal_continuation_count: status.nonterminal_continuation_count,
        nonterminal_effect_count: status.nonterminal_effect_count,
    }
}

fn trusted_now_ms() -> Result<i64, RuntimeControlRejectionCodeV1> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|value| i64::try_from(value.as_millis()).ok())
        .ok_or(RuntimeControlRejectionCodeV1::Unavailable)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context(value: char) -> RuntimeControlRequestContextV1 {
        RuntimeControlRequestContextV1 {
            request_id: value.to_string().repeat(64),
            nonce: ((value as u8 + 1) as char).to_string().repeat(64),
            issued_at_ms: 1_800_000_000_000,
        }
    }

    fn request(confirmation: Option<CloseConfirmationBindingV1>) -> GracefulShutdownRequestV1 {
        GracefulShutdownRequestV1 {
            client_instance_id: "mission-control-1".into(),
            runtime_host_generation: 7,
            runtime_host_instance_id: "host-7".into(),
            confirmation,
        }
    }

    fn status(desired_mode: RuntimeDesiredModeV1, active_work_count: i64) -> StatusReplyV1 {
        StatusReplyV1 {
            runtime_host_generation: 7,
            runtime_host_instance_id: "host-7".into(),
            actual_state: if desired_mode == RuntimeDesiredModeV1::Background {
                RuntimeActualStateV1::RunningBackground
            } else {
                RuntimeActualStateV1::RunningAppBound
            },
            desired_mode,
            start_at_login: desired_mode == RuntimeDesiredModeV1::Background,
            active_work: ActiveWorkStatusV1 {
                active: active_work_count > 0,
                active_work_count,
                nonterminal_delegation_count: active_work_count,
                nonterminal_continuation_count: 0,
                nonterminal_effect_count: 0,
            },
        }
    }

    fn work_binding(value: char) -> String {
        format!("sha256:{}", value.to_string().repeat(64))
    }

    #[test]
    fn background_mode_rejects_close_without_signaling() {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let control = CloseControl::new(Some(tx));
        let reply = control
            .decide(
                &status(RuntimeDesiredModeV1::Background, 0),
                &work_binding('a'),
                context('1'),
                request(None),
            )
            .unwrap();
        assert_eq!(
            reply.disposition,
            GracefulShutdownDispositionV1::Rejected {
                reason: GracefulShutdownRejectionReasonV1::BackgroundMode
            }
        );
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn active_work_prompts_once_then_unchanged_confirmation_signals_exactly_once() {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let control = CloseControl::new(Some(tx));
        let active = status(RuntimeDesiredModeV1::AppBound, 3);
        let first = control
            .decide(&active, &work_binding('a'), context('1'), request(None))
            .unwrap();
        let GracefulShutdownDispositionV1::ConfirmationRequired {
            active_work,
            consequence,
            binding,
        } = first.disposition
        else {
            panic!("expected confirmation");
        };
        assert_eq!(active_work.active_work_count, 3);
        assert_eq!(
            consequence,
            "Closing now will pause 3 active local work items and stop the app-bound runtime."
        );

        let repeated = control
            .decide(&active, &work_binding('a'), context('2'), request(None))
            .unwrap();
        let GracefulShutdownDispositionV1::ConfirmationRequired {
            binding: stable, ..
        } = repeated.disposition
        else {
            panic!("expected stable confirmation");
        };
        assert_eq!(stable, binding);

        let authorized = control
            .decide(
                &active,
                &work_binding('a'),
                context('3'),
                request(Some(binding.clone())),
            )
            .unwrap();
        let GracefulShutdownDispositionV1::Authorized { authorization } = authorized.disposition
        else {
            panic!("expected authorization");
        };
        assert_eq!(authorization.request_id, "3".repeat(64));
        assert_eq!(rx.try_recv().unwrap(), authorization);
        assert!(rx.try_recv().is_err());

        let replay = control
            .decide(
                &active,
                &work_binding('a'),
                context('4'),
                request(Some(binding)),
            )
            .unwrap();
        assert_eq!(
            replay.disposition,
            GracefulShutdownDispositionV1::Rejected {
                reason: GracefulShutdownRejectionReasonV1::AlreadyAuthorized
            }
        );
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn changed_exact_work_cannot_reuse_confirmation_even_when_counts_match() {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let control = CloseControl::new(Some(tx));
        let first = control
            .decide(
                &status(RuntimeDesiredModeV1::AppBound, 2),
                &work_binding('a'),
                context('1'),
                request(None),
            )
            .unwrap();
        let GracefulShutdownDispositionV1::ConfirmationRequired { binding, .. } = first.disposition
        else {
            panic!("expected confirmation");
        };
        let changed = control
            .decide(
                &status(RuntimeDesiredModeV1::AppBound, 2),
                &work_binding('b'),
                context('2'),
                request(Some(binding)),
            )
            .unwrap();
        assert_eq!(
            changed.disposition,
            GracefulShutdownDispositionV1::Rejected {
                reason: GracefulShutdownRejectionReasonV1::CloseStateChanged
            }
        );
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn stale_host_binding_is_rejected_before_confirmation_or_signal() {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let control = CloseControl::new(Some(tx));
        let mut stale = request(None);
        stale.runtime_host_generation = 6;
        let reply = control
            .decide(
                &status(RuntimeDesiredModeV1::AppBound, 1),
                &work_binding('a'),
                context('1'),
                stale,
            )
            .unwrap();
        assert_eq!(
            reply.disposition,
            GracefulShutdownDispositionV1::Rejected {
                reason: GracefulShutdownRejectionReasonV1::StaleHostBinding
            }
        );
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn no_active_work_authorizes_immediately_but_requires_live_supervisor() {
        let unavailable = CloseControl::new(None)
            .decide(
                &status(RuntimeDesiredModeV1::AppBound, 0),
                &work_binding('a'),
                context('1'),
                request(None),
            )
            .unwrap();
        assert_eq!(
            unavailable.disposition,
            GracefulShutdownDispositionV1::Rejected {
                reason: GracefulShutdownRejectionReasonV1::SupervisorUnavailable
            }
        );

        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let control = CloseControl::new(Some(tx));
        let reply = control
            .decide(
                &status(RuntimeDesiredModeV1::AppBound, 0),
                &work_binding('a'),
                context('2'),
                request(None),
            )
            .unwrap();
        assert!(matches!(
            reply.disposition,
            GracefulShutdownDispositionV1::Authorized { .. }
        ));
        assert!(rx.try_recv().is_ok());
    }
}
