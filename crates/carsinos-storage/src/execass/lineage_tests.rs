//! EA-108 focused atomicity, replay, resolution, and unsupported-target tests.

use super::tests::{fixture, foundation};
use super::*;
use crate::{NewAssistantToolCallAudit, Storage};
use rusqlite::{params, Connection};

fn lineage(target: AuthorityLinkTarget, suffix: &str) -> AppendAuthorityLineageCommand {
    let timestamp = 1_800_000_010_000;
    AppendAuthorityLineageCommand {
        write: WriteContext {
            idempotency_key: format!("lineage-idem-{suffix}"),
            correlation_id: format!("lineage-corr-{suffix}"),
            causation_id: format!("lineage-cause-{suffix}"),
            occurred_at: timestamp,
        },
        delegation_id: "delegation-1".into(),
        expected_state_revision: 1,
        resulting_state_revision: 2,
        linked_at: timestamp,
        links: vec![NewAuthorityLink {
            link_id: format!("link-{suffix}"),
            target,
        }],
        outbox_event: NewOutboxEvent {
            event_id: format!("lineage-event-{suffix}"),
            event_name: OutboxEventName::DelegationTransitioned,
            aggregate_id: "delegation-1".into(),
            aggregate_revision: 2,
            correlation_id: format!("lineage-corr-{suffix}"),
            causation_id: format!("lineage-cause-{suffix}"),
            occurred_at: timestamp,
            safe_payload_json: "{}".into(),
            duplicate_identity: format!("lineage-idem-{suffix}"),
        },
    }
}

fn observation(target: AuthorityLinkTarget, suffix: &str) -> ObserveOrchestrationCommand {
    let lineage = lineage(target, suffix);
    let ownership_checks = lineage
        .links
        .iter()
        .filter_map(fixture_ownership_check)
        .collect();
    ObserveOrchestrationCommand {
        write: lineage.write,
        delegation_id: lineage.delegation_id,
        expected_state_revision: lineage.expected_state_revision,
        resulting_state_revision: lineage.resulting_state_revision,
        observed_at: lineage.linked_at,
        references: lineage.links,
        ownership_checks,
        outbox_event: lineage.outbox_event,
    }
}

fn fixture_ownership_check(link: &NewAuthorityLink) -> Option<AuthorityOwnershipCheck> {
    let (owner_kind, expected_owner_id) = match link.target {
        AuthorityLinkTarget::Session { .. } | AuthorityLinkTarget::Job { .. } => {
            (AuthorityOwnerKind::Agent, "agent")
        }
        AuthorityLinkTarget::Run { .. } => (AuthorityOwnerKind::Session, "session"),
        AuthorityLinkTarget::JobRun { .. } => (AuthorityOwnerKind::Job, "job"),
        AuthorityLinkTarget::Task { .. } => (AuthorityOwnerKind::Project, "project"),
        AuthorityLinkTarget::BoardCard { .. } => (AuthorityOwnerKind::Board, "board"),
        AuthorityLinkTarget::MailMessage { .. } => (AuthorityOwnerKind::MailThread, "mail-thread"),
        AuthorityLinkTarget::ArtifactAttachment { .. } => (AuthorityOwnerKind::Message, "message"),
        AuthorityLinkTarget::ArtifactBoardCardAsset { .. } => {
            (AuthorityOwnerKind::BoardCard, "card")
        }
        AuthorityLinkTarget::ArtifactMailAttachment { .. } => {
            (AuthorityOwnerKind::MailMessage, "mail-message")
        }
        AuthorityLinkTarget::AssistantToolCallAudit { .. } => {
            (AuthorityOwnerKind::Session, "session")
        }
        AuthorityLinkTarget::ToolCall { .. } => (AuthorityOwnerKind::Run, "run"),
        AuthorityLinkTarget::Board { .. }
        | AuthorityLinkTarget::MailThread { .. }
        | AuthorityLinkTarget::SecurityAuditEvent { .. }
        | AuthorityLinkTarget::Unsupported { .. } => return None,
    };
    Some(AuthorityOwnershipCheck {
        link_id: link.link_id.clone(),
        owner_kind,
        expected_owner_id: expected_owner_id.into(),
    })
}

fn second_foundation() -> CreateFoundationCommand {
    let mut command = foundation();
    command.write.idempotency_key = "idem-foundation-2".into();
    command.write.correlation_id = "corr-foundation-2".into();
    command.write.causation_id = "cause-foundation-2".into();
    command.authority.authority_provenance_id = "authority-2".into();
    command.authority.source_correlation_id = "corr-foundation-2".into();
    command.authority.source_message_id = Some("message-2".into());
    command.delegation.delegation_id = "delegation-2".into();
    command.delegation.source_message_id = Some("message-2".into());
    command.delegation.source_correlation_id = "corr-foundation-2".into();
    command.delegation.ingress_idempotency_key = "idem-foundation-2".into();
    command.delegation.authority_provenance_id = "authority-2".into();
    command.plan.plan_id = "plan-delegation-2".into();
    command.plan.delegation_id = "delegation-2".into();
    command.plan.created_by_authority_provenance_id = "authority-2".into();
    for (index, criterion) in command.outcome_criteria.iter_mut().enumerate() {
        criterion.criterion_id = format!("criterion-delegation-2-{index}");
        criterion.delegation_id = "delegation-2".into();
    }
    command.initial_continuation = None;
    command.outbox_event.event_id = "event-foundation-2".into();
    command.outbox_event.aggregate_id = "delegation-2".into();
    command.outbox_event.correlation_id = "corr-foundation-2".into();
    command.outbox_event.causation_id = "cause-foundation-2".into();
    command.outbox_event.duplicate_identity = "idem-foundation-2".into();
    command
}

fn seed_security_event(paths: &crate::AppPaths, event_id: &str) {
    let conn = Connection::open(&paths.db_path).expect("open fixture db");
    conn.execute("PRAGMA foreign_keys = ON", []).unwrap();
    conn.execute(r#"INSERT INTO security_audit_events (event_id, request_id, correlation_id, principal, action, resource, decision, transport, status, created_at) VALUES (?1, 'req', 'corr', 'local', 'test', 'resource', 'allow', 'native', '200', 1)"#, params![event_id]).unwrap();
}

fn seed_all_authority_sources(paths: &crate::AppPaths) {
    let conn = Connection::open(&paths.db_path).expect("open fixture db");
    conn.execute_batch(
        r#"
        PRAGMA foreign_keys = ON;
        INSERT INTO agents (agent_id,name,workspace_root,model_provider,model_id,tool_profile,created_at,updated_at) VALUES ('agent','agent','.','test','test','default',1,1);
        INSERT INTO sessions (session_id,session_key,agent_id,created_at,updated_at) VALUES ('session','session-key','agent',1,1);
        INSERT INTO runs (run_id,session_id,status,model_provider,model_id,created_at) VALUES ('run','session','done','test','test',1);
        INSERT INTO tool_calls (tool_call_id,run_id,tool_name,args_json,status) VALUES ('tool','run','test','{}','done');
        INSERT INTO jobs (job_id,agent_id,name,enabled,schedule_kind,payload_json,max_retries,retry_backoff_ms,timeout_ms,created_at,updated_at) VALUES ('job','agent','job',1,'manual','{}',0,0,1,1,1);
        INSERT INTO job_runs (job_run_id,job_id,trigger_kind,status,attempt,created_at) VALUES ('job-run','job','manual','done',1,1);
        INSERT INTO goals (goal_id,slug,title,status,created_at,updated_at) VALUES ('goal','goal','goal','active',1,1);
        INSERT INTO projects (project_id,goal_id,slug,name,status,created_at,updated_at) VALUES ('project','goal','project','project','active',1,1);
        INSERT INTO boards (board_id,board_key,name,board_type,created_at,updated_at) VALUES ('board','board','board','project',1,1);
        INSERT INTO board_columns (column_id,board_id,column_key,name,position,created_at,updated_at) VALUES ('column','board','column','column',1,1,1);
        INSERT INTO board_cards (card_id,board_id,column_id,title,owner_kind,position,created_at,updated_at) VALUES ('card','board','column','card','agent',1,1,1);
        INSERT INTO tasks (task_id,project_id,title,status,priority,created_at,updated_at) VALUES ('task','project','task','open','normal',1,1);
        INSERT INTO messages (message_id,session_id,source_channel,role,content_text,content_format,created_at) VALUES ('message','session','test','user','body','plain',1);
        INSERT INTO attachments (attachment_id,message_id,kind,mime,sha256,bytes,local_path,created_at) VALUES ('attachment','message','file','text/plain','x',1,'safe',1);
        INSERT INTO board_card_assets (card_asset_id,card_id,filename,mime,sha256,bytes,local_path,created_at) VALUES ('board-asset','card','a','text/plain','x',1,'safe',1);
        INSERT INTO agent_mail_threads (thread_id,kind,subject,created_by_principal,created_at,updated_at) VALUES ('mail-thread','direct','subject','agent',1,1);
        INSERT INTO agent_mail_messages (message_id,thread_id,sender_principal,sender_kind,body_text,created_at) VALUES ('mail-message','mail-thread','agent','agent','body',1);
        INSERT INTO agent_mail_attachments (attachment_id,message_id,filename,mime,sha256,bytes,local_path,created_at) VALUES ('mail-attachment','mail-message','a','text/plain','x',1,'safe',1);
        INSERT INTO security_audit_events (event_id,request_id,correlation_id,principal,action,resource,decision,transport,status,created_at) VALUES ('security','req','corr','agent','test','resource','allow','native','200',1);
        INSERT INTO assistant_tool_calls_audit (event_id,request_id,boss_key,root_session_id,caller_agent_id,tool_name,decision,created_at) VALUES ('assistant-audit','req','boss','session','agent','test','allow',1);
        "#,
    ).unwrap();
}

fn seed_complete_reachability_graph(
    paths: &crate::AppPaths,
    insert_claim_receipt: bool,
    settle_actual: bool,
    settle_evidence_digest: &str,
) {
    let conn = Connection::open(&paths.db_path).expect("open fixture db");
    conn.execute_batch(
        r#"
        PRAGMA foreign_keys = ON;
        INSERT INTO execass_plans (
          plan_id, delegation_id, plan_revision, based_on_delegation_revision,
          policy_revision, plan_summary, resolved_leaf_manifest_json,
          manifest_digest, created_by_authority_provenance_id, created_at
        ) VALUES ('plan-2','delegation-1',2,1,1,'amended plan','[]','manifest-2','authority-1',2);
        INSERT INTO execass_plan_amendments (
          amendment_id, delegation_id, amendment_revision, superseded_plan_revision,
          resulting_plan_revision, normalized_amendment, intake_evidence_json,
          authority_provenance_id, created_at
        ) VALUES ('amendment-1','delegation-1',1,1,2,'bounded revision','{}','authority-1',2);
        INSERT INTO execass_verifier_results (
          verifier_result_id, delegation_id, criterion_id, result_revision, result,
          evidence_refs_json, evidence_digest, verifier_identity, verified_at
        ) VALUES ('verifier-1','delegation-1','criterion-a',1,'pass','[]','evidence','independent-db',2);
        INSERT INTO execass_decisions (
          decision_id, delegation_id, decision_revision, delegation_revision, plan_revision,
          policy_revision, decision_kind, status, exact_presented_action_json, confirmed_logical_action_identity, manifest_digest,
          payload_digest, payload_and_material_operands_json, target_audience_path_json, side_effect_envelope_json,
          recommendation, consequence, alternatives_json, idempotency_key, requested_at
        ) VALUES (
          'decision-1','delegation-1',1,1,1,1,
          'dangerous_action_confirmation','pending','{}','logical-action-1','manifest-1',
          'payload-1','{}','[]','{}','continue','bounded','[]','decision-idem-1',2
        );
        INSERT INTO execass_confirmation_challenges (
          challenge_id, decision_id, delegation_id, decision_revision,
          exact_presented_action_json, confirmed_logical_action_identity, manifest_digest, payload_digest,
          payload_and_material_operands_json,
          canonical_action_envelope_or_selector_json, declared_consequence,
          nonce_digest, status, created_at, expires_at
        ) VALUES ('challenge-1','decision-1','delegation-1',1,'{}','logical-action-1','manifest-1','payload-1','{}','{}','bounded','nonce-1','pending',2,200);
        INSERT INTO execass_logical_effects (
          logical_effect_id, delegation_id, continuation_id, action_kind, state,
          internal_idempotency_key, manifest_digest, payload_digest, created_at, updated_at
        ) VALUES (
          'effect-1','delegation-1','continuation-1',
          'read_only_local_inspection_and_bounded_reversible_local_work','succeeded',
          'effect-idem-1','manifest-1','payload-1',2,3
        );
        INSERT INTO execass_effect_tombstones (
          tombstone_id, delegation_id, logical_effect_id, internal_idempotency_key,
          terminal_state, outcome_digest, retained_at
        ) VALUES ('tombstone-1','delegation-1','effect-1','effect-idem-1','succeeded','outcome',3);
        INSERT INTO agents (
          agent_id,name,workspace_root,model_provider,model_id,tool_profile,created_at,updated_at
        ) VALUES ('lineage-agent','lineage-agent','.','test','test','default',1,1);
        INSERT INTO jobs (
          job_id,agent_id,name,enabled,schedule_kind,payload_json,max_retries,
          retry_backoff_ms,timeout_ms,created_at,updated_at
        ) VALUES ('lineage-job','lineage-agent','lineage-job',1,'manual','{}',0,0,1,1,1);
        UPDATE execass_continuations
        SET job_id='lineage-job'
        WHERE continuation_id='continuation-1';
        INSERT INTO execass_runtime_host_generations (
          generation,ownership_scope,state_root_generation,installation_identity,
          os_user_identity_digest,host_instance_id,started_at
        ) VALUES (1,'execass',1,'lineage-installation','lineage-user','lineage-host',1);
        INSERT INTO execass_outbox_events (
          event_id,event_name,aggregate_id,aggregate_revision,correlation_id,
          causation_id,occurred_at,schema_version,safe_payload_json,duplicate_identity
        ) VALUES (
          'claim-event-1','execass.v1.continuation.claimed_or_result_recorded',
          'delegation-1',1,'claim-correlation-1','claim-cause-1',2,'v1','{}','claim-idem-1'
        );
        INSERT INTO execass_technical_resource_quota_snapshots (
          quota_snapshot_id,delegation_id,policy_revision,effective_authority_digest,
          scope_key,canonical_entries_json,canonical_entries_digest,created_at
        ) VALUES (
          'quota-1','delegation-1',1,'authority-digest-1','delegation','[]','entries-digest-1',2
        );
        INSERT INTO execass_technical_resource_quota_entries (
          quota_snapshot_id,technical_resource_kind,unit,amount_limit
        ) VALUES
          ('quota-1','tokens','token',10),
          ('quota-1','time_ms','ms',10),
          ('quota-1','connector_calls','connector:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa',10),
          ('quota-1','resource_units','resource:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb',10);
        INSERT INTO execass_technical_resource_requirement_sets (
          requirement_set_id,quota_snapshot_id,delegation_id,logical_effect_id,action_id,
          manifest_digest,canonical_requirements_json,canonical_requirements_digest,created_at
        ) VALUES (
          'requirements-1','quota-1','delegation-1','effect-1','action-1',
          'manifest-1','[]','requirements-digest-1',2
        );
        INSERT INTO execass_technical_resource_requirements (
          requirement_set_id,quota_snapshot_id,technical_resource_kind,unit,amount_required
        ) VALUES
          ('requirements-1','quota-1','tokens','token',1),
          ('requirements-1','quota-1','time_ms','ms',1),
          ('requirements-1','quota-1','connector_calls','connector:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa',1),
          ('requirements-1','quota-1','resource_units','resource:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb',1);
        INSERT INTO execass_continuation_operation_history (
          event_id,claim_event_id,claim_receipt_id,operation,result_status,
          continuation_id,delegation_id,action_id,job_id,worker_id,job_lease_expires_at,
          continuation_fencing_token,runtime_host_generation,runtime_host_instance_id,
          runtime_fencing_token,state_root_generation,runtime_authority_provenance_id,
          runtime_actor_identity,policy_revision,global_stop_epoch,technical_quota_policy_digest,
          technical_quota_snapshot_id,technical_resource_reservation_set_json,
          technical_resource_reservation_set_digest,recorded_at
        ) VALUES (
          'claim-event-1','claim-event-1','receipt-1','claim','executing',
          'continuation-1','delegation-1','action-1','lineage-job','lineage-worker',200,
          1,1,'lineage-host',1,1,'authority-1','runtime-1',1,0,'quota-policy-digest-1',
          'quota-1',
          '[{"reservation_id":"reservation-1","quota_snapshot_id":"quota-1","logical_effect_id":"effect-1","technical_resource_kind":"connector_calls","unit":"connector:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","amount_reserved":1}]',
          'sha256:06f2d1466c27846b144311bf6776f64574320913eaf33e9e62cfe72edf19949e',2
        );
        INSERT INTO execass_technical_resource_reservations (
          reservation_id,delegation_id,logical_effect_id,quota_snapshot_id,continuation_id,
          claim_event_id,claim_receipt_id,technical_resource_kind,unit,amount_reserved,status,
          idempotency_key,continuation_fencing_token,runtime_host_generation,
          runtime_fencing_token,created_at,expires_at,settled_at
        ) VALUES (
          'reservation-1','delegation-1','effect-1','quota-1','continuation-1',
          'claim-event-1','receipt-1','connector_calls',
          'connector:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa',
          1,'reserved','budget-idem-1',1,1,1,2,200,NULL
        );
        INSERT INTO execass_provider_attempts (
          attempt_id,delegation_id,logical_effect_id,continuation_id,action_id,
          claim_event_id,claim_receipt_id,attempt_number,fencing_token,host_generation,
          host_instance_id,runtime_fencing_token,status,provider_request_digest,
          provider_response_digest,remote_effect_id,started_at,finished_at
        ) VALUES (
          'attempt-1','delegation-1','effect-1','continuation-1','action-1',
          'claim-event-1','receipt-1',1,1,1,'lineage-host',1,'succeeded','request',
          'response',NULL,2,3
        );
        INSERT INTO execass_technical_resource_actuals (
          technical_resource_actual_id,delegation_id,reservation_id,amount_actual,
          continuation_fencing_token,runtime_host_generation,runtime_fencing_token,
          evidence_digest,recorded_at
        ) VALUES ('actual-1','delegation-1','reservation-1',1,1,1,1,'budget-evidence',3);
        "#,
    )
    .expect("seed complete reachability graph");
    if insert_claim_receipt {
        conn.execute_batch(
            r#"
            INSERT INTO execass_receipts (
              receipt_id, delegation_id, receipt_sequence, global_sequence,
              causation_id, causation_event_id, actor_type,
              actor_identity, runtime_host_generation, state_revision, canonical_payload,
              serialization_version, hash_algorithm, key_id, key_generation,
              receipt_digest, keyed_integrity_tag, redacted_summary,
              occurred_at, committed_at
            ) VALUES (
              'receipt-1','delegation-1',1,1,'claim-cause-1','claim-event-1','runtime','runtime-1',1,1,X'01',
              'v1','sha256','key-1',1,'receipt-digest-1','tag-1','safe',2,2
            );
            "#,
        )
        .expect("seed claim receipt authority");
    }
    if settle_actual {
        assert!(insert_claim_receipt);
        conn.execute_batch(
            r#"
            INSERT INTO execass_outbox_events (
              event_id,event_name,aggregate_id,aggregate_revision,correlation_id,
              causation_id,occurred_at,schema_version,safe_payload_json,duplicate_identity
            ) VALUES (
              'settle-event-1','execass.v1.continuation.claimed_or_result_recorded',
              'delegation-1',1,'settle-correlation-1','settle-cause-1',3,'v1','{}','settle-idem-1'
            );
            INSERT INTO execass_receipts (
              receipt_id,delegation_id,receipt_sequence,global_sequence,
              causation_id,causation_event_id,
              parent_receipt_id,actor_type,actor_identity,runtime_host_generation,state_revision,
              canonical_payload,serialization_version,hash_algorithm,key_id,key_generation,
              receipt_digest,keyed_integrity_tag,redacted_summary,occurred_at,committed_at
            ) VALUES (
              'receipt-2','delegation-1',2,2,'settle-cause-1','settle-event-1','receipt-1',
              'runtime','runtime-1',1,1,X'02','v1','sha256','key-1',1,
              'receipt-digest-2','tag-2','safe',3,3
            );
            "#,
        )
        .expect("seed settle event and receipt authority");
        conn.execute(
            r#"INSERT INTO execass_continuation_operation_history (
              event_id,claim_event_id,claim_receipt_id,operation,result_status,
              continuation_id,delegation_id,action_id,job_id,worker_id,job_lease_expires_at,
              continuation_fencing_token,runtime_host_generation,runtime_host_instance_id,
              runtime_fencing_token,state_root_generation,runtime_authority_provenance_id,
              runtime_actor_identity,policy_revision,global_stop_epoch,technical_quota_policy_digest,
              technical_quota_snapshot_id,technical_resource_reservation_set_json,
              technical_resource_reservation_set_digest,technical_resource_evidence_digest,recorded_at
            ) VALUES (
              'settle-event-1','claim-event-1','receipt-1','settle','terminal',
              'continuation-1','delegation-1','action-1','lineage-job','lineage-worker',200,
              1,1,'lineage-host',1,1,'authority-1','runtime-1',1,0,'quota-policy-digest-1',
              'quota-1',
              '[{"reservation_id":"reservation-1","quota_snapshot_id":"quota-1","logical_effect_id":"effect-1","technical_resource_kind":"connector_calls","unit":"connector:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","amount_reserved":1}]',
              'sha256:06f2d1466c27846b144311bf6776f64574320913eaf33e9e62cfe72edf19949e',?1,3
            )"#,
            params![settle_evidence_digest],
        )
        .expect("seed immutable settle operation history");
        conn.execute(
            "UPDATE execass_technical_resource_reservations SET status='settled', settled_at=3 WHERE reservation_id='reservation-1'",
            [],
        )
        .expect("settle technical resource reservation");
    }
}

fn counts(paths: &crate::AppPaths) -> (i64, i64, i64) {
    let conn = Connection::open(&paths.db_path).unwrap();
    let state = conn
        .query_row(
            "SELECT state_revision FROM execass_delegations WHERE delegation_id='delegation-1'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let outbox = conn
        .query_row("SELECT COUNT(*) FROM execass_outbox_events", [], |row| {
            row.get(0)
        })
        .unwrap();
    let links = conn
        .query_row("SELECT COUNT(*) FROM execass_authority_links", [], |row| {
            row.get(0)
        })
        .unwrap();
    (state, outbox, links)
}

#[test]
fn all_fifteen_allowed_authority_kinds_resolve_only_safe_identity_projection() {
    let fixture = fixture();
    fixture.store.create_foundation(&foundation()).unwrap();
    seed_all_authority_sources(&fixture.paths);
    let targets = vec![
        AuthorityLinkTarget::Session {
            session_id: "session".into(),
        },
        AuthorityLinkTarget::Run {
            run_id: "run".into(),
        },
        AuthorityLinkTarget::Job {
            job_id: "job".into(),
        },
        AuthorityLinkTarget::JobRun {
            job_run_id: "job-run".into(),
        },
        AuthorityLinkTarget::Task {
            task_id: "task".into(),
        },
        AuthorityLinkTarget::Board {
            board_id: "board".into(),
        },
        AuthorityLinkTarget::BoardCard {
            board_card_id: "card".into(),
        },
        AuthorityLinkTarget::MailThread {
            mail_thread_id: "mail-thread".into(),
        },
        AuthorityLinkTarget::MailMessage {
            mail_message_id: "mail-message".into(),
        },
        AuthorityLinkTarget::ArtifactAttachment {
            attachment_id: "attachment".into(),
        },
        AuthorityLinkTarget::ArtifactBoardCardAsset {
            board_card_asset_id: "board-asset".into(),
        },
        AuthorityLinkTarget::ArtifactMailAttachment {
            mail_attachment_id: "mail-attachment".into(),
        },
        AuthorityLinkTarget::SecurityAuditEvent {
            event_id: "security".into(),
        },
        AuthorityLinkTarget::AssistantToolCallAudit {
            event_id: "assistant-audit".into(),
        },
        AuthorityLinkTarget::ToolCall {
            tool_call_id: "tool".into(),
        },
    ];
    let mut command = lineage(targets[0].clone(), "all");
    command.links = targets
        .into_iter()
        .enumerate()
        .map(|(index, target)| NewAuthorityLink {
            link_id: format!("all-link-{index}"),
            target,
        })
        .collect();
    let AuthorityLineageOutcome::Appended(appended) =
        fixture.store.append_authority_lineage(&command).unwrap()
    else {
        panic!("expected append")
    };
    assert_eq!(appended.links.len(), 15);
    assert!(appended
        .links
        .iter()
        .all(|link| link.authoritative_revision == 0 && link.reachable));
}

#[test]
fn production_adapter_reuses_every_supported_authority_without_copying_terminal_status() {
    let fixture = fixture();
    fixture.store.create_foundation(&foundation()).unwrap();
    seed_all_authority_sources(&fixture.paths);
    let targets = vec![
        AuthorityLinkTarget::Session {
            session_id: "session".into(),
        },
        AuthorityLinkTarget::Run {
            run_id: "run".into(),
        },
        AuthorityLinkTarget::Job {
            job_id: "job".into(),
        },
        AuthorityLinkTarget::JobRun {
            job_run_id: "job-run".into(),
        },
        AuthorityLinkTarget::Task {
            task_id: "task".into(),
        },
        AuthorityLinkTarget::Board {
            board_id: "board".into(),
        },
        AuthorityLinkTarget::BoardCard {
            board_card_id: "card".into(),
        },
        AuthorityLinkTarget::MailThread {
            mail_thread_id: "mail-thread".into(),
        },
        AuthorityLinkTarget::MailMessage {
            mail_message_id: "mail-message".into(),
        },
        AuthorityLinkTarget::ArtifactAttachment {
            attachment_id: "attachment".into(),
        },
        AuthorityLinkTarget::ArtifactBoardCardAsset {
            board_card_asset_id: "board-asset".into(),
        },
        AuthorityLinkTarget::ArtifactMailAttachment {
            mail_attachment_id: "mail-attachment".into(),
        },
        AuthorityLinkTarget::SecurityAuditEvent {
            event_id: "security".into(),
        },
        AuthorityLinkTarget::AssistantToolCallAudit {
            event_id: "assistant-audit".into(),
        },
        AuthorityLinkTarget::ToolCall {
            tool_call_id: "tool".into(),
        },
    ];
    let mut command = observation(targets[0].clone(), "adapter-all");
    command.references = targets
        .into_iter()
        .enumerate()
        .map(|(index, target)| NewAuthorityLink {
            link_id: format!("adapter-all-link-{index}"),
            target,
        })
        .collect();
    command.ownership_checks = command
        .references
        .iter()
        .filter_map(fixture_ownership_check)
        .collect();

    let OrchestrationObservationOutcome::Linked(linked) =
        fixture.store.observe_orchestration(&command).unwrap()
    else {
        panic!("adapter must link every supported authority")
    };
    assert_eq!(linked.links.len(), 15);
    assert_eq!(
        fixture
            .store
            .read_foundation("delegation-1")
            .unwrap()
            .unwrap()
            .delegation
            .phase,
        DelegationPhase::InMotion,
        "done runs and job runs cannot terminalize Delegation through the adapter"
    );
    let OrchestrationRereadOutcome::Current(reread) =
        fixture.store.reread_orchestration("delegation-1").unwrap()
    else {
        panic!("linked authorities must re-read")
    };
    assert_eq!(reread, linked.links);
}

#[test]
fn adapter_exact_replay_conflict_stale_and_missing_are_typed_and_atomic() {
    let fixture = fixture();
    fixture.store.create_foundation(&foundation()).unwrap();
    seed_security_event(&fixture.paths, "security-adapter-outcomes");
    let command = observation(
        AuthorityLinkTarget::SecurityAuditEvent {
            event_id: "security-adapter-outcomes".into(),
        },
        "adapter-outcomes",
    );
    let OrchestrationObservationOutcome::Linked(original) =
        fixture.store.observe_orchestration(&command).unwrap()
    else {
        panic!("initial observation must link")
    };
    assert_eq!(counts(&fixture.paths), (2, 2, 1));
    assert_eq!(
        fixture.store.observe_orchestration(&command).unwrap(),
        OrchestrationObservationOutcome::Replayed(original)
    );
    assert_eq!(counts(&fixture.paths), (2, 2, 1));

    let mut conflict = command.clone();
    conflict.references[0].link_id = "changed-member".into();
    assert!(matches!(
        fixture.store.observe_orchestration(&conflict).unwrap(),
        OrchestrationObservationOutcome::Conflict { .. }
    ));
    assert_eq!(counts(&fixture.paths), (2, 2, 1));

    let mut stale = observation(
        AuthorityLinkTarget::SecurityAuditEvent {
            event_id: "security-adapter-outcomes".into(),
        },
        "adapter-stale",
    );
    stale.expected_state_revision = 1;
    assert_eq!(
        fixture.store.observe_orchestration(&stale).unwrap(),
        OrchestrationObservationOutcome::Stale {
            current_state_revision: 2
        }
    );
    assert_eq!(counts(&fixture.paths), (2, 2, 1));

    let missing = observation(
        AuthorityLinkTarget::Run {
            run_id: "missing-run".into(),
        },
        "adapter-missing",
    );
    let mut missing = missing;
    missing.expected_state_revision = 2;
    missing.resulting_state_revision = 3;
    missing.outbox_event.aggregate_revision = 3;
    assert_eq!(
        fixture.store.observe_orchestration(&missing).unwrap(),
        OrchestrationObservationOutcome::MissingAuthority {
            kind: AuthorityLinkKind::Run,
            source_id: "missing-run".into()
        }
    );
    assert_eq!(counts(&fixture.paths), (2, 2, 1));
}

#[test]
fn adapter_allows_authoritative_evidence_to_be_shared_across_delegations() {
    let fixture = fixture();
    fixture.store.create_foundation(&foundation()).unwrap();
    fixture
        .store
        .create_foundation(&second_foundation())
        .unwrap();
    seed_security_event(&fixture.paths, "security-shared");
    fixture
        .store
        .observe_orchestration(&observation(
            AuthorityLinkTarget::SecurityAuditEvent {
                event_id: "security-shared".into(),
            },
            "shared-first",
        ))
        .unwrap();

    let mut second = observation(
        AuthorityLinkTarget::SecurityAuditEvent {
            event_id: "security-shared".into(),
        },
        "shared-second",
    );
    second.delegation_id = "delegation-2".into();
    second.outbox_event.aggregate_id = "delegation-2".into();
    assert!(matches!(
        fixture.store.observe_orchestration(&second).unwrap(),
        OrchestrationObservationOutcome::Linked(_)
    ));
    assert_eq!(
        fixture
            .store
            .read_foundation("delegation-2")
            .unwrap()
            .unwrap()
            .delegation
            .state_revision,
        2
    );
    let conn = Connection::open(&fixture.paths.db_path).unwrap();
    assert_eq!(
        conn.query_row(
            "SELECT COUNT(DISTINCT delegation_id) FROM execass_authority_links WHERE security_audit_event_id='security-shared'",
            [],
            |row| row.get::<_, i64>(0),
        )
        .unwrap(),
        2
    );
}

#[test]
fn adapter_rejects_mismatched_authoritative_parentage_with_zero_mutation() {
    let fixture = fixture();
    fixture.store.create_foundation(&foundation()).unwrap();
    seed_all_authority_sources(&fixture.paths);
    let command = observation(
        AuthorityLinkTarget::Run {
            run_id: "run".into(),
        },
        "wrong-run-owner",
    );
    assert!(matches!(
        fixture.store.observe_orchestration(&command).unwrap(),
        OrchestrationObservationOutcome::Linked(_)
    ));
    let before = counts(&fixture.paths);
    let mut replay_with_substituted_owner = command;
    replay_with_substituted_owner.ownership_checks[0].expected_owner_id =
        "substituted-session".into();
    assert_eq!(
        fixture
            .store
            .observe_orchestration(&replay_with_substituted_owner)
            .unwrap(),
        OrchestrationObservationOutcome::OwnershipMismatch {
            kind: AuthorityLinkKind::Run,
            source_id: "run".into(),
            expected_owner: "substituted-session".into(),
            actual_owner: Some("session".into())
        }
    );
    assert_eq!(counts(&fixture.paths), before);
}

#[test]
fn adapter_parentage_contract_is_closed_against_omitted_wrong_duplicate_and_orphan_checks() {
    for mutation in 0..4 {
        let fixture = fixture();
        fixture.store.create_foundation(&foundation()).unwrap();
        seed_all_authority_sources(&fixture.paths);
        let mut command = observation(
            AuthorityLinkTarget::Run {
                run_id: "run".into(),
            },
            &format!("ownership-shape-{mutation}"),
        );
        match mutation {
            0 => command.ownership_checks.clear(),
            1 => command.ownership_checks[0].owner_kind = AuthorityOwnerKind::Agent,
            2 => command
                .ownership_checks
                .push(command.ownership_checks[0].clone()),
            _ => command.ownership_checks[0].link_id = "orphan-link".into(),
        }
        assert!(fixture.store.observe_orchestration(&command).is_err());
        assert_eq!(counts(&fixture.paths), (1, 1, 0));
    }
}

#[test]
fn adapter_reread_surfaces_authoritative_reparenting_as_typed_conflict() {
    let fixture = fixture();
    fixture.store.create_foundation(&foundation()).unwrap();
    seed_all_authority_sources(&fixture.paths);
    let command = observation(
        AuthorityLinkTarget::Run {
            run_id: "run".into(),
        },
        "run-reparent",
    );
    fixture.store.observe_orchestration(&command).unwrap();
    let conn = Connection::open(&fixture.paths.db_path).unwrap();
    assert_eq!(
        conn.query_row(
            "SELECT owner_kind,expected_owner_id FROM execass_authority_parent_bindings WHERE link_id='link-run-reparent'",
            [],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
        .unwrap(),
        ("session".into(), "session".into())
    );
    assert!(conn
        .execute(
            "UPDATE execass_authority_parent_bindings SET expected_owner_id='forged' WHERE link_id='link-run-reparent'",
            [],
        )
        .is_err());
    assert!(conn
        .execute(
            "DELETE FROM execass_authority_parent_bindings WHERE link_id='link-run-reparent'",
            [],
        )
        .is_err());
    conn.execute(
        "INSERT INTO sessions (session_id,session_key,agent_id,created_at,updated_at) VALUES ('session-reparented','session-reparented-key','agent',2,2)",
        [],
    )
    .unwrap();
    conn.execute(
        "UPDATE runs SET session_id='session-reparented' WHERE run_id='run'",
        [],
    )
    .unwrap();
    drop(conn);
    assert_eq!(
        fixture.store.reread_orchestration("delegation-1").unwrap(),
        OrchestrationRereadOutcome::OwnershipMismatch {
            kind: AuthorityLinkKind::Run,
            source_id: "run".into(),
            expected_owner: "session".into(),
            actual_owner: Some("session-reparented".into())
        }
    );
}

#[test]
fn append_lineage_is_atomic_replay_safe_and_resolves_after_security_retention() {
    let fixture = fixture();
    fixture.store.create_foundation(&foundation()).unwrap();
    seed_security_event(&fixture.paths, "security-1");
    let command = lineage(
        AuthorityLinkTarget::SecurityAuditEvent {
            event_id: "security-1".into(),
        },
        "security",
    );
    let AuthorityLineageOutcome::Appended(first) =
        fixture.store.append_authority_lineage(&command).unwrap()
    else {
        panic!("expected append")
    };
    assert_eq!(first.resulting_state_revision, 2);
    assert_eq!(first.links[0].authoritative_revision, 0);
    assert!(matches!(
        fixture.store.append_authority_lineage(&command).unwrap(),
        AuthorityLineageOutcome::Replayed(_)
    ));
    let storage = Storage::from_paths(&fixture.paths);
    assert_eq!(storage.archive_security_audit_events_before(2).unwrap(), 1);
    assert_eq!(storage.delete_security_audit_events_before(2).unwrap(), 1);
    let resolved = fixture
        .store
        .resolve_authority_lineage("delegation-1")
        .unwrap();
    assert_eq!(resolved[0].location, AuthoritySourceLocation::Archived);
}

#[test]
fn stale_missing_and_unsupported_lineage_are_distinct_and_leave_no_outbox() {
    let fixture = fixture();
    fixture.store.create_foundation(&foundation()).unwrap();
    for (index, kind) in [
        UnsupportedAuthorityKind::Team,
        UnsupportedAuthorityKind::Project,
        UnsupportedAuthorityKind::Goal,
        UnsupportedAuthorityKind::BoardColumn,
        UnsupportedAuthorityKind::GenericMessage,
        UnsupportedAuthorityKind::MailRecipient,
        UnsupportedAuthorityKind::FileLease,
    ]
    .into_iter()
    .enumerate()
    {
        let unsupported = lineage(
            AuthorityLinkTarget::Unsupported { kind },
            &format!("unsupported-{index}"),
        );
        let error = fixture
            .store
            .append_authority_lineage(&unsupported)
            .unwrap_err();
        assert!(error.downcast_ref::<AuthorityLineageError>().is_some());
        assert_eq!(counts(&fixture.paths), (1, 1, 0));
    }
    let missing = lineage(
        AuthorityLinkTarget::SecurityAuditEvent {
            event_id: "missing".into(),
        },
        "missing",
    );
    assert!(fixture.store.append_authority_lineage(&missing).is_err());
    assert_eq!(counts(&fixture.paths), (1, 1, 0));
    seed_security_event(&fixture.paths, "security-stale");
    let mut stale = lineage(
        AuthorityLinkTarget::SecurityAuditEvent {
            event_id: "security-stale".into(),
        },
        "stale",
    );
    stale.expected_state_revision = 0;
    stale.resulting_state_revision = 1;
    stale.outbox_event.aggregate_revision = 1;
    assert!(matches!(
        fixture.store.append_authority_lineage(&stale).unwrap(),
        AuthorityLineageOutcome::Stale {
            current_state_revision: 1
        }
    ));
    assert_eq!(counts(&fixture.paths), (1, 1, 0));
}

#[test]
fn production_adapter_reports_the_absent_team_authority_without_inventing_storage() {
    let fixture = fixture();
    fixture.store.create_foundation(&foundation()).unwrap();
    let unsupported = observation(
        AuthorityLinkTarget::Unsupported {
            kind: UnsupportedAuthorityKind::Team,
        },
        "adapter-team",
    );
    assert_eq!(
        fixture.store.observe_orchestration(&unsupported).unwrap(),
        OrchestrationObservationOutcome::UnsupportedAuthority {
            kind: UnsupportedAuthorityKind::Team
        }
    );
    assert_eq!(counts(&fixture.paths), (1, 1, 0));
}

#[test]
fn restart_keeps_the_same_delegation_and_complete_lineage_after_terminal_revision() {
    let fixture = fixture();
    fixture.store.create_foundation(&foundation()).unwrap();
    seed_security_event(&fixture.paths, "security-restart");
    fixture
        .store
        .append_authority_lineage(&lineage(
            AuthorityLinkTarget::SecurityAuditEvent {
                event_id: "security-restart".into(),
            },
            "restart",
        ))
        .unwrap();
    let terminal = CasDelegationStateCommand {
        write: WriteContext {
            idempotency_key: "terminal-idem".into(),
            correlation_id: "terminal-corr".into(),
            causation_id: "terminal-cause".into(),
            occurred_at: 1_800_000_020_000,
        },
        delegation_id: "delegation-1".into(),
        expected_state_revision: 2,
        new_state_revision: 3,
        phase: DelegationPhase::Completed,
        run_control: RunControlState::Stopped,
        pending_decision_id: None,
        external_wait_json: None,
        updated_at: 1_800_000_020_000,
        terminal_at: Some(1_800_000_020_000),
        outbox_event: NewOutboxEvent {
            event_id: "terminal-event".into(),
            event_name: OutboxEventName::DelegationTransitioned,
            aggregate_id: "delegation-1".into(),
            aggregate_revision: 3,
            correlation_id: "terminal-corr".into(),
            causation_id: "terminal-cause".into(),
            occurred_at: 1_800_000_020_000,
            safe_payload_json: "{}".into(),
            duplicate_identity: "terminal-idem".into(),
        },
    };
    fixture
        .store
        .compare_and_swap_delegation_state(&terminal)
        .unwrap();
    let reopened = ExecAssStore::open(&fixture.paths).unwrap();
    let foundation = reopened.read_foundation("delegation-1").unwrap().unwrap();
    assert_eq!(foundation.delegation.delegation_id, "delegation-1");
    assert_eq!(foundation.delegation.state_revision, 3);
    assert_eq!(
        reopened
            .resolve_authority_lineage("delegation-1")
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn replay_after_later_progress_returns_the_exact_immutable_append_result() {
    let fixture = fixture();
    fixture.store.create_foundation(&foundation()).unwrap();
    seed_security_event(&fixture.paths, "security-progress-replay");
    let command = lineage(
        AuthorityLinkTarget::SecurityAuditEvent {
            event_id: "security-progress-replay".into(),
        },
        "progress-replay",
    );
    let AuthorityLineageOutcome::Appended(original) =
        fixture.store.append_authority_lineage(&command).unwrap()
    else {
        panic!("expected original append")
    };
    let terminal = CasDelegationStateCommand {
        write: WriteContext {
            idempotency_key: "progress-terminal-idem".into(),
            correlation_id: "progress-terminal-corr".into(),
            causation_id: "progress-terminal-cause".into(),
            occurred_at: 1_800_000_030_000,
        },
        delegation_id: "delegation-1".into(),
        expected_state_revision: 2,
        new_state_revision: 3,
        phase: DelegationPhase::Completed,
        run_control: RunControlState::Stopped,
        pending_decision_id: None,
        external_wait_json: None,
        updated_at: 1_800_000_030_000,
        terminal_at: Some(1_800_000_030_000),
        outbox_event: NewOutboxEvent {
            event_id: "progress-terminal-event".into(),
            event_name: OutboxEventName::DelegationTransitioned,
            aggregate_id: "delegation-1".into(),
            aggregate_revision: 3,
            correlation_id: "progress-terminal-corr".into(),
            causation_id: "progress-terminal-cause".into(),
            occurred_at: 1_800_000_030_000,
            safe_payload_json: "{}".into(),
            duplicate_identity: "progress-terminal-idem".into(),
        },
    };
    fixture
        .store
        .compare_and_swap_delegation_state(&terminal)
        .unwrap();
    let AuthorityLineageOutcome::Replayed(replayed) =
        fixture.store.append_authority_lineage(&command).unwrap()
    else {
        panic!("expected exact replay")
    };
    assert_eq!(replayed, original);
    assert_eq!(original.resulting_state_revision, 2);
}

#[test]
fn causally_impossible_revisions_and_continuations_never_validate() {
    let fixture = fixture();
    fixture.store.create_foundation(&foundation()).unwrap();
    let conn = Connection::open(&fixture.paths.db_path).unwrap();
    conn.execute(
        r#"INSERT INTO execass_plans (
          plan_id,delegation_id,plan_revision,based_on_delegation_revision,policy_revision,
          plan_summary,resolved_leaf_manifest_json,manifest_digest,
          created_by_authority_provenance_id,created_at
        ) VALUES ('impossible-plan','delegation-1',2,999,1,'bad','[]','bad-manifest','authority-1',2)"#,
        [],
    )
    .unwrap();
    conn.execute(
        r#"INSERT INTO execass_action_branches (
          action_id,delegation_id,action_revision,target_delegation_revision,target_plan_revision,
          stop_epoch,branch_kind,status,action_summary,created_at,updated_at
        ) VALUES ('missing-decision-action','delegation-1',2,1,1,0,'ordinary','waiting','bad',2,2)"#,
        [],
    )
    .unwrap();
    conn.execute(
        r#"INSERT INTO execass_continuations (
          continuation_id,delegation_id,target_delegation_revision,target_plan_revision,
          action_id,branch_kind,causation_kind,causation_id,status,fencing_token,host_generation,stop_epoch,global_stop_epoch,
          created_at,updated_at
        ) VALUES ('missing-decision-continuation','delegation-1',1,1,'missing-decision-action','ordinary','decision',
          'missing-decision','waiting',0,1,0,0,2,2)"#,
        [],
    )
    .unwrap();
    conn.execute(
        r#"INSERT INTO execass_verifier_results (
          verifier_result_id,delegation_id,criterion_id,result_revision,result,
          evidence_refs_json,evidence_digest,verifier_identity,verified_at
        ) VALUES ('gapped-verifier','delegation-1','criterion-a',2,'pass','[]','e','v',2)"#,
        [],
    )
    .unwrap();
    conn.execute(
        r#"INSERT INTO execass_receipts (
          receipt_id,delegation_id,receipt_sequence,causation_id,causation_event_id,actor_type,actor_identity,
          runtime_host_generation,state_revision,canonical_payload,serialization_version,
          hash_algorithm,key_id,key_generation,receipt_digest,keyed_integrity_tag,
          redacted_summary,occurred_at,committed_at
        ) VALUES ('bad-receipt','delegation-1',1,'cause-foundation-1','event-foundation-1','runtime','runtime',
          1,999,X'01','v1','sha256','key',1,'bad-digest','bad-tag','safe',2,2)"#,
        [],
    )
    .unwrap();
    conn.execute(
        "UPDATE execass_delegations SET state_revision=3, updated_at=1800000000003 WHERE delegation_id='delegation-1'",
        [],
    )
    .unwrap();
    drop(conn);
    let DelegationReachabilityOutcome::Invalid(report) = fixture
        .store
        .validate_delegation_reachability("delegation-1")
        .unwrap()
    else {
        panic!("causally impossible graph must be invalid")
    };
    assert!(report
        .violations
        .contains(&"plan:impossible-plan:authority_or_revision".into()));
    assert!(report
        .violations
        .contains(&"continuation:missing-decision-continuation:plan_revision_or_causation".into()));
    assert!(report
        .violations
        .contains(&"verifier:gapped-verifier:criterion_or_revision".into()));
    assert!(report
        .violations
        .contains(&"receipt:bad-receipt:parent_or_causation".into()));
    assert!(report
        .violations
        .contains(&"delegation:delegation-1:transition_revision_set".into()));
}

#[test]
fn a_delegation_revision_has_exactly_one_canonical_transition_event() {
    let fixture = fixture();
    fixture.store.create_foundation(&foundation()).unwrap();
    let conn = Connection::open(&fixture.paths.db_path).unwrap();
    assert!(conn
        .execute(
            r#"INSERT INTO execass_outbox_events (
              event_id,event_name,aggregate_id,aggregate_revision,correlation_id,
              causation_id,occurred_at,schema_version,safe_payload_json,duplicate_identity
            ) VALUES ('forged-transition','execass.v1.delegation.transitioned',
              'delegation-1',1,'forged-correlation','forged-causation',2,'v1','{}',
              'forged-transition-idempotency')"#,
            [],
        )
        .is_err());
    drop(conn);
    assert!(matches!(
        fixture
            .store
            .validate_delegation_reachability("delegation-1")
            .unwrap(),
        DelegationReachabilityOutcome::Valid(_)
    ));
}

#[test]
fn replay_requires_exact_immutable_members_but_ignores_member_order() {
    let fixture = fixture();
    fixture.store.create_foundation(&foundation()).unwrap();
    seed_security_event(&fixture.paths, "security-replay-a");
    seed_security_event(&fixture.paths, "security-replay-b");
    let mut command = lineage(
        AuthorityLinkTarget::SecurityAuditEvent {
            event_id: "security-replay-a".into(),
        },
        "exact-replay",
    );
    command.links.push(NewAuthorityLink {
        link_id: "second-link".into(),
        target: AuthorityLinkTarget::SecurityAuditEvent {
            event_id: "security-replay-b".into(),
        },
    });
    fixture.store.append_authority_lineage(&command).unwrap();
    let mut reordered = command.clone();
    reordered.links.reverse();
    assert!(matches!(
        fixture.store.append_authority_lineage(&reordered).unwrap(),
        AuthorityLineageOutcome::Replayed(_)
    ));
    let mut changed_id = command;
    changed_id.links[0].link_id = "different-immutable-link-id".into();
    assert!(matches!(
        fixture.store.append_authority_lineage(&changed_id).unwrap(),
        AuthorityLineageOutcome::Conflict { .. }
    ));
}

#[test]
fn complete_sorted_reachability_inventory_survives_restart_without_replacement_delegation() {
    let fixture = fixture();
    fixture.store.create_foundation(&foundation()).unwrap();
    seed_complete_reachability_graph(
        &fixture.paths,
        true,
        true,
        "sha256:ae5cd1f8905c6f242ed8896e4946a42ee89efd7dd504479147961d3103ce5a7b",
    );
    seed_security_event(&fixture.paths, "security-graph");
    fixture
        .store
        .append_authority_lineage(&lineage(
            AuthorityLinkTarget::SecurityAuditEvent {
                event_id: "security-graph".into(),
            },
            "graph",
        ))
        .unwrap();
    let reopened = ExecAssStore::open(&fixture.paths).unwrap();
    let DelegationReachabilityOutcome::Valid(report) = reopened
        .validate_delegation_reachability("delegation-1")
        .unwrap()
    else {
        panic!("complete graph must be reachable")
    };
    assert_eq!(report.delegation_state_revision, 2);
    assert_eq!(report.authority_provenance.len(), 1);
    assert_eq!(
        report
            .plans
            .iter()
            .map(|record| (record.record_id.as_str(), record.revision))
            .collect::<Vec<_>>(),
        vec![("plan-1", 1), ("plan-2", 2)]
    );
    assert_eq!(report.plan_amendments.len(), 1);
    assert_eq!(report.outcome_criteria.len(), 2);
    assert_eq!(report.verifier_results.len(), 1);
    assert_eq!(report.decisions.len(), 1);
    assert_eq!(report.continuations.len(), 1);
    assert_eq!(
        report.continuation_operation_history,
        vec![
            ReachabilityRecordRef {
                record_id: "claim-event-1".into(),
                revision: 1,
            },
            ReachabilityRecordRef {
                record_id: "settle-event-1".into(),
                revision: 1,
            },
        ]
    );
    assert_eq!(report.logical_effects.len(), 1);
    assert_eq!(report.provider_attempts.len(), 1);
    assert_eq!(report.effect_tombstones.len(), 1);
    assert_eq!(report.confirmation_challenges.len(), 1);
    assert_eq!(report.technical_resource_quota_snapshots.len(), 1);
    assert_eq!(report.technical_resource_quota_entries.len(), 4);
    assert_eq!(report.technical_resource_requirement_sets.len(), 1);
    assert_eq!(report.technical_resource_requirements.len(), 4);
    assert_eq!(report.technical_resource_reservations.len(), 1);
    assert_eq!(
        report.technical_resource_actuals,
        vec![ReachabilityRecordRef {
            record_id: "actual-1".into(),
            revision: 0,
        }]
    );
    assert_eq!(report.receipts.len(), 2);
    assert_eq!(report.outbox_events.len(), 4);
    assert_eq!(report.authority_links.len(), 1);
    assert!(report.violations.is_empty());
    let conn = Connection::open(&fixture.paths.db_path).unwrap();
    assert_eq!(
        conn.query_row("SELECT COUNT(*) FROM execass_delegations", [], |row| row
            .get::<_, i64>(0))
            .unwrap(),
        2
    );
}

#[test]
fn trigger_permitted_accounting_gaps_are_reported_as_reachability_violations() {
    let fixture = fixture();
    fixture.store.create_foundation(&foundation()).unwrap();
    seed_complete_reachability_graph(&fixture.paths, false, false, "unused");

    let DelegationReachabilityOutcome::Invalid(report) = fixture
        .store
        .validate_delegation_reachability("delegation-1")
        .unwrap()
    else {
        panic!("missing claim receipt and unsettled actual must not validate")
    };
    assert_eq!(
        report.violations,
        vec![
            "continuation_operation_history:claim-event-1:authority_or_accounting_reference",
            "technical_resource_actual:actual-1:reservation_provenance",
            "technical_resource_reservation:reservation-1:accounting_provenance",
        ]
    );
}

#[test]
fn settle_history_evidence_digest_must_match_the_immutable_actual_set() {
    let fixture = fixture();
    fixture.store.create_foundation(&foundation()).unwrap();
    seed_complete_reachability_graph(
        &fixture.paths,
        true,
        true,
        "sha256:0000000000000000000000000000000000000000000000000000000000000000",
    );

    let DelegationReachabilityOutcome::Invalid(report) = fixture
        .store
        .validate_delegation_reachability("delegation-1")
        .unwrap()
    else {
        panic!("settle evidence must be bound to the immutable actual set")
    };
    assert_eq!(
        report.violations,
        vec!["continuation_operation_history:settle-event-1:evidence_digest"]
    );
}

#[test]
fn detached_subordinate_rows_are_rejected_by_the_operation_connection_before_projection() {
    let fixture = fixture();
    fixture.store.create_foundation(&foundation()).unwrap();
    let conn = Connection::open(&fixture.paths.db_path).unwrap();
    conn.execute_batch(
        r#"
        PRAGMA foreign_keys = OFF;
        INSERT INTO execass_effect_tombstones (
          tombstone_id, delegation_id, logical_effect_id, internal_idempotency_key,
          terminal_state, retained_at
        ) VALUES ('orphan-tombstone','delegation-1','missing-effect-2','orphan-key','failed',1);
        "#,
    )
    .unwrap();
    assert!(conn
        .execute(
            r#"INSERT INTO execass_technical_resource_actuals (
              technical_resource_actual_id,delegation_id,reservation_id,amount_actual,
              continuation_fencing_token,runtime_host_generation,runtime_fencing_token,
              evidence_digest,recorded_at
            ) VALUES ('orphan-actual','delegation-1','missing-reservation',0,1,1,1,'evidence',1)"#,
            [],
        )
        .is_err());
    drop(conn);
    let error = fixture
        .store
        .validate_delegation_reachability("delegation-1")
        .unwrap_err();
    assert!(error
        .to_string()
        .contains("ExecAss schema identity changed after store construction"));
}

#[test]
fn duplicate_observation_and_outbox_failure_roll_back_state_outbox_and_links() {
    let fixture = fixture();
    fixture.store.create_foundation(&foundation()).unwrap();
    seed_security_event(&fixture.paths, "security-a");
    seed_security_event(&fixture.paths, "security-b");
    fixture
        .store
        .append_authority_lineage(&lineage(
            AuthorityLinkTarget::SecurityAuditEvent {
                event_id: "security-a".into(),
            },
            "first",
        ))
        .unwrap();
    assert_eq!(counts(&fixture.paths), (2, 2, 1));

    let mut duplicate = lineage(
        AuthorityLinkTarget::SecurityAuditEvent {
            event_id: "security-a".into(),
        },
        "duplicate-source",
    );
    duplicate.expected_state_revision = 2;
    duplicate.resulting_state_revision = 3;
    duplicate.outbox_event.aggregate_revision = 3;
    assert!(fixture.store.append_authority_lineage(&duplicate).is_err());
    assert_eq!(counts(&fixture.paths), (2, 2, 1));

    let mut outbox_collision = lineage(
        AuthorityLinkTarget::SecurityAuditEvent {
            event_id: "security-b".into(),
        },
        "outbox-collision",
    );
    outbox_collision.expected_state_revision = 2;
    outbox_collision.resulting_state_revision = 3;
    outbox_collision.outbox_event.aggregate_revision = 3;
    outbox_collision.outbox_event.event_id = "event-foundation-1".into();
    assert!(fixture
        .store
        .append_authority_lineage(&outbox_collision)
        .is_err());
    assert_eq!(counts(&fixture.paths), (2, 2, 1));
}

#[test]
fn raw_link_guards_reject_multiple_mismatched_gapped_mutable_or_deleted_lineage() {
    let fixture = fixture();
    fixture.store.create_foundation(&foundation()).unwrap();
    seed_all_authority_sources(&fixture.paths);
    let command = lineage(
        AuthorityLinkTarget::Session {
            session_id: "session".into(),
        },
        "raw-guards",
    );
    fixture.store.append_authority_lineage(&command).unwrap();
    let conn = Connection::open(&fixture.paths.db_path).unwrap();
    let insert =
        |link_id: &str, revision: i64, kind: &str, session: Option<&str>, run: Option<&str>| {
            conn.execute(
                r#"INSERT INTO execass_authority_links (
              link_id, delegation_id, link_revision, delegation_state_revision,
              correlation_id, causation_id, outbox_event_id, authority_kind,
              session_id, run_id, authoritative_revision, linked_at
            ) VALUES (?1,'delegation-1',?2,2,'lineage-corr-raw-guards',
              'lineage-cause-raw-guards','lineage-event-raw-guards',?3,?4,?5,0,1800000010000)"#,
                params![link_id, revision, kind, session, run],
            )
        };
    assert!(insert("multiple", 2, "session", Some("session"), Some("run")).is_err());
    assert!(insert("mismatch", 2, "session", None, Some("run")).is_err());
    assert!(insert("missing", 2, "session", None, None).is_err());
    assert!(insert("gap", 3, "run", None, Some("run")).is_err());
    conn.execute(
        r#"INSERT INTO execass_outbox_events (
          event_id,event_name,aggregate_id,aggregate_revision,correlation_id,
          causation_id,occurred_at,schema_version,safe_payload_json,duplicate_identity
        ) VALUES ('wrong-event','execass.v1.summary.changed','delegation-1',2,
          'wrong-corr','wrong-cause',1800000010000,'v1','{}','wrong-event-idem')"#,
        [],
    )
    .unwrap();
    assert!(conn
        .execute(
            r#"INSERT INTO execass_authority_links (
              link_id,delegation_id,link_revision,delegation_state_revision,
              correlation_id,causation_id,outbox_event_id,authority_kind,run_id,
              authoritative_revision,linked_at
            ) VALUES ('wrong-event-link','delegation-1',2,2,'wrong-corr','wrong-cause',
              'wrong-event','run','run',0,1800000010000)"#,
            [],
        )
        .is_err());
    assert!(conn
        .execute(
            r#"INSERT INTO execass_authority_links (
              link_id,delegation_id,link_revision,delegation_state_revision,
              correlation_id,causation_id,outbox_event_id,authority_kind,run_id,
              authoritative_revision,linked_at
            ) VALUES ('wrong-time-link','delegation-1',2,2,
              'lineage-corr-raw-guards','lineage-cause-raw-guards',
              'lineage-event-raw-guards','run','run',0,1800000010001)"#,
            [],
        )
        .is_err());
    assert!(conn
        .execute(
            "UPDATE execass_authority_links SET linked_at=linked_at+1 WHERE link_id='link-raw-guards'",
            [],
        )
        .is_err());
    assert!(conn
        .execute(
            "DELETE FROM execass_authority_links WHERE link_id='link-raw-guards'",
            [],
        )
        .is_err());
    assert_eq!(counts(&fixture.paths), (2, 3, 1));
}

#[test]
fn exact_assistant_audit_identity_is_returned_read_and_linked() {
    let fixture = fixture();
    fixture.store.create_foundation(&foundation()).unwrap();
    let storage = Storage::from_paths(&fixture.paths);
    let audit = storage
        .create_assistant_tool_call_audit(NewAssistantToolCallAudit {
            request_id: "assistant-request".into(),
            boss_key: "boss".into(),
            root_session_id: "root-session".into(),
            root_run_id: None,
            caller_agent_id: "agent".into(),
            tool_name: "bounded-test".into(),
            decision: "allow".into(),
            reason_code: None,
            audit_ref: None,
            metadata_json: None,
        })
        .unwrap();
    assert_eq!(
        storage
            .get_assistant_tool_call_audit(&audit.event_id)
            .unwrap(),
        Some(audit.clone())
    );
    let AuthorityLineageOutcome::Appended(appended) = fixture
        .store
        .append_authority_lineage(&lineage(
            AuthorityLinkTarget::AssistantToolCallAudit {
                event_id: audit.event_id.clone(),
            },
            "assistant-exact",
        ))
        .unwrap()
    else {
        panic!("exact returned assistant audit must link")
    };
    assert_eq!(appended.links[0].source_id, audit.event_id);
}

#[test]
fn lineage_production_modules_do_not_query_retired_or_replacement_authorities() {
    let production_sources = [
        include_str!("aggregate.rs"),
        include_str!("foundation.rs"),
        include_str!("lineage.rs"),
        include_str!("orchestration.rs"),
        include_str!("rows.rs"),
        include_str!("store.rs"),
        include_str!("validation.rs"),
    ]
    .join("\n")
    .to_ascii_lowercase();
    for forbidden in [
        "from approvals",
        "join approvals",
        "assistant_workers",
        "assistant_task_links",
        "create table",
    ] {
        assert!(
            !production_sources.contains(forbidden),
            "retired authority token leaked into ExecAss production module: {forbidden}"
        );
    }
}
