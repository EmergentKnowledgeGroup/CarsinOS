use carsinos_storage::execass::*;
use carsinos_storage::{init_execass_fresh_root, AppPaths};
use rusqlite::Connection;
use std::process::Command;

fn verifier() -> &'static str {
    env!("CARGO_BIN_EXE_carsinos-receipt-integrity")
}

fn run(action: &str, paths: &AppPaths) -> std::process::Output {
    Command::new(verifier())
        .args([action, "--state-root"])
        .arg(&paths.root)
        .output()
        .expect("run receipt-integrity verifier")
}

fn seed_typed_receipt(paths: &AppPaths) {
    Connection::open(&paths.db_path)
        .unwrap()
        .execute_batch(
            r#"
            PRAGMA foreign_keys=ON;
            INSERT INTO agents(agent_id,name,workspace_root,model_provider,model_id,tool_profile,created_at,updated_at)
              VALUES('cli-agent','cli agent','.','test','test','default',1,1);
            INSERT INTO sessions(session_id,session_key,agent_id,created_at,updated_at)
              VALUES('cli-session','cli-session-key','cli-agent',1,1);
            INSERT INTO execass_authority_provenance(
              authority_provenance_id,actor_type,credential_identity,authenticated_ingress,
              channel_assurance,source_correlation_id,source_message_id,authority_kind,
              normalized_scope_json,policy_revision,bound_decision_id,bound_decision_revision,
              bound_manifest_digest,bound_challenge_nonce_digest,evidence_digest,created_at,expires_at
            ) VALUES('cli-authority','human_local','local-operator','native-control','interactive-local',
              'cli-correlation','cli-message','original_request','{}',1,NULL,NULL,NULL,NULL,'sha256:authority',1800000000000,NULL);
            INSERT INTO execass_delegations(
              delegation_id,normalized_original_intent,intake_evidence_json,ingress_source,
              ingress_credential_identity,source_message_id,source_correlation_id,
              ingress_idempotency_key,classifier_version,classifier_reasons_json,phase,run_control,
              state_revision,current_plan_revision,current_criteria_revision,policy_revision,
              effective_authority_json,authority_provenance_id,pending_decision_id,external_wait_json,
              stop_epoch,completion_assessment_json,receipt_chain_count,receipt_chain_head_digest,
              created_at,updated_at,acknowledged_at,terminal_at
            ) VALUES('cli-delegation','typed verifier proof','{}','native-control','local-operator',
              'cli-message','cli-correlation','cli-idempotency','v1','["durable_work"]','in_motion',
              'running',1,NULL,NULL,1,'{}','cli-authority',NULL,NULL,0,NULL,0,NULL,
              1800000000000,1800000000000,NULL,NULL);
            INSERT INTO execass_outbox_events(
              event_id,event_name,aggregate_id,aggregate_revision,correlation_id,causation_id,
              occurred_at,schema_version,safe_payload_json,duplicate_identity
            ) VALUES('cli-event','execass.v1.delegation.transitioned','cli-delegation',1,
              'cli-correlation','cli-cause',1800000000000,'v1','{}','cli-event-identity');
            INSERT INTO execass_runtime_host_generations(
              generation,ownership_scope,state_root_generation,installation_identity,
              os_user_identity_digest,host_instance_id,started_at
            ) VALUES(1,'execass',1,'cli-installation','cli-user-digest','cli-host',1);
            INSERT INTO execass_runtime_host_leases(
              lease_id,ownership_scope,generation,host_instance_id,fencing_token,acquired_at,expires_at
            ) VALUES('cli-lease','execass',1,'cli-host',1,1,9999999999999);
            INSERT INTO execass_authority_links(
              link_id,delegation_id,link_revision,delegation_state_revision,correlation_id,
              causation_id,outbox_event_id,authority_kind,session_id,authoritative_revision,linked_at
            ) VALUES('cli-link','cli-delegation',1,1,'cli-correlation','cli-cause','cli-event',
              'session','cli-session',0,1800000000000);
            "#,
        )
        .unwrap();
    let integrity = ReceiptIntegrityStore::open(paths).unwrap();
    let key = integrity.provision_initial_key("cli-dpapi-key").unwrap();
    let command = AppendReceiptCommand {
        receipt_id: "cli-receipt".into(),
        transaction_id: "cli-receipt-transaction".into(),
        state_root_generation: 1,
        delegation_id: "cli-delegation".into(),
        expected_state_revision: 1,
        expected_global_count: 0,
        expected_global_head_digest: None,
        expected_delegation_count: 0,
        expected_delegation_head_digest: None,
        receipt_kind: ReceiptKind::Intake,
        subject: ReceiptSubject {
            kind: ReceiptSubjectKind::OutboxEvent,
            subject_id: "cli-event".into(),
            revision: 1,
        },
        causation_id: "cli-cause".into(),
        causation_event_id: "cli-event".into(),
        actor: ReceiptActorBinding {
            actor_type: ActorType::HumanLocal,
            actor_identity: SafeText::new("local-operator", &[]).unwrap(),
            authority_provenance_id: "cli-authority".into(),
        },
        runtime: ReceiptRuntimeBinding {
            host_generation: 1,
            host_instance_id: "cli-host".into(),
            fencing_token: 1,
        },
        key,
        rotation: None,
        evidence: vec![ReceiptEvidenceInput {
            authority_link_id: "cli-link".into(),
            kind: AuthorityLinkKind::Session,
            source_id: "cli-session".into(),
            authoritative_revision: 0,
        }],
        redacted_summary: SafeText::summary("typed verifier proof", &[]).unwrap(),
        occurred_at: 1_800_000_000_000,
        committed_at: 1_800_000_000_010,
    };
    ExecAssStore::open(paths)
        .unwrap()
        .append_receipt(
            &integrity,
            &ReceiptRedactor::new(&["cli-fixture-secret"]).unwrap(),
            &command,
        )
        .unwrap();
}

#[test]
fn inspect_db_accepts_only_the_exact_execass_root() {
    let temp = tempfile::tempdir_in("Z:\\carsinos").expect("project-drive tempdir");
    let paths = AppPaths::from_root(temp.path().join("state"));
    init_execass_fresh_root(&paths).expect("initialize ExecAss root");
    let accepted = run("inspect-db", &paths);
    assert!(
        accepted.status.success(),
        "{}",
        String::from_utf8_lossy(&accepted.stderr)
    );
    assert!(String::from_utf8_lossy(&accepted.stdout).contains("\"ok\":true"));

    let legacy = AppPaths::from_root(temp.path().join("legacy"));
    carsinos_storage::init(&legacy).expect("initialize legacy root");
    let rejected = run("inspect-db", &legacy);
    assert_eq!(rejected.status.code(), Some(2));
}

#[cfg(windows)]
#[test]
fn windows_verifier_accepts_typed_dpapi_history_and_rejects_cross_root_without_path_echo() {
    let temp = tempfile::tempdir_in("Z:\\carsinos").expect("project-drive tempdir");
    let paths = AppPaths::from_root(temp.path().join("state"));
    init_execass_fresh_root(&paths).expect("initialize ExecAss root");
    seed_typed_receipt(&paths);
    let accepted = run("verify-active", &paths);
    assert!(
        accepted.status.success(),
        "{}",
        String::from_utf8_lossy(&accepted.stderr)
    );
    let accepted_json: serde_json::Value = serde_json::from_slice(&accepted.stdout).unwrap();
    assert_eq!(accepted_json["status"], "trusted");
    assert!(accepted_json["root_identity"]
        .as_str()
        .unwrap()
        .starts_with("sha256:"));
    assert!(accepted_json.get("state_root").is_none());
    let accepted_output = [accepted.stdout, accepted.stderr].concat();
    assert!(!accepted_output
        .windows(paths.root.to_string_lossy().len())
        .any(|window| window == paths.root.to_string_lossy().as_bytes()));

    let copied = AppPaths::from_root(temp.path().join("copied-state"));
    std::fs::create_dir_all(&copied.root).expect("create copied root");
    std::fs::copy(&paths.db_path, &copied.db_path).expect("copy receipt database");
    let rejected = run("verify-active", &copied);
    assert_eq!(rejected.status.code(), Some(2));
    let rejected_json: serde_json::Value = serde_json::from_slice(&rejected.stdout).unwrap();
    assert_eq!(rejected_json["ok"], false);
    assert_eq!(rejected_json["action"], "verify-active");
    assert_eq!(rejected_json["reason"], "receipt_integrity_rejected");
    assert!(rejected_json["root_locator"]
        .as_str()
        .unwrap()
        .starts_with("sha256:"));
    assert!(rejected_json.get("state_root").is_none());
    let rejected_output = [rejected.stdout, rejected.stderr].concat();
    assert!(!rejected_output
        .windows(copied.root.to_string_lossy().len())
        .any(|window| window == copied.root.to_string_lossy().as_bytes()));
}
