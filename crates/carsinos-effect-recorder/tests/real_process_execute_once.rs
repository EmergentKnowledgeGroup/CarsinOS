#![cfg(feature = "test-support")]

use carsinos_effect_recorder::{
    canonical_state_root, current_peer_identity_digest, hex_for_binary, Journal,
    ReadOnlyBeganVerifier, RecorderEndpoint, RecorderService, TestRecorderFixture,
};
use carsinos_protocol::execass_recorder::*;
use rusqlite::{params, Connection};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

static NEXT: AtomicU64 = AtomicU64::new(1);

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn fresh_execute_once_and_restart_query_keep_provider_count_one() {
    let root = test_root("real-process");
    let database = root.join("carsinos.db");
    let now = now_ms();
    let canonical_root_identity = canonical_state_root(&root).unwrap().identity;
    let installation_id = "installation-ea213";
    let os_user_identity_digest = current_peer_identity_digest().unwrap();
    create_authoritative_fixture(
        &database,
        now,
        &canonical_root_identity,
        installation_id,
        &os_user_identity_digest,
    );

    let fake_provider = PathBuf::from(env!("CARGO_BIN_EXE_ea213-fake-provider"));
    let artifact_digest = format!(
        "sha256:{}",
        hex_for_binary(&Sha256::digest(fs::read(&fake_provider).unwrap()))
    );
    let endpoint = RecorderEndpoint::for_binding(&root, installation_id, 1);
    let fixture = TestRecorderFixture::for_root(&canonical_root_identity);
    let client = fixture.client(endpoint);

    let mut recorder = spawn_recorder(&root, &database);
    let execute = execute_request(
        now,
        canonical_root_identity.clone(),
        installation_id.into(),
        os_user_identity_digest.clone(),
        artifact_digest,
    );
    let reply = client
        .send(RecorderRequestV1::ExecuteOnce(Box::new(execute.clone())))
        .await
        .unwrap();
    let command_digest = match reply {
        RecorderReplyV1::Observation {
            replayed: false,
            observation,
            ..
        } => {
            assert_eq!(observation.kind, RecorderObservationKindV1::Present);
            assert!(observation.evidence_payload_digest.is_some());
            assert!(observation.technical_resource_actuals.is_empty());
            observation.command_digest
        }
        other => panic!("fresh ExecuteOnce did not produce Present: {other:?}"),
    };

    // Exact replay is journal truth and must not depend on the authoritative
    // attempt still being in its former live Began state.
    let connection = Connection::open(&database).unwrap();
    connection
        .execute(
            "UPDATE execass_provider_attempts SET status='succeeded'",
            [],
        )
        .unwrap();
    connection
        .execute("UPDATE execass_logical_effects SET state='succeeded'", [])
        .unwrap();
    connection
        .execute(
            "UPDATE execass_global_runtime_control SET engaged=1,global_stop_epoch=1",
            [],
        )
        .unwrap();
    drop(connection);
    let replay = client
        .send(RecorderRequestV1::ExecuteOnce(Box::new(execute.clone())))
        .await
        .unwrap();
    assert!(matches!(
        replay,
        RecorderReplyV1::Observation {
            replayed: true,
            observation,
            ..
        } if observation.kind == RecorderObservationKindV1::Present
    ));
    terminate(&mut recorder);
    assert_eq!(provider_count(&root), 1);

    let mut restarted = spawn_recorder(&root, &database);
    let query = QueryOnlyV1 {
        binding: execute.binding,
        request_id: "query-after-restart".into(),
        attempt_id: execute.attempt_id,
        expected_command_digest: Some(command_digest),
        known_journal_head: None,
        client_nonce: "query-nonce".into(),
        command_mac: String::new(),
    };
    let reply = client
        .send(RecorderRequestV1::QueryOnly(Box::new(query)))
        .await
        .unwrap();
    match reply {
        RecorderReplyV1::Observation {
            replayed: true,
            observation,
            ..
        } => assert_eq!(observation.kind, RecorderObservationKindV1::Present),
        other => panic!("restart QueryOnly did not replay Present: {other:?}"),
    }
    terminate(&mut restarted);
    assert_eq!(provider_count(&root), 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn eight_concurrent_clients_invoke_provider_exactly_once() {
    let root = test_root("concurrent-eight");
    let database = root.join("carsinos.db");
    let now = now_ms();
    let canonical_root_identity = canonical_state_root(&root).unwrap().identity;
    let installation_id = "installation-concurrent";
    let os_user_identity_digest = current_peer_identity_digest().unwrap();
    create_authoritative_fixture(
        &database,
        now,
        &canonical_root_identity,
        installation_id,
        &os_user_identity_digest,
    );
    let artifact_digest = fake_provider_artifact_digest();
    let endpoint = RecorderEndpoint::for_binding(&root, installation_id, 1);
    let fixture = TestRecorderFixture::for_root(&canonical_root_identity);
    let client = fixture.client(endpoint);
    let request = execute_request(
        now,
        canonical_root_identity,
        installation_id.into(),
        os_user_identity_digest,
        artifact_digest,
    );
    let mut recorder = spawn_recorder(&root, &database);
    let barrier = Arc::new(tokio::sync::Barrier::new(9));
    let mut tasks = Vec::new();
    for index in 0..8 {
        let barrier = barrier.clone();
        let client = client.clone();
        let mut request = request.clone();
        request.request_id = format!("concurrent-request-{index}");
        request.client_nonce = format!("concurrent-nonce-{index}");
        tasks.push(tokio::spawn(async move {
            barrier.wait().await;
            client
                .send(RecorderRequestV1::ExecuteOnce(Box::new(request)))
                .await
        }));
    }
    barrier.wait().await;
    for task in tasks {
        assert!(matches!(
            task.await.unwrap().unwrap(),
            RecorderReplyV1::Observation { .. }
        ));
    }
    terminate(&mut recorder);
    assert_eq!(provider_count(&root), 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn crash_boundaries_do_not_duplicate_or_strand() {
    for (name, prestarted, expected_kind) in [
        ("accepted-only", false, RecorderObservationKindV1::Accepted),
        (
            "started-only",
            true,
            RecorderObservationKindV1::InvocationStarted,
        ),
    ] {
        let root = test_root(name);
        let database = root.join("carsinos.db");
        let now = now_ms();
        let canonical_root_identity = canonical_state_root(&root).unwrap().identity;
        let installation_id = format!("installation-{name}");
        let os_user_identity_digest = current_peer_identity_digest().unwrap();
        create_authoritative_fixture(
            &database,
            now,
            &canonical_root_identity,
            &installation_id,
            &os_user_identity_digest,
        );
        let request = execute_request(
            now,
            canonical_root_identity.clone(),
            installation_id.clone(),
            os_user_identity_digest,
            fake_provider_artifact_digest(),
        );
        let fixture = TestRecorderFixture::for_root(&canonical_root_identity);
        {
            let mut journal = fixture.open_journal(&root).unwrap();
            journal.accept(&request, now).unwrap();
            if prestarted {
                journal.mark_invocation_started(&request, now + 1).unwrap();
            }
        }
        let endpoint = RecorderEndpoint::for_binding(&root, &installation_id, 1);
        let client = fixture.client(endpoint);
        let mut recorder = spawn_recorder(&root, &database);
        let reply = client
            .send(RecorderRequestV1::ExecuteOnce(Box::new(request)))
            .await
            .unwrap();
        match reply {
            RecorderReplyV1::Observation {
                replayed: true,
                observation,
                ..
            } => assert_eq!(observation.kind, expected_kind),
            other => panic!("unexpected crash-boundary reply: {other:?}"),
        }
        terminate(&mut recorder);
        assert_eq!(provider_count(&root), 0);
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn queued_fresh_request_rechecks_live_fence_before_admission() {
    let root = test_root("queued-fence-loss");
    let database = root.join("carsinos.db");
    let now = now_ms();
    let canonical_root_identity = canonical_state_root(&root).unwrap().identity;
    let installation_id = "installation-queued";
    let os_user_identity_digest = current_peer_identity_digest().unwrap();
    create_authoritative_fixture(
        &database,
        now,
        &canonical_root_identity,
        installation_id,
        &os_user_identity_digest,
    );
    let fixture = TestRecorderFixture::for_root(&canonical_root_identity);
    let journal = fixture.open_journal(&root).unwrap();
    let service = RecorderService::with_fake_provider(
        [0x72; 32],
        ReadOnlyBeganVerifier::new(&database),
        journal,
        root.join("ea213-fake-provider"),
    );
    let (entered_tx, entered_rx) = tokio::sync::oneshot::channel();
    let (release_tx, release_rx) = tokio::sync::oneshot::channel();
    let holder = {
        let service = service.clone();
        tokio::spawn(async move { service.test_hold_journal(entered_tx, release_rx).await })
    };
    entered_rx.await.unwrap();
    let mut request = RecorderRequestV1::ExecuteOnce(Box::new(execute_request(
        now,
        canonical_root_identity,
        installation_id.into(),
        os_user_identity_digest,
        fake_provider_artifact_digest(),
    )));
    fixture.sign_request(&mut request).unwrap();
    let queued = {
        let service = service.clone();
        tokio::spawn(async move { service.handle(request).await })
    };
    for _ in 0..8 {
        tokio::task::yield_now().await;
    }
    Connection::open(&database)
        .unwrap()
        .execute("UPDATE execass_runtime_host_leases SET fencing_token=2", [])
        .unwrap();
    release_tx.send(()).unwrap();
    holder.await.unwrap();
    assert!(matches!(
        queued.await.unwrap(),
        RecorderReplyV1::Rejected { code, .. } if code == "began_not_proven"
    ));
    assert_eq!(provider_count(&root), 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn wrong_mac_fence_and_attempt_never_reach_provider() {
    let root = test_root("hostile-process");
    let database = root.join("carsinos.db");
    let now = now_ms();
    let canonical_root_identity = canonical_state_root(&root).unwrap().identity;
    let installation_id = "installation-hostile";
    let os_user_identity_digest = current_peer_identity_digest().unwrap();
    create_authoritative_fixture(
        &database,
        now,
        &canonical_root_identity,
        installation_id,
        &os_user_identity_digest,
    );
    let fake_provider = PathBuf::from(env!("CARGO_BIN_EXE_ea213-fake-provider"));
    let artifact_digest = format!(
        "sha256:{}",
        hex_for_binary(&Sha256::digest(fs::read(fake_provider).unwrap()))
    );
    let endpoint = RecorderEndpoint::for_binding(&root, installation_id, 1);
    let fixture = TestRecorderFixture::for_root(&canonical_root_identity);
    let client = fixture.client(endpoint.clone());
    let wrong_client = fixture.client_with_channel_key(endpoint, [9u8; 32]);
    let mut recorder = spawn_recorder(&root, &database);

    let request = execute_request(
        now,
        canonical_root_identity,
        installation_id.into(),
        os_user_identity_digest,
        artifact_digest,
    );
    assert!(matches!(
        wrong_client
            .send(RecorderRequestV1::ExecuteOnce(Box::new(request.clone())))
            .await
            .unwrap(),
        RecorderReplyV1::Rejected { code, .. } if code == "authentication_failed"
    ));
    let mut wrong_fence = request.clone();
    wrong_fence.binding.runtime_fencing_token += 1;
    assert!(client
        .send(RecorderRequestV1::ExecuteOnce(Box::new(wrong_fence)))
        .await
        .is_err());
    let mut wrong_attempt = request;
    wrong_attempt.attempt_id = "caller-invented-attempt".into();
    assert!(matches!(
        client
            .send(RecorderRequestV1::ExecuteOnce(Box::new(wrong_attempt)))
            .await
            .unwrap(),
        RecorderReplyV1::Rejected { code, .. } if code == "began_not_proven"
    ));
    terminate(&mut recorder);
    assert_eq!(provider_count(&root), 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn alternate_root_fails_closed_and_lexical_alias_reuses_one_journal() {
    let root = test_root("root-binding");
    let database = root.join("carsinos.db");
    let now = now_ms();
    let canonical_root_identity = canonical_state_root(&root).unwrap().identity;
    let installation_id = "installation-root-binding";
    let os_user_identity_digest = current_peer_identity_digest().unwrap();
    create_authoritative_fixture(
        &database,
        now,
        &canonical_root_identity,
        installation_id,
        &os_user_identity_digest,
    );
    let fixture = TestRecorderFixture::for_root(&canonical_root_identity);
    let client = fixture.client(RecorderEndpoint::for_binding(&root, installation_id, 1));
    let request = execute_request(
        now,
        canonical_root_identity,
        installation_id.into(),
        os_user_identity_digest,
        fake_provider_artifact_digest(),
    );
    let mut first = spawn_recorder(&root, &database);
    assert!(matches!(
        client
            .send(RecorderRequestV1::ExecuteOnce(Box::new(request.clone())))
            .await
            .unwrap(),
        RecorderReplyV1::Observation { observation, .. }
            if observation.kind == RecorderObservationKindV1::Present
    ));
    terminate(&mut first);
    assert_eq!(provider_count(&root), 1);

    let alternate_root = test_root("root-binding-alternate");
    let alternate_database = alternate_root.join("carsinos.db");
    fs::hard_link(&database, &alternate_database).unwrap();
    let mut rejected = spawn_recorder_with_state_root(&alternate_root, &alternate_database);
    assert!(!rejected.wait().unwrap().success());
    assert_eq!(provider_count(&root), 1);
    assert!(!alternate_root.join("runtime/effect-recorder/v1").exists());

    let alias_child = root.join("lexical-alias-child");
    fs::create_dir_all(&alias_child).unwrap();
    let alias = alias_child.join("..");
    let mut restarted = spawn_recorder_with_state_root(&alias, &database);
    let command_digest = Journal::command_digest(&request).unwrap();
    let query = QueryOnlyV1 {
        binding: request.binding,
        request_id: "alias-restart-query".into(),
        attempt_id: request.attempt_id,
        expected_command_digest: Some(command_digest),
        known_journal_head: None,
        client_nonce: "alias-query-nonce".into(),
        command_mac: String::new(),
    };
    assert!(matches!(
        client.send(RecorderRequestV1::QueryOnly(Box::new(query))).await.unwrap(),
        RecorderReplyV1::Observation { replayed: true, observation, .. }
            if observation.kind == RecorderObservationKindV1::Present
    ));
    terminate(&mut restarted);
    assert_eq!(provider_count(&root), 1);
}

pub(crate) fn execute_request(
    now: i64,
    canonical_root_identity: String,
    installation_id: String,
    os_user_identity_digest: String,
    adapter_artifact_digest: String,
) -> ExecuteOnceV1 {
    let mut command = ExecuteOnceV1 {
        binding: RecorderBindingV1 {
            protocol_version: RECORDER_PROTOCOL_VERSION.into(),
            canonical_root_identity,
            installation_id,
            state_root_generation: 1,
            os_user_identity_digest,
            runtime_host_generation: 1,
            runtime_host_instance_id: "host-ea213".into(),
            runtime_fencing_token: 1,
        },
        request_id: "execute-ea213".into(),
        claim_event_id: "claim-event-ea213".into(),
        claim_receipt_id: "claim-receipt-ea213".into(),
        continuation_fencing_token: 1,
        delegation_id: "delegation-ea213".into(),
        continuation_id: "continuation-ea213".into(),
        action_id: "action-ea213".into(),
        logical_effect_id: "effect-ea213".into(),
        internal_idempotency_key: "internal-key-ea213".into(),
        attempt_id: "attempt-ea213".into(),
        attempt_number: 1,
        provider_identity: "fake-provider".into(),
        provider_version: "v1".into(),
        adapter_identity: "ea213.fake-provider.v1".into(),
        adapter_artifact_digest,
        provider_request_digest: String::new(),
        provider_idempotency_key: Some("provider-key-ea213".into()),
        reconciliation_key: Some("reconcile-key-ea213".into()),
        manifest_digest: "manifest-ea213".into(),
        payload_digest: "payload-ea213".into(),
        operand_envelope: OpaqueOperandEnvelopeV1 {
            non_secret: serde_json::json!({"fixture": true}),
            secret_handles: vec![],
        },
        deadline_ms: now + 120_000,
        client_nonce: "execute-nonce".into(),
        command_mac: String::new(),
    };
    command.provider_request_digest = command.derived_provider_request_digest().unwrap();
    command
}

pub(crate) fn create_authoritative_fixture(
    database: &Path,
    now: i64,
    canonical_root_identity: &str,
    installation_id: &str,
    os_user_identity_digest: &str,
) {
    let connection = Connection::open(database).unwrap();
    connection
        .execute_batch(
            r#"
            CREATE TABLE execass_provider_attempts(attempt_id TEXT,attempt_number INTEGER,status TEXT,delegation_id TEXT,continuation_id TEXT,action_id TEXT,logical_effect_id TEXT,claim_event_id TEXT,claim_receipt_id TEXT,fencing_token INTEGER,host_generation INTEGER,host_instance_id TEXT,runtime_fencing_token INTEGER,provider_request_digest TEXT);
            CREATE TABLE execass_logical_effects(delegation_id TEXT,logical_effect_id TEXT,state TEXT,internal_idempotency_key TEXT,provider_identity TEXT,provider_idempotency_key TEXT,reconciliation_key TEXT,manifest_digest TEXT,payload_digest TEXT);
            CREATE TABLE execass_continuation_operation_history(event_id TEXT,operation TEXT,claim_event_id TEXT,claim_receipt_id TEXT,continuation_id TEXT,delegation_id TEXT,action_id TEXT,job_id TEXT,worker_id TEXT,job_lease_expires_at INTEGER,continuation_fencing_token INTEGER,runtime_host_generation INTEGER,runtime_host_instance_id TEXT,runtime_fencing_token INTEGER,state_root_generation INTEGER,global_stop_epoch INTEGER);
            CREATE TABLE execass_continuations(delegation_id TEXT,continuation_id TEXT,status TEXT,job_id TEXT,lease_owner TEXT,lease_expires_at INTEGER,fencing_token INTEGER,host_generation INTEGER);
            CREATE TABLE jobs(job_id TEXT,enabled INTEGER,deleted_at INTEGER,lease_owner TEXT,lease_expires_at INTEGER,payload_json TEXT);
            CREATE TABLE execass_runtime_host_generations(generation INTEGER,host_instance_id TEXT,state_root_generation INTEGER,installation_identity TEXT,os_user_identity_digest TEXT);
            CREATE TABLE execass_runtime_host_leases(ownership_scope TEXT,generation INTEGER,host_instance_id TEXT,fencing_token INTEGER,released_at INTEGER,expires_at INTEGER);
            CREATE TABLE execass_confirmation_authority_keys(status TEXT,canonical_root_identity TEXT,installation_identity TEXT,os_user_identity_digest TEXT,state_root_generation INTEGER);
            CREATE TABLE execass_global_runtime_control(singleton INTEGER,engaged INTEGER,global_stop_epoch INTEGER);
            "#,
        )
        .unwrap();
    let expiry = now + 300_000;
    connection.execute("INSERT INTO execass_logical_effects VALUES('delegation-ea213','effect-ea213','invoking','internal-key-ea213','fake-provider','provider-key-ea213','reconcile-key-ea213','manifest-ea213','payload-ea213')", []).unwrap();
    let artifact_digest = format!(
        "sha256:{}",
        hex_for_binary(&Sha256::digest(
            fs::read(env!("CARGO_BIN_EXE_ea213-fake-provider")).unwrap()
        ))
    );
    let request_digest = execute_request(
        now,
        canonical_root_identity.to_owned(),
        installation_id.to_owned(),
        os_user_identity_digest.to_owned(),
        artifact_digest,
    )
    .provider_request_digest;
    connection.execute("INSERT INTO execass_provider_attempts VALUES('attempt-ea213',1,'invoking','delegation-ea213','continuation-ea213','action-ea213','effect-ea213','claim-event-ea213','claim-receipt-ea213',1,1,'host-ea213',1,?1)", [&request_digest]).unwrap();
    connection.execute("INSERT INTO execass_continuation_operation_history VALUES('claim-event-ea213','claim','claim-event-ea213','claim-receipt-ea213','continuation-ea213','delegation-ea213','action-ea213','job-ea213','worker-ea213',?1,1,1,'host-ea213',1,1,0)", [expiry]).unwrap();
    connection.execute("INSERT INTO execass_continuations VALUES('delegation-ea213','continuation-ea213','executing','job-ea213','worker-ea213',?1,1,1)", [expiry]).unwrap();
    connection.execute("INSERT INTO jobs VALUES('job-ea213',1,NULL,'worker-ea213',?1,'{\"mode\":\"execass.continuation\"}')", [expiry]).unwrap();
    connection
        .execute(
            "INSERT INTO execass_runtime_host_generations VALUES(1,'host-ea213',1,?1,?2)",
            params![installation_id, os_user_identity_digest],
        )
        .unwrap();
    connection
        .execute(
            "INSERT INTO execass_runtime_host_leases VALUES('execass',1,'host-ea213',1,NULL,?1)",
            [expiry],
        )
        .unwrap();
    connection
        .execute(
            "INSERT INTO execass_confirmation_authority_keys VALUES('active',?1,?2,?3,1)",
            params![
                canonical_root_identity,
                installation_id,
                os_user_identity_digest
            ],
        )
        .unwrap();
    connection
        .execute(
            "INSERT INTO execass_global_runtime_control VALUES(1,0,0)",
            [],
        )
        .unwrap();
}

pub(crate) fn spawn_recorder(root: &Path, database: &Path) -> Child {
    spawn_recorder_with_state_root(root, database)
}

fn spawn_recorder_with_state_root(root: &Path, database: &Path) -> Child {
    Command::new(env!("CARGO_BIN_EXE_carsinos-effect-recorder"))
        .arg("--state-root")
        .arg(root)
        .arg("--database")
        .arg(database)
        .arg("--test-fake-provider")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap()
}

fn terminate(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

pub(crate) fn provider_count(root: &Path) -> usize {
    fs::read_to_string(root.join("ea213-fake-provider/invocations.jsonl"))
        .map(|value| value.lines().count())
        .unwrap_or(0)
}

pub(crate) fn fake_provider_artifact_digest() -> String {
    format!(
        "sha256:{}",
        hex_for_binary(&Sha256::digest(
            fs::read(env!("CARGO_BIN_EXE_ea213-fake-provider")).unwrap()
        ))
    )
}

pub(crate) fn test_root(name: &str) -> PathBuf {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join(".ea213-test-tmp")
        .join(format!(
            "{}-{}-{}",
            name,
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
    fs::create_dir_all(&root).unwrap();
    root
}

pub(crate) fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis()
        .try_into()
        .unwrap()
}
