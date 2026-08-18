#![cfg(feature = "test-support")]

#[path = "real_process_execute_once.rs"]
mod base;

use base::{
    create_authoritative_fixture, execute_request, fake_provider_artifact_digest, now_ms,
    provider_count, spawn_recorder, test_root,
};
use carsinos_effect_recorder::{current_peer_identity_digest, Journal, TestRecorderFixture};
use carsinos_protocol::execass_recorder::{
    stable_text_digest, ExecuteOnceV1, QueryOnlyV1, ReconcileV1, RecorderObservationKindV1,
    RecorderObservationSourceV1, RecorderReplyV1, RecorderRequestV1,
};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

struct ChildGuard(Child);

impl ChildGuard {
    fn child_mut(&mut self) -> &mut Child {
        &mut self.0
    }
}

impl Drop for ChildGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

struct CoordinationRelease(PathBuf);

impl Drop for CoordinationRelease {
    fn drop(&mut self) {
        let _ = fs::create_dir_all(self.0.parent().unwrap_or(Path::new(".")));
        let _ = fs::write(&self.0, b"continue");
    }
}

#[test]
fn three_executable_kill_and_restart_matrix() {
    for (failpoint, expected_count, expected_reconcile) in [
        ("before_accepted_fsync", 0, None),
        (
            "after_accepted_fsync",
            0,
            Some(RecorderObservationKindV1::Absent),
        ),
        (
            "after_invocation_started_fsync",
            0,
            Some(RecorderObservationKindV1::Absent),
        ),
        (
            "after_provider_ledger_fsync",
            1,
            Some(RecorderObservationKindV1::Present),
        ),
        (
            "after_terminal_fsync",
            1,
            Some(RecorderObservationKindV1::Present),
        ),
    ] {
        let root = matrix_fixture(failpoint);
        let database = root.join("carsinos.db");
        let request = matrix_execute(&root, &database, failpoint);
        let mut recorder = spawn_recorder_failpoint(&root, &database, failpoint);
        let mut runtime = spawn_runtime(&root, &request, false, false, failpoint);
        let _release = coordination_release(&root, failpoint);
        wait_for_marker(&root, failpoint);
        recorder.child_mut().kill().unwrap();
        recorder.child_mut().wait().unwrap();
        if failpoint == "after_provider_ledger_fsync" {
            continue_from_marker(&root, failpoint);
            wait_for_provider_exit(&root);
        }
        let _ = runtime.child_mut().wait();
        assert_eq!(provider_count(&root), expected_count, "{failpoint}");
        if failpoint != "before_accepted_fsync" {
            let fixture = TestRecorderFixture::for_root(&request.binding.canonical_root_identity);
            let journal = fixture.open_journal(&root).unwrap();
            let latest = journal
                .query(
                    &request.binding.installation_id,
                    request.binding.state_root_generation,
                    &request.attempt_id,
                    Some(&Journal::command_digest(&request).unwrap()),
                )
                .unwrap()
                .unwrap();
            let expected_boundary = if failpoint == "after_accepted_fsync" {
                RecorderObservationKindV1::Accepted
            } else if failpoint == "after_terminal_fsync" {
                RecorderObservationKindV1::Present
            } else {
                RecorderObservationKindV1::InvocationStarted
            };
            assert_eq!(
                latest.kind, expected_boundary,
                "journal boundary {failpoint}"
            );
            drop(journal);
        }

        let restarted = ChildGuard(spawn_recorder(&root, &database));
        if failpoint == "after_terminal_fsync" {
            let reply = run_runtime(
                &root,
                &RecorderRequestV1::QueryOnly(Box::new(matrix_query(
                    &request,
                    "terminal-restart-query".into(),
                ))),
                false,
                false,
                "terminal-restart-query",
            );
            assert!(matches!(
                reply,
                RecorderReplyV1::Observation { replayed: true, observation, .. }
                    if observation.kind == RecorderObservationKindV1::Present
                        && observation.source == RecorderObservationSourceV1::Execution
            ));
        } else if let Some(expected_kind) = expected_reconcile {
            let reply = run_runtime(
                &root,
                &RecorderRequestV1::Reconcile(Box::new(matrix_reconcile(
                    &request,
                    format!("reconcile-{failpoint}"),
                ))),
                false,
                false,
                &format!("restart-reconcile-{failpoint}"),
            );
            assert!(
                matches!(
                    &reply,
                    RecorderReplyV1::Observation { replayed: false, observation, .. }
                        if observation.kind == expected_kind
                            && observation.source == RecorderObservationSourceV1::Reconciliation
                            && observation.technical_resource_actuals.is_empty()
                ),
                "{failpoint}: {reply:?}"
            );
        } else {
            let reply = run_runtime(
                &root,
                &RecorderRequestV1::ExecuteOnce(Box::new(request.clone())),
                false,
                false,
                "restart-execute-before-accepted",
            );
            assert!(matches!(
                reply,
                RecorderReplyV1::Observation { observation, .. }
                    if observation.kind == RecorderObservationKindV1::Present
            ));
        }
        drop(restarted);
        let final_count = if failpoint == "before_accepted_fsync" {
            1
        } else {
            expected_count
        };
        assert_eq!(provider_count(&root), final_count, "restart {failpoint}");

        let second_restart = ChildGuard(spawn_recorder(&root, &database));
        let reply = run_runtime(
            &root,
            &RecorderRequestV1::QueryOnly(Box::new(matrix_query(
                &request,
                format!("second-restart-{failpoint}"),
            ))),
            false,
            false,
            &format!("second-restart-query-{failpoint}"),
        );
        assert!(matches!(
            reply,
            RecorderReplyV1::Observation { replayed: true, .. }
        ));
        drop(second_restart);
        assert!(provider_count(&root) <= 1);
    }
}

#[test]
fn runtime_death_reconnect_race_and_hostile_matrix() {
    let root = matrix_fixture("runtime-after-send");
    let database = root.join("carsinos.db");
    let request = matrix_execute(&root, &database, "runtime-after-send");
    let recorder = spawn_recorder_failpoint(&root, &database, "after_accepted_fsync");
    let mut first_runtime = spawn_runtime(&root, &request, false, false, "runtime-to-kill");
    let _release = coordination_release(&root, "after_accepted_fsync");
    wait_for_marker(&root, "after_accepted_fsync");
    first_runtime.child_mut().kill().unwrap();
    first_runtime.child_mut().wait().unwrap();
    continue_from_marker(&root, "after_accepted_fsync");
    wait_for_provider_count(&root, 1);

    let query_a = RecorderRequestV1::QueryOnly(Box::new(matrix_query(&request, "query-a".into())));
    let query_b = RecorderRequestV1::QueryOnly(Box::new(matrix_query(&request, "query-b".into())));
    let mut runtime_a = spawn_runtime_request(&root, &query_a, false, false, "query-a-run");
    let mut runtime_b = spawn_runtime_request(&root, &query_b, false, false, "query-b-run");
    assert!(runtime_a.child_mut().wait().unwrap().success());
    assert!(runtime_b.child_mut().wait().unwrap().success());
    assert_eq!(provider_count(&root), 1);
    drop(recorder);

    let hostile_root = matrix_fixture("hostile-runtime");
    let hostile_db = hostile_root.join("carsinos.db");
    let base = matrix_execute(&hostile_root, &hostile_db, "hostile-runtime");
    let hostile_recorder = ChildGuard(spawn_recorder(&hostile_root, &hostile_db));
    let mut cases = Vec::new();
    let mut payload = base.clone();
    payload.payload_digest.push_str("-wrong");
    cases.push(("payload", payload));
    let mut provider = base.clone();
    provider.provider_identity = "wrong-provider".into();
    provider.provider_request_digest = provider.derived_provider_request_digest().unwrap();
    cases.push(("provider", provider));
    let mut adapter = base.clone();
    adapter.adapter_identity = "wrong-adapter".into();
    cases.push(("adapter", adapter));
    let mut artifact = base.clone();
    artifact.adapter_artifact_digest = format!("sha256:{}", "0".repeat(64));
    cases.push(("artifact", artifact));
    let mut fence = base.clone();
    fence.binding.runtime_fencing_token += 1;
    cases.push(("fence", fence));
    let mut attempt = base.clone();
    attempt.attempt_id = "wrong-attempt".into();
    cases.push(("attempt", attempt));
    for (name, command) in cases {
        let _ = run_runtime_result(
            &hostile_root,
            &RecorderRequestV1::ExecuteOnce(Box::new(command)),
            false,
            false,
            name,
        );
        assert_eq!(provider_count(&hostile_root), 0, "hostile {name}");
    }
    let mac_reply = run_runtime(
        &hostile_root,
        &RecorderRequestV1::ExecuteOnce(Box::new(base)),
        false,
        true,
        "wrong-mac",
    );
    assert!(matches!(
        mac_reply,
        RecorderReplyV1::Rejected { code, .. } if code == "authentication_failed"
    ));
    drop(hostile_recorder);
    assert_eq!(provider_count(&hostile_root), 0);
}

#[test]
fn runtime_death_before_connect_has_no_recorder_or_provider_record() {
    let root = matrix_fixture("runtime-before-connect");
    let database = root.join("carsinos.db");
    let request = matrix_execute(&root, &database, "runtime-before-connect");
    let recorder = ChildGuard(spawn_recorder(&root, &database));
    let mut runtime = spawn_runtime(&root, &request, true, false, "before-connect");
    assert!(runtime.child_mut().wait().unwrap().success());
    assert_eq!(provider_count(&root), 0);
    assert!(
        !root.join("runtime/effect-recorder/v1/journal.v1").exists()
            || fs::metadata(root.join("runtime/effect-recorder/v1/journal.v1"))
                .unwrap()
                .len()
                == 0
    );
    drop(recorder);
}

#[test]
fn reconcile_validation_unknown_and_exact_replay_are_signed() {
    let root = matrix_fixture("reconcile-error");
    let database = root.join("carsinos.db");
    let request = matrix_execute(&root, &database, "reconcile-error");
    let fixture = TestRecorderFixture::for_root(&request.binding.canonical_root_identity);
    {
        let mut journal = fixture.open_journal(&root).unwrap();
        journal.accept(&request, now_ms()).unwrap();
        journal.mark_invocation_started(&request, now_ms()).unwrap();
    }
    fs::create_dir_all(root.join("ea213-fake-provider/invocations.jsonl")).unwrap();
    let recorder = ChildGuard(spawn_recorder(&root, &database));
    let reconcile = matrix_reconcile(&request, "reconcile-error-first".into());
    let first = run_runtime(
        &root,
        &RecorderRequestV1::Reconcile(Box::new(reconcile.clone())),
        false,
        false,
        "reconcile-error-first",
    );
    let first_observation = match first {
        RecorderReplyV1::Observation {
            replayed: false,
            observation,
            ..
        } => observation,
        other => panic!("expected fresh reconciliation observation: {other:?}"),
    };
    assert_eq!(first_observation.kind, RecorderObservationKindV1::Unknown);
    assert_eq!(
        first_observation.source,
        RecorderObservationSourceV1::Reconciliation
    );
    assert_eq!(
        first_observation.canonical_root_identity,
        request.binding.canonical_root_identity
    );
    assert_eq!(
        first_observation.os_user_identity_digest,
        request.binding.os_user_identity_digest
    );
    assert_eq!(
        first_observation.reconciliation_window_start_ms,
        Some(reconcile.consistency_window_start_ms)
    );
    assert_eq!(
        first_observation.reconciliation_window_end_ms,
        Some(reconcile.consistency_window_end_ms)
    );
    assert!(first_observation.technical_resource_actuals.is_empty());

    let replay = run_runtime(
        &root,
        &RecorderRequestV1::Reconcile(Box::new(reconcile.clone())),
        false,
        false,
        "reconcile-error-replay",
    );
    assert!(matches!(
        replay,
        RecorderReplyV1::Observation { replayed: true, observation, .. }
            if observation.record_digest == first_observation.record_digest
                && observation.sequence == first_observation.sequence
    ));

    let mut future = reconcile.clone();
    future.request_id = "future-window".into();
    future.client_nonce = "future-window-nonce".into();
    future.consistency_window_end_ms = now_ms() + 60_000;
    let rejected = run_runtime(
        &root,
        &RecorderRequestV1::Reconcile(Box::new(future)),
        false,
        false,
        "future-window",
    );
    assert!(matches!(
        rejected,
        RecorderReplyV1::Rejected { code, .. }
            if code == "reconciliation_identity_mismatch"
    ));

    fs::remove_dir(root.join("ea213-fake-provider/invocations.jsonl")).unwrap();
    let status = Command::new(env!("CARGO_BIN_EXE_ea213-fake-provider"))
        .arg("invoke")
        .arg("--fixture-root")
        .arg(root.join("ea213-fake-provider"))
        .arg("--attempt-id")
        .arg(&request.attempt_id)
        .arg("--idempotency-key")
        .arg(request.provider_idempotency_key.as_deref().unwrap())
        .arg("--reconciliation-key")
        .arg(request.reconciliation_key.as_deref().unwrap())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .unwrap();
    assert!(status.success());
    let mut later = reconcile.clone();
    later.request_id = "later-present".into();
    later.client_nonce = "later-present-nonce".into();
    later.consistency_window_start_ms = 1;
    later.consistency_window_end_ms = now_ms();
    let present = run_runtime(
        &root,
        &RecorderRequestV1::Reconcile(Box::new(later.clone())),
        false,
        false,
        "later-present",
    );
    let present_observation = match present {
        RecorderReplyV1::Observation {
            replayed: false,
            observation,
            ..
        } if observation.kind == RecorderObservationKindV1::Present => observation,
        other => panic!("expected Unknown to advance to Present: {other:?}"),
    };
    fs::write(
        root.join("ea213-fake-provider/invocations.jsonl"),
        b"not-json\n",
    )
    .unwrap();
    later.request_id = "terminal-present-replay".into();
    later.client_nonce = "terminal-present-replay-nonce".into();
    later.consistency_window_start_ms = 2;
    later.consistency_window_end_ms = now_ms();
    let terminal_replay = run_runtime(
        &root,
        &RecorderRequestV1::Reconcile(Box::new(later)),
        false,
        false,
        "terminal-present-replay",
    );
    assert!(matches!(
        terminal_replay,
        RecorderReplyV1::Observation { replayed: true, observation, .. }
            if observation.kind == RecorderObservationKindV1::Present
                && observation.record_digest == present_observation.record_digest
                && observation.sequence == present_observation.sequence
    ));
    drop(recorder);
    assert_eq!(provider_count(&root), 1);
}

fn matrix_fixture(name: &str) -> PathBuf {
    let root = test_root(&format!("matrix-{name}"));
    let database = root.join("carsinos.db");
    let now = now_ms();
    create_authoritative_fixture(
        &database,
        now,
        &carsinos_effect_recorder::canonical_state_root(&root)
            .unwrap()
            .identity,
        &format!("installation-matrix-{name}"),
        &current_peer_identity_digest().unwrap(),
    );
    root
}

fn matrix_execute(root: &Path, _database: &Path, name: &str) -> ExecuteOnceV1 {
    let now = now_ms();
    let mut command = execute_request(
        now,
        carsinos_effect_recorder::canonical_state_root(root)
            .unwrap()
            .identity,
        format!("installation-matrix-{name}"),
        current_peer_identity_digest().unwrap(),
        fake_provider_artifact_digest(),
    );
    command.request_id = format!("execute-{name}");
    command.client_nonce = format!("nonce-{name}");
    command
}

fn matrix_query(command: &ExecuteOnceV1, request_id: String) -> QueryOnlyV1 {
    QueryOnlyV1 {
        binding: command.binding.clone(),
        request_id,
        attempt_id: command.attempt_id.clone(),
        expected_command_digest: Some(Journal::command_digest(command).unwrap()),
        known_journal_head: None,
        client_nonce: format!("query-nonce-{}", command.request_id),
        command_mac: String::new(),
    }
}

fn matrix_reconcile(command: &ExecuteOnceV1, request_id: String) -> ReconcileV1 {
    let key = command.reconciliation_key.clone().unwrap();
    let now = now_ms();
    ReconcileV1 {
        binding: command.binding.clone(),
        request_id,
        attempt_id: command.attempt_id.clone(),
        expected_command_digest: Journal::command_digest(command).unwrap(),
        reconciliation_key_digest: stable_text_digest(&key),
        reconciliation_key: key,
        consistency_window_start_ms: 0,
        consistency_window_end_ms: now,
        client_nonce: format!("reconcile-nonce-{}", command.request_id),
        command_mac: String::new(),
    }
}

fn spawn_recorder_failpoint(root: &Path, database: &Path, failpoint: &str) -> ChildGuard {
    ChildGuard(
        Command::new(env!("CARGO_BIN_EXE_carsinos-effect-recorder"))
            .arg("--state-root")
            .arg(root)
            .arg("--database")
            .arg(database)
            .arg("--test-fake-provider")
            .arg("--test-failpoint")
            .arg(failpoint)
            .arg("--test-coordination-root")
            .arg(root.join("coordination"))
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    )
}

fn spawn_runtime(
    root: &Path,
    command: &ExecuteOnceV1,
    die_before_connect: bool,
    wrong_channel_key: bool,
    name: &str,
) -> ChildGuard {
    spawn_runtime_request(
        root,
        &RecorderRequestV1::ExecuteOnce(Box::new(command.clone())),
        die_before_connect,
        wrong_channel_key,
        name,
    )
}

fn spawn_runtime_request(
    root: &Path,
    request: &RecorderRequestV1,
    die_before_connect: bool,
    wrong_channel_key: bool,
    name: &str,
) -> ChildGuard {
    let request_path = root.join(format!("{name}.request.json"));
    fs::write(&request_path, serde_json::to_vec(request).unwrap()).unwrap();
    let reply_path = root.join(format!("{name}.reply.json"));
    let mut process = Command::new(env!("CARGO_BIN_EXE_ea213-runtime-harness"));
    process
        .arg("--state-root")
        .arg(root)
        .arg("--request")
        .arg(request_path)
        .arg("--reply")
        .arg(reply_path);
    if die_before_connect {
        process.arg("--die-before-connect");
    }
    if wrong_channel_key {
        process.arg("--wrong-channel-key");
    }
    ChildGuard(
        process
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    )
}

fn run_runtime(
    root: &Path,
    request: &RecorderRequestV1,
    die_before_connect: bool,
    wrong_channel_key: bool,
    name: &str,
) -> RecorderReplyV1 {
    run_runtime_result(root, request, die_before_connect, wrong_channel_key, name).unwrap()
}

fn run_runtime_result(
    root: &Path,
    request: &RecorderRequestV1,
    die_before_connect: bool,
    wrong_channel_key: bool,
    name: &str,
) -> anyhow::Result<RecorderReplyV1> {
    let request_path = root.join(format!("{name}.request.json"));
    let reply_path = root.join(format!("{name}.reply.json"));
    fs::write(&request_path, serde_json::to_vec(request)?)?;
    let mut child =
        spawn_runtime_request(root, request, die_before_connect, wrong_channel_key, name);
    let status = child.child_mut().wait()?;
    if !status.success() {
        anyhow::bail!("runtime harness {name} failed with {status}");
    }
    Ok(serde_json::from_slice(&fs::read(reply_path)?)?)
}

fn wait_for_marker(root: &Path, failpoint: &str) {
    let marker = root
        .join("coordination")
        .join(format!("{}.reached", failpoint.replace('_', "")));
    wait_until(|| marker.exists(), &format!("marker {marker:?}"));
}

fn continue_from_marker(root: &Path, failpoint: &str) {
    let marker = root
        .join("coordination")
        .join(format!("{}.continue", failpoint.replace('_', "")));
    fs::write(marker, b"continue").unwrap();
}

fn coordination_release(root: &Path, failpoint: &str) -> CoordinationRelease {
    CoordinationRelease(
        root.join("coordination")
            .join(format!("{}.continue", failpoint.replace('_', ""))),
    )
}

fn wait_for_provider_count(root: &Path, count: usize) {
    wait_until(|| provider_count(root) == count, "provider count");
}

fn wait_for_provider_exit(root: &Path) {
    let marker = root
        .join("coordination")
        .join("afterproviderledgerfsync.exited");
    wait_until(|| marker.exists(), "fake-provider exit");
}

fn wait_until(mut predicate: impl FnMut() -> bool, label: &str) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        if predicate() {
            return;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("timed out waiting for {label}");
}
