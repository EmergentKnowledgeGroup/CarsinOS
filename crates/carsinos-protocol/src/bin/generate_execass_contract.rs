//! Deterministic generator for the checked-in ExecAss v1 schema and OpenAPI contract.
//!
//! Run with `cargo run -p carsinos-protocol --bin generate_execass_contract -- --write`
//! to update artifacts, or `--check` to verify that checked-in bytes are current.

use carsinos_protocol::execass::*;
use schemars::{schema_for, JsonSchema};
use serde_json::{json, Map, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

const OPENAPI_FILE: &str = "contracts/execass/v1/openapi.json";
const SCHEMA_DIR: &str = "contracts/execass/v1/schema";
const ERROR_SCHEMA: &str = "api-error";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    Check,
    Write,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CheckFailure {
    missing: Vec<PathBuf>,
    stale: Vec<PathBuf>,
    unexpected: Vec<PathBuf>,
}

impl CheckFailure {
    fn is_empty(&self) -> bool {
        self.missing.is_empty() && self.stale.is_empty() && self.unexpected.is_empty()
    }

    fn message(&self) -> String {
        let mut lines = vec!["ExecAss generated artifacts are not current:".to_string()];
        for (label, paths) in [
            ("missing", &self.missing),
            ("stale", &self.stale),
            ("unexpected", &self.unexpected),
        ] {
            for path in paths {
                lines.push(format!("  {label}: {}", path.display()));
            }
        }
        lines
            .push("Run generate_execass_contract --write to reconcile generated artifacts.".into());
        lines.join("\n")
    }
}

fn main() {
    let mode = match parse_mode(env::args().skip(1)) {
        Ok(mode) => mode,
        Err(message) => {
            eprintln!("{message}");
            std::process::exit(2);
        }
    };
    let repo_root = repository_root();
    let expected = expected_artifacts();
    match mode {
        Mode::Write => {
            if let Err(error) = write_artifacts(&repo_root, &expected) {
                eprintln!("failed to write ExecAss artifacts: {error}");
                std::process::exit(1);
            }
        }
        Mode::Check => match check_artifacts(&repo_root, &expected) {
            Ok(()) => {}
            Err(failure) => {
                eprintln!("{}", failure.message());
                std::process::exit(1);
            }
        },
    }
}

fn parse_mode<I>(mut args: I) -> Result<Mode, String>
where
    I: Iterator<Item = String>,
{
    match (args.next(), args.next()) {
        (Some(arg), None) if arg == "--check" => Ok(Mode::Check),
        (Some(arg), None) if arg == "--write" => Ok(Mode::Write),
        _ => Err("usage: generate_execass_contract --check | --write".to_string()),
    }
}

fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("carsinos-protocol must live under <repo>/crates")
        .to_path_buf()
}

fn schema_bytes<T: JsonSchema>() -> Vec<u8> {
    pretty_bytes(&schema_for!(T).to_value())
}

fn pretty_bytes(value: &Value) -> Vec<u8> {
    let mut bytes = serde_json::to_vec_pretty(value).expect("generated JSON must serialize");
    bytes.push(b'\n');
    bytes
}

fn expected_artifacts() -> BTreeMap<PathBuf, Vec<u8>> {
    let mut artifacts = BTreeMap::new();
    macro_rules! schema {
        ($filename:expr, $root:ty) => {
            artifacts.insert(
                PathBuf::from(SCHEMA_DIR).join(format!("{}.json", $filename)),
                schema_bytes::<$root>(),
            );
        };
    }

    schema!("intake-request", IntakeRequest);
    schema!("intake-response", IntakeResponse);
    schema!("summary-response", SummaryResponse);
    schema!("summary-ack-request", SummaryAckRequest);
    schema!("summary-ack-response", SummaryAckResponse);
    schema!("delegation-list-query", DelegationListQuery);
    schema!("delegation-list-response", DelegationListResponse);
    schema!("delegation-detail-response", DelegationDetailResponse);
    schema!("delegation-receipts-response", DelegationReceiptsResponse);
    schema!("resolve-decision-request", ResolveDecisionRequest);
    schema!("resolve-decision-response", ResolveDecisionResponse);
    schema!(
        "delegation-run-control-request",
        DelegationRunControlRequest
    );
    schema!(
        "delegation-run-control-response",
        DelegationRunControlResponse
    );
    schema!("stop-all-status-response", StopAllStatusResponse);
    schema!("stop-all-request", StopAllRequest);
    schema!("resume-all-request", ResumeAllRequest);
    schema!("resume-all-response", ResumeAllResponse);
    schema!("policy-response", PolicyResponse);
    schema!("policy-update-request", PolicyUpdateRequest);
    schema!("policy-update-response", PolicyUpdateResponse);
    schema!("local-owner-mutation-proof", LocalOwnerMutationProof);
    schema!("local-owner-mutation-binding", LocalOwnerMutationBinding);
    schema!("runtime-host-status-response", RuntimeHostStatusResponse);
    schema!("runtime-host-config-request", RuntimeHostConfigRequest);
    schema!("runtime-host-config-response", RuntimeHostConfigResponse);
    schema!("durable-event-envelope", DurableEventEnvelope);
    schema!(ERROR_SCHEMA, ApiError);

    artifacts.insert(
        PathBuf::from(OPENAPI_FILE),
        pretty_bytes(&openapi_document()),
    );
    artifacts
}

fn external_schema(name: &str) -> Value {
    json!({"$ref": format!("./schema/{name}.json")})
}

fn json_content(schema_name: &str) -> Value {
    json!({"application/json": {"schema": external_schema(schema_name)}})
}

fn error_response(description: &str) -> Value {
    json!({"description": description, "content": json_content(ERROR_SCHEMA)})
}

fn standard_errors() -> Map<String, Value> {
    BTreeMap::from([
        (
            "400".to_string(),
            error_response("Invalid ExecAss request."),
        ),
        (
            "401".to_string(),
            error_response("Authentication is required."),
        ),
        (
            "403".to_string(),
            error_response("The requested authority is denied."),
        ),
        (
            "409".to_string(),
            error_response("The resource revision or idempotency key conflicts."),
        ),
        (
            "422".to_string(),
            error_response("The requested state transition is not valid."),
        ),
        (
            "429".to_string(),
            error_response("The request is rate limited."),
        ),
        (
            "500".to_string(),
            error_response("The operation could not be completed safely."),
        ),
    ])
    .into_iter()
    .collect()
}

fn response_set(
    success_code: &str,
    description: &str,
    schema_name: &str,
    not_found: bool,
) -> Value {
    let mut responses = standard_errors();
    responses.insert(
        success_code.to_string(),
        json!({"description": description, "content": json_content(schema_name)}),
    );
    if not_found {
        responses.insert(
            "404".to_string(),
            error_response("The requested ExecAss record was not found."),
        );
    }
    Value::Object(responses)
}

fn bearer_security() -> Value {
    json!([{"bearerAuth": []}])
}

fn idempotency_parameter() -> Value {
    json!({
        "name": "Idempotency-Key",
        "in": "header",
        "required": true,
        "description": "Required stable key for safe retry. It must match the request body's idempotency_key.",
        "schema": {"type": "string", "minLength": 1},
        "x-idempotency-required": true
    })
}

fn owner_proof_parameter() -> Value {
    json!({
        "name": "X-ExecAss-Owner-Proof",
        "in": "header",
        "required": true,
        "description": "Base64url-encoded canonical native-owner proof bound to the exact mutation request.",
        "schema": {"type": "string", "minLength": 1}
    })
}

fn path_parameter(name: &str, description: &str) -> Value {
    json!({
        "name": name,
        "in": "path",
        "required": true,
        "description": description,
        "schema": {"type": "string", "minLength": 1}
    })
}

fn query_parameter(name: &str, schema: Value, description: &str) -> Value {
    json!({"name": name, "in": "query", "required": false, "description": description, "schema": schema})
}

struct OperationOptions<'a> {
    request_schema: Option<&'a str>,
    parameters: Vec<Value>,
    not_found: bool,
}

fn read_operation(parameters: Vec<Value>, not_found: bool) -> OperationOptions<'static> {
    OperationOptions {
        request_schema: None,
        parameters,
        not_found,
    }
}

fn write_operation(
    request_schema: &'static str,
    parameters: Vec<Value>,
    not_found: bool,
) -> OperationOptions<'static> {
    OperationOptions {
        request_schema: Some(request_schema),
        parameters,
        not_found,
    }
}

fn operation(
    operation_id: &str,
    summary: &str,
    response_code: &str,
    response_description: &str,
    response_schema: &str,
    mut options: OperationOptions<'_>,
) -> Value {
    let mut value = json!({
        "operationId": operation_id,
        "summary": summary,
        "security": bearer_security(),
        "responses": response_set(response_code, response_description, response_schema, options.not_found)
    });
    if let Some(request_schema) = options.request_schema {
        options.parameters.push(idempotency_parameter());
        value["requestBody"] = json!({
            "required": true,
            "content": json_content(request_schema)
        });
        value["x-idempotency-required"] = Value::Bool(true);
    }
    if !options.parameters.is_empty() {
        value["parameters"] = Value::Array(options.parameters);
    }
    value
}

fn openapi_document() -> Value {
    let mut paths = Map::new();
    paths.insert(
        "/api/v1/execass/intake".into(),
        json!({"post": operation("execassIntake", "Submit an ExecAss intake request.", "200", "The intake result.", "intake-response", write_operation("intake-request", vec![owner_proof_parameter()], false))}),
    );
    paths.insert(
        "/api/v1/execass/summary".into(),
        json!({"get": operation("getExecassSummary", "Fetch the current ExecAss summary projection.", "200", "The current summary.", "summary-response", read_operation(vec![], false))}),
    );
    paths.insert(
        "/api/v1/execass/summary/ack".into(),
        json!({"post": operation("acknowledgeExecassSummary", "Acknowledge exactly the delivered summary set.", "200", "The acknowledgement result.", "summary-ack-response", write_operation("summary-ack-request", vec![], false))}),
    );
    paths.insert(
        "/api/v1/execass/delegations".into(),
        json!({"get": operation("listExecassDelegations", "List delegations using the versioned cursor contract.", "200", "The delegation page.", "delegation-list-response", read_operation(vec![
            query_parameter("phase", json!({"type": "string"}), "Optional lifecycle phase filter."),
            query_parameter("run_control", json!({"type": "string"}), "Optional run-control filter."),
            query_parameter("cursor", json!({"type": "string"}), "Opaque pagination cursor."),
            query_parameter("limit", json!({"type": "integer", "format": "uint32", "minimum": 1}), "Maximum page size."),
        ], false))}),
    );
    paths.insert(
        "/api/v1/execass/delegations/{delegation_id}".into(),
        json!({"get": operation("getExecassDelegation", "Fetch one delegation and its immutable lineage details.", "200", "The delegation detail.", "delegation-detail-response", read_operation(vec![path_parameter("delegation_id", "Delegation identifier.")], true))}),
    );
    paths.insert(
        "/api/v1/execass/delegations/{delegation_id}/receipts".into(),
        json!({"get": operation("listExecassDelegationReceipts", "List the receipt chain for one delegation.", "200", "The receipt chain.", "delegation-receipts-response", read_operation(vec![path_parameter("delegation_id", "Delegation identifier.")], true))}),
    );
    paths.insert(
        "/api/v1/execass/decisions/{decision_id}/resolve".into(),
        json!({"post": operation("resolveExecassDecision", "Resolve one current decision against its exact revision.", "200", "The resolved decision and delegation state.", "resolve-decision-response", write_operation("resolve-decision-request", vec![path_parameter("decision_id", "Decision identifier.")], true))}),
    );
    paths.insert(
        "/api/v1/execass/delegations/{delegation_id}/stop".into(),
        json!({"post": operation("stopExecassDelegation", "Stop a delegation at its declared safe boundary.", "200", "The current delegation run-control state, including any safe-boundary drain still in progress.", "delegation-run-control-response", write_operation("delegation-run-control-request", vec![path_parameter("delegation_id", "Delegation identifier.")], true))}),
    );
    paths.insert(
        "/api/v1/execass/delegations/{delegation_id}/resume".into(),
        json!({"post": operation("resumeExecassDelegation", "Resume a delegation from fresh plan and policy snapshots.", "200", "The resumed delegation state.", "delegation-run-control-response", write_operation("delegation-run-control-request", vec![path_parameter("delegation_id", "Delegation identifier.")], true))}),
    );
    paths.insert(
        "/api/v1/execass/stop-all".into(),
        json!({
            "get": operation("getExecassStopAllStatus", "Fetch the atomic global stop state.", "200", "The global stop state.", "stop-all-status-response", read_operation(vec![], false)),
            "post": operation("engageExecassStopAll", "Atomically engage the global stop epoch.", "200", "The engaged global stop state.", "stop-all-status-response", write_operation("stop-all-request", vec![], false))
        }),
    );
    paths.insert(
        "/api/v1/execass/resume-all".into(),
        json!({"post": operation("resumeExecassAll", "Resume all work from verified owner ingress.", "200", "The resumed global stop state.", "resume-all-response", write_operation("resume-all-request", vec![], false))}),
    );
    paths.insert(
        "/api/v1/execass/policy".into(),
        json!({
            "get": operation("getExecassPolicy", "Fetch the effective operational policy.", "200", "The current policy.", "policy-response", read_operation(vec![], false)),
            "put": operation("updateExecassPolicy", "Apply an exact owner policy amendment through the canonical intake transaction.", "200", "The updated policy.", "policy-update-response", write_operation("policy-update-request", vec![owner_proof_parameter()], false))
        }),
    );
    paths.insert(
        "/api/v1/execass/runtime-host".into(),
        json!({
            "get": operation("getExecassRuntimeHost", "Fetch the current single-host runtime state.", "200", "The runtime-host state.", "runtime-host-status-response", read_operation(vec![], false)),
            "put": operation("configureExecassRuntimeHost", "Update runtime-host settings using a bounded revision.", "200", "The updated runtime-host configuration.", "runtime-host-config-response", write_operation("runtime-host-config-request", vec![owner_proof_parameter()], false))
        }),
    );

    json!({
        "openapi": "3.1.0",
        "info": {
            "title": "CarsinOS ExecAss API",
            "version": EXECASS_SCHEMA_VERSION,
            "description": "Versioned ExecAss control-plane contract generated from carsinos-protocol DTO schemas."
        },
        "x-execass-api-version": EXECASS_API_VERSION,
        "servers": [{"url": "/"}],
        "security": bearer_security(),
        "paths": paths,
        "components": {
            "securitySchemes": {
                "bearerAuth": {"type": "http", "scheme": "bearer", "bearerFormat": "opaque"}
            }
        }
    })
}

fn write_artifacts(repo_root: &Path, expected: &BTreeMap<PathBuf, Vec<u8>>) -> io::Result<()> {
    let schema_dir = repo_root.join(SCHEMA_DIR);
    if let Ok(entries) = fs::read_dir(&schema_dir) {
        let expected_schema_paths: BTreeSet<_> = expected
            .keys()
            .filter(|path| path.starts_with(SCHEMA_DIR))
            .cloned()
            .collect();
        for entry in entries {
            let entry = entry?;
            let path = entry.path();
            if path.is_file()
                && path
                    .extension()
                    .is_some_and(|extension| extension == "json")
            {
                let relative = PathBuf::from(SCHEMA_DIR).join(entry.file_name());
                if !expected_schema_paths.contains(&relative) {
                    fs::remove_file(path)?;
                }
            }
        }
    }
    for (relative_path, bytes) in expected {
        let path = repo_root.join(relative_path);
        let parent = path.parent().expect("artifact path must have a parent");
        fs::create_dir_all(parent)?;
        fs::write(path, bytes)?;
    }
    Ok(())
}

fn check_artifacts(
    repo_root: &Path,
    expected: &BTreeMap<PathBuf, Vec<u8>>,
) -> Result<(), CheckFailure> {
    let mut failure = CheckFailure {
        missing: Vec::new(),
        stale: Vec::new(),
        unexpected: Vec::new(),
    };
    for (relative_path, expected_bytes) in expected {
        let path = repo_root.join(relative_path);
        match fs::read(&path) {
            Ok(actual_bytes) if actual_bytes == *expected_bytes => {}
            Ok(_) => failure.stale.push(relative_path.clone()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                failure.missing.push(relative_path.clone())
            }
            Err(_) => failure.stale.push(relative_path.clone()),
        }
    }

    let schema_dir = repo_root.join(SCHEMA_DIR);
    if let Ok(entries) = fs::read_dir(schema_dir) {
        let expected_schema_paths: BTreeSet<_> = expected
            .keys()
            .filter(|path| path.starts_with(SCHEMA_DIR))
            .cloned()
            .collect();
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file()
                && path
                    .extension()
                    .is_some_and(|extension| extension == "json")
            {
                let relative = PathBuf::from(SCHEMA_DIR).join(entry.file_name());
                if !expected_schema_paths.contains(&relative) {
                    failure.unexpected.push(relative);
                }
            }
        }
    }

    if failure.is_empty() {
        Ok(())
    } else {
        failure.missing.sort();
        failure.stale.sort();
        failure.unexpected.sort();
        Err(failure)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static TEST_DIRECTORY_COUNTER: AtomicUsize = AtomicUsize::new(0);

    fn temporary_project_root() -> PathBuf {
        let suffix = TEST_DIRECTORY_COUNTER.fetch_add(1, Ordering::Relaxed);
        repository_root().join(format!(
            ".execass-generator-test-{}-{suffix}",
            std::process::id()
        ))
    }

    fn operations(document: &Value) -> Vec<&Map<String, Value>> {
        document["paths"]
            .as_object()
            .expect("paths object")
            .values()
            .flat_map(|path_item| {
                path_item
                    .as_object()
                    .expect("path item object")
                    .values()
                    .filter_map(Value::as_object)
            })
            .collect()
    }

    #[test]
    fn generated_schema_roots_are_json_schema_2020_12() {
        let artifacts = expected_artifacts();
        let schema_paths: Vec<_> = artifacts
            .keys()
            .filter(|path| path.starts_with(SCHEMA_DIR))
            .collect();
        // Includes the operation-closed local owner mutation binding and proof
        // roots in addition to the original 25 checked contract schemas.
        assert_eq!(schema_paths.len(), 27);
        for schema_path in schema_paths {
            let schema: Value =
                serde_json::from_slice(&artifacts[schema_path]).expect("schema JSON");
            assert_eq!(
                schema["$schema"],
                "https://json-schema.org/draft/2020-12/schema"
            );
            assert!(artifacts[schema_path].ends_with(b"\n"));
        }
    }

    #[test]
    fn openapi_covers_every_locked_route_and_method() {
        let document = openapi_document();
        assert_eq!(document["openapi"], "3.1.0");
        assert_eq!(document["info"]["version"], EXECASS_SCHEMA_VERSION);
        assert_eq!(document["x-execass-api-version"], EXECASS_API_VERSION);
        let expected = [
            ("/api/v1/execass/intake", "post"),
            ("/api/v1/execass/summary", "get"),
            ("/api/v1/execass/summary/ack", "post"),
            ("/api/v1/execass/delegations", "get"),
            ("/api/v1/execass/delegations/{delegation_id}", "get"),
            (
                "/api/v1/execass/delegations/{delegation_id}/receipts",
                "get",
            ),
            ("/api/v1/execass/decisions/{decision_id}/resolve", "post"),
            ("/api/v1/execass/delegations/{delegation_id}/stop", "post"),
            ("/api/v1/execass/delegations/{delegation_id}/resume", "post"),
            ("/api/v1/execass/stop-all", "get"),
            ("/api/v1/execass/stop-all", "post"),
            ("/api/v1/execass/resume-all", "post"),
            ("/api/v1/execass/policy", "get"),
            ("/api/v1/execass/policy", "put"),
            ("/api/v1/execass/runtime-host", "get"),
            ("/api/v1/execass/runtime-host", "put"),
        ];
        for (path, method) in expected {
            assert!(
                document["paths"][path].get(method).is_some(),
                "missing {method} {path}"
            );
        }
        for path in [
            "/api/v1/execass/intake",
            "/api/v1/execass/policy",
            "/api/v1/execass/runtime-host",
        ] {
            let method = if path.ends_with("intake") {
                "post"
            } else {
                "put"
            };
            let parameters = document["paths"][path][method]["parameters"]
                .as_array()
                .expect("owner mutation parameters");
            assert!(parameters.iter().any(|parameter| {
                parameter["name"] == "X-ExecAss-Owner-Proof"
                    && parameter["in"] == "header"
                    && parameter["required"] == true
            }));
        }
        for stale_path in [
            "/api/v1/execass/delegations/{delegation_id}/decisions/{decision_id}/resolve",
            "/api/v1/execass/policy/changes",
            "/api/v1/execass/policy/changes/{change_id}/confirm",
            "/api/v1/execass/policy/change-requests",
            "/api/v1/execass/policy/change-requests/{change_id}/confirm",
        ] {
            assert!(
                document["paths"].get(stale_path).is_none(),
                "obsolete path must not remain: {stale_path}"
            );
        }
    }

    #[test]
    fn every_operation_requires_bearer_and_safe_error_responses() {
        for operation in operations(&openapi_document()) {
            assert_eq!(operation["security"], bearer_security());
            for status in ["400", "401", "403", "409", "422", "429", "500"] {
                assert_eq!(
                    operation["responses"][status]["content"]["application/json"]["schema"]["$ref"],
                    "./schema/api-error.json"
                );
            }
            if operation.get("requestBody").is_some() {
                assert_eq!(operation["x-idempotency-required"], true);
                assert!(operation["parameters"]
                    .as_array()
                    .expect("parameters")
                    .iter()
                    .any(|parameter| {
                        parameter["name"] == "Idempotency-Key"
                            && parameter["in"] == "header"
                            && parameter["required"] == true
                            && parameter["x-idempotency-required"] == true
                    }));
            }
        }
    }

    #[test]
    fn generated_contract_rejects_retired_authority_and_finance_vocabulary() {
        for (path, bytes) in expected_artifacts() {
            let text = String::from_utf8(bytes).expect("generated contract is UTF-8");
            for prohibited in [
                "budget",
                "currency",
                "payee",
                "purchase",
                "financial",
                "hard_lock",
                "fresh_local",
                "local_presence",
                "policy-change-confirm",
                "change-requests",
            ] {
                assert!(
                    !text.contains(prohibited),
                    "{} retains prohibited vocabulary: {prohibited}",
                    path.display()
                );
            }
        }
    }

    #[test]
    fn check_detects_deliberate_artifact_drift_without_rewriting() {
        let project_root = temporary_project_root();
        let artifacts = expected_artifacts();
        write_artifacts(&project_root, &artifacts).expect("write test artifacts");
        check_artifacts(&project_root, &artifacts).expect("fresh artifacts pass check");

        let changed = project_root.join(SCHEMA_DIR).join("intake-request.json");
        let original = fs::read(&changed).expect("read generated schema");
        fs::write(&changed, b"{\n  \"drift\": true\n}\n").expect("write deliberate drift");
        let failure =
            check_artifacts(&project_root, &artifacts).expect_err("drift must fail check");
        assert!(failure
            .stale
            .contains(&PathBuf::from(SCHEMA_DIR).join("intake-request.json")));
        assert_ne!(
            fs::read(&changed).expect("read drift"),
            original,
            "check must not rewrite"
        );

        fs::remove_dir_all(&project_root).expect("remove project-bound test directory");
    }
}
