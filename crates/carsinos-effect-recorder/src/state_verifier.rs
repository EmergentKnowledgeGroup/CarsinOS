use carsinos_protocol::execass_recorder::ExecuteOnceV1;
use rusqlite::{Connection, OpenFlags, OptionalExtension};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

#[derive(Debug, thiserror::Error)]
pub enum VerificationError {
    #[error("opening authoritative state read-only failed: {0}")]
    Open(#[source] rusqlite::Error),
    #[error("reading authoritative state failed: {0}")]
    Query(#[source] rusqlite::Error),
    #[error("the exact committed Began attempt and live fence were not proven")]
    NotAuthoritative,
    #[error("execute-once deadline is not live")]
    Expired,
}

/// Opaque proof that the recorder, not its caller, read and matched the exact
/// committed `invoking` attempt/effect plus every live host/claim fence.
pub(crate) struct VerifiedExecuteOnceAdmission {
    command: ExecuteOnceV1,
    technical_resource_reservations: Vec<VerifiedTechnicalResourceReservation>,
}

impl VerifiedExecuteOnceAdmission {
    pub(crate) fn command(&self) -> &ExecuteOnceV1 {
        &self.command
    }

    pub(crate) fn technical_resource_reservations(
        &self,
    ) -> &[VerifiedTechnicalResourceReservation] {
        &self.technical_resource_reservations
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct VerifiedTechnicalResourceReservation {
    pub reservation_id: String,
    pub amount_reserved: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct StoredReservationIdentity {
    reservation_id: String,
    quota_snapshot_id: String,
    logical_effect_id: String,
    technical_resource_kind: String,
    unit: String,
    amount_reserved: i64,
}

#[derive(Debug, Clone)]
pub struct ReadOnlyBeganVerifier {
    database_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthoritativeRecorderBinding {
    pub canonical_root_identity: String,
    pub installation_id: String,
    pub state_root_generation: i64,
    pub os_user_identity_digest: String,
    pub runtime_host_generation: i64,
    pub runtime_host_instance_id: String,
    pub runtime_fencing_token: i64,
}

impl ReadOnlyBeganVerifier {
    pub fn new(database_path: impl Into<PathBuf>) -> Self {
        Self {
            database_path: database_path.into(),
        }
    }

    pub fn database_path(&self) -> &Path {
        &self.database_path
    }

    pub fn load_authoritative_binding(
        &self,
        trusted_now_ms: i64,
    ) -> Result<AuthoritativeRecorderBinding, VerificationError> {
        let connection = Connection::open_with_flags(
            &self.database_path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .map_err(VerificationError::Open)?;
        connection
            .pragma_update(None, "query_only", true)
            .map_err(VerificationError::Query)?;
        connection
            .execute_batch("BEGIN DEFERRED")
            .map_err(VerificationError::Query)?;
        connection
            .query_row(
                r#"SELECT k.canonical_root_identity,g.installation_identity,
                          g.state_root_generation,g.os_user_identity_digest,
                          l.generation,l.host_instance_id,l.fencing_token
                   FROM execass_runtime_host_leases l
                   JOIN execass_runtime_host_generations g
                     ON g.generation=l.generation AND g.host_instance_id=l.host_instance_id
                   JOIN execass_confirmation_authority_keys k
                     ON k.status='active'
                    AND k.installation_identity=g.installation_identity
                    AND k.os_user_identity_digest=g.os_user_identity_digest
                    AND k.state_root_generation=g.state_root_generation
                   WHERE l.ownership_scope='execass'
                     AND l.released_at IS NULL AND l.expires_at>?1"#,
                [trusted_now_ms],
                |row| {
                    Ok(AuthoritativeRecorderBinding {
                        canonical_root_identity: row.get(0)?,
                        installation_id: row.get(1)?,
                        state_root_generation: row.get(2)?,
                        os_user_identity_digest: row.get(3)?,
                        runtime_host_generation: row.get(4)?,
                        runtime_host_instance_id: row.get(5)?,
                        runtime_fencing_token: row.get(6)?,
                    })
                },
            )
            .map_err(|error| match error {
                rusqlite::Error::QueryReturnedNoRows => VerificationError::NotAuthoritative,
                other => VerificationError::Query(other),
            })
    }

    pub(crate) fn verify(
        &self,
        command: &ExecuteOnceV1,
        trusted_now_ms: i64,
    ) -> Result<VerifiedExecuteOnceAdmission, VerificationError> {
        if command.deadline_ms <= trusted_now_ms {
            return Err(VerificationError::Expired);
        }
        if command
            .derived_provider_request_digest()
            .map_err(|_| VerificationError::NotAuthoritative)?
            != command.provider_request_digest
        {
            return Err(VerificationError::NotAuthoritative);
        }
        let connection = Connection::open_with_flags(
            &self.database_path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .map_err(VerificationError::Open)?;
        connection
            .pragma_update(None, "query_only", true)
            .map_err(VerificationError::Query)?;
        connection
            .execute_batch("BEGIN DEFERRED")
            .map_err(VerificationError::Query)?;
        let proven = connection
            .query_row(
                r#"SELECT 1
                   FROM execass_provider_attempts a
                   JOIN execass_logical_effects e
                     ON e.delegation_id=a.delegation_id
                    AND e.logical_effect_id=a.logical_effect_id
                   JOIN execass_continuation_operation_history h
                     ON h.event_id=a.claim_event_id AND h.operation='claim'
                   JOIN execass_continuations c
                     ON c.delegation_id=a.delegation_id
                    AND c.continuation_id=a.continuation_id
                   JOIN jobs j ON j.job_id=h.job_id
                   JOIN execass_runtime_host_generations g
                     ON g.generation=a.host_generation
                    AND g.host_instance_id=a.host_instance_id
                   JOIN execass_runtime_host_leases l
                     ON l.ownership_scope='execass'
                    AND l.generation=a.host_generation
                    AND l.host_instance_id=a.host_instance_id
                    AND l.fencing_token=a.runtime_fencing_token
                   JOIN execass_confirmation_authority_keys k
                     ON k.status='active'
                   JOIN execass_global_runtime_control control ON control.singleton=1
                   WHERE a.attempt_id=?1
                     AND a.status='invoking' AND e.state='invoking'
                     AND a.attempt_number=?2
                     AND a.delegation_id=?3 AND a.continuation_id=?4 AND a.action_id=?5
                     AND a.logical_effect_id=?6
                     AND a.claim_event_id=?7 AND a.claim_receipt_id=?8
                     AND a.fencing_token=?9
                     AND a.host_generation=?10 AND a.host_instance_id=?11
                     AND a.runtime_fencing_token=?12
                     AND a.provider_request_digest=?13
                     AND e.internal_idempotency_key=?14
                     AND e.provider_identity=?15
                     AND e.provider_idempotency_key IS ?16
                     AND e.reconciliation_key IS ?17
                     AND e.manifest_digest=?18 AND e.payload_digest=?19
                     AND h.claim_event_id=?7 AND h.claim_receipt_id=?8
                     AND h.continuation_id=?4 AND h.delegation_id=?3 AND h.action_id=?5
                     AND h.continuation_fencing_token=?9
                     AND h.runtime_host_generation=?10
                     AND h.runtime_host_instance_id=?11
                     AND h.runtime_fencing_token=?12
                     AND h.state_root_generation=?20
                     AND c.status='executing' AND c.job_id=h.job_id
                     AND c.lease_owner=h.worker_id
                     AND c.lease_expires_at=h.job_lease_expires_at
                     AND c.lease_expires_at>?21
                     AND c.fencing_token=?9 AND c.host_generation=?10
                     AND j.enabled=1 AND j.deleted_at IS NULL
                     AND j.lease_owner=h.worker_id
                     AND j.lease_expires_at=h.job_lease_expires_at
                     AND j.lease_expires_at>?21
                     AND json_extract(j.payload_json,'$.mode')='execass.continuation'
                     AND l.released_at IS NULL AND l.expires_at>?21
                     AND g.state_root_generation=?20
                     AND g.installation_identity=?22
                     AND g.os_user_identity_digest=?23
                     AND k.canonical_root_identity=?24
                     AND k.installation_identity=?22
                     AND k.os_user_identity_digest=?23
                     AND k.state_root_generation=?20
                     AND control.engaged=0
                     AND control.global_stop_epoch=h.global_stop_epoch
                   LIMIT 1"#,
                rusqlite::params![
                    command.attempt_id,
                    command.attempt_number,
                    command.delegation_id,
                    command.continuation_id,
                    command.action_id,
                    command.logical_effect_id,
                    command.claim_event_id,
                    command.claim_receipt_id,
                    command.continuation_fencing_token,
                    command.binding.runtime_host_generation,
                    command.binding.runtime_host_instance_id,
                    command.binding.runtime_fencing_token,
                    command.provider_request_digest,
                    command.internal_idempotency_key,
                    command.provider_identity,
                    command.provider_idempotency_key,
                    command.reconciliation_key,
                    command.manifest_digest,
                    command.payload_digest,
                    command.binding.state_root_generation,
                    trusted_now_ms,
                    command.binding.installation_id,
                    command.binding.os_user_identity_digest,
                    command.binding.canonical_root_identity,
                ],
                |_| Ok(()),
            )
            .optional()
            .map_err(VerificationError::Query)?
            .is_some();
        if !proven {
            return Err(VerificationError::NotAuthoritative);
        }
        let technical_resource_reservations =
            if command.provider_identity == crate::EXACT_OVERWRITE_PROVIDER_IDENTITY {
                let (stored_reservation_json, stored_reservation_digest) = connection
                    .query_row(
                        r#"SELECT technical_resource_reservation_set_json,
                                  technical_resource_reservation_set_digest
                           FROM execass_continuation_operation_history
                           WHERE event_id=?1 AND operation='claim'
                             AND claim_event_id=?1 AND claim_receipt_id=?2
                             AND delegation_id=?3 AND continuation_id=?4
                             AND continuation_fencing_token=?5
                             AND runtime_host_generation=?6
                             AND runtime_fencing_token=?7"#,
                        rusqlite::params![
                            command.claim_event_id,
                            command.claim_receipt_id,
                            command.delegation_id,
                            command.continuation_id,
                            command.continuation_fencing_token,
                            command.binding.runtime_host_generation,
                            command.binding.runtime_fencing_token,
                        ],
                        |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
                    )
                    .map_err(|error| match error {
                        rusqlite::Error::QueryReturnedNoRows => VerificationError::NotAuthoritative,
                        other => VerificationError::Query(other),
                    })?;
                load_verified_reservations(
                    &connection,
                    command,
                    trusted_now_ms,
                    &stored_reservation_json,
                    &stored_reservation_digest,
                    true,
                    true,
                )?
            } else {
                Vec::new()
            };
        Ok(VerifiedExecuteOnceAdmission {
            command: command.clone(),
            technical_resource_reservations,
        })
    }

    pub(crate) fn verify_reconciliation_reservations(
        &self,
        attempt_id: &str,
        logical_effect_id: &str,
        provider_request_digest: &str,
        trusted_now_ms: i64,
    ) -> Result<Vec<VerifiedTechnicalResourceReservation>, VerificationError> {
        let connection = Connection::open_with_flags(
            &self.database_path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .map_err(VerificationError::Open)?;
        connection
            .pragma_update(None, "query_only", true)
            .map_err(VerificationError::Query)?;
        connection
            .execute_batch("BEGIN DEFERRED")
            .map_err(VerificationError::Query)?;
        let proof = connection
            .query_row(
                r#"SELECT a.claim_event_id,a.claim_receipt_id,a.delegation_id,
                          a.continuation_id,a.fencing_token,a.host_generation,
                          a.runtime_fencing_token,h.technical_resource_reservation_set_json,
                          h.technical_resource_reservation_set_digest
                   FROM execass_provider_attempts a
                   JOIN execass_continuation_operation_history h
                     ON h.event_id=a.claim_event_id AND h.operation='claim'
                   WHERE a.attempt_id=?1 AND a.logical_effect_id=?2
                     AND a.provider_request_digest=?3
                     AND h.claim_event_id=a.claim_event_id
                     AND h.claim_receipt_id=a.claim_receipt_id
                     AND h.delegation_id=a.delegation_id
                     AND h.continuation_id=a.continuation_id
                     AND h.continuation_fencing_token=a.fencing_token
                     AND h.runtime_host_generation=a.host_generation
                     AND h.runtime_fencing_token=a.runtime_fencing_token
                   LIMIT 1"#,
                rusqlite::params![attempt_id, logical_effect_id, provider_request_digest],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, i64>(4)?,
                        row.get::<_, i64>(5)?,
                        row.get::<_, i64>(6)?,
                        row.get::<_, String>(7)?,
                        row.get::<_, String>(8)?,
                    ))
                },
            )
            .optional()
            .map_err(VerificationError::Query)?;
        let Some((
            claim_event_id,
            claim_receipt_id,
            delegation_id,
            continuation_id,
            continuation_fencing_token,
            runtime_host_generation,
            runtime_fencing_token,
            stored_json,
            stored_digest,
        )) = proof
        else {
            return Err(VerificationError::NotAuthoritative);
        };
        let identity = ReservationQueryIdentity {
            claim_event_id: &claim_event_id,
            claim_receipt_id: &claim_receipt_id,
            delegation_id: &delegation_id,
            continuation_id: &continuation_id,
            logical_effect_id,
            continuation_fencing_token,
            runtime_host_generation,
            runtime_fencing_token,
        };
        load_verified_reservations_for_identity(
            &connection,
            &identity,
            trusted_now_ms,
            &stored_json,
            &stored_digest,
            false,
            true,
        )
    }
}

struct ReservationQueryIdentity<'a> {
    claim_event_id: &'a str,
    claim_receipt_id: &'a str,
    delegation_id: &'a str,
    continuation_id: &'a str,
    logical_effect_id: &'a str,
    continuation_fencing_token: i64,
    runtime_host_generation: i64,
    runtime_fencing_token: i64,
}

fn load_verified_reservations(
    connection: &Connection,
    command: &ExecuteOnceV1,
    trusted_now_ms: i64,
    stored_json: &str,
    stored_digest: &str,
    require_live: bool,
    require_nonempty: bool,
) -> Result<Vec<VerifiedTechnicalResourceReservation>, VerificationError> {
    load_verified_reservations_for_identity(
        connection,
        &ReservationQueryIdentity {
            claim_event_id: &command.claim_event_id,
            claim_receipt_id: &command.claim_receipt_id,
            delegation_id: &command.delegation_id,
            continuation_id: &command.continuation_id,
            logical_effect_id: &command.logical_effect_id,
            continuation_fencing_token: command.continuation_fencing_token,
            runtime_host_generation: command.binding.runtime_host_generation,
            runtime_fencing_token: command.binding.runtime_fencing_token,
        },
        trusted_now_ms,
        stored_json,
        stored_digest,
        require_live,
        require_nonempty,
    )
}

fn load_verified_reservations_for_identity(
    connection: &Connection,
    identity: &ReservationQueryIdentity<'_>,
    trusted_now_ms: i64,
    stored_json: &str,
    stored_digest: &str,
    require_live: bool,
    require_nonempty: bool,
) -> Result<Vec<VerifiedTechnicalResourceReservation>, VerificationError> {
    let stored: Vec<StoredReservationIdentity> =
        serde_json::from_str(stored_json).map_err(|_| VerificationError::NotAuthoritative)?;
    let mut statement = connection
        .prepare(
            r#"SELECT reservation_id,quota_snapshot_id,logical_effect_id,
                      technical_resource_kind,unit,amount_reserved,status,
                      delegation_id,continuation_id,claim_receipt_id,
                      continuation_fencing_token,runtime_host_generation,
                      runtime_fencing_token,expires_at
               FROM execass_technical_resource_reservations
               WHERE claim_event_id=?1
               ORDER BY technical_resource_kind,unit"#,
        )
        .map_err(VerificationError::Query)?;
    let rows = statement
        .query_map([identity.claim_event_id], |row| {
            Ok((
                StoredReservationIdentity {
                    reservation_id: row.get(0)?,
                    quota_snapshot_id: row.get(1)?,
                    logical_effect_id: row.get(2)?,
                    technical_resource_kind: row.get(3)?,
                    unit: row.get(4)?,
                    amount_reserved: row.get(5)?,
                },
                row.get::<_, String>(6)?,
                row.get::<_, String>(7)?,
                row.get::<_, String>(8)?,
                row.get::<_, String>(9)?,
                row.get::<_, i64>(10)?,
                row.get::<_, i64>(11)?,
                row.get::<_, i64>(12)?,
                row.get::<_, i64>(13)?,
            ))
        })
        .map_err(VerificationError::Query)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(VerificationError::Query)?;
    let current = rows.iter().map(|row| row.0.clone()).collect::<Vec<_>>();
    let canonical =
        serde_json::to_string(&current).map_err(|_| VerificationError::NotAuthoritative)?;
    if (require_nonempty && stored.is_empty())
        || stored != current
        || canonical != stored_json
        || reservation_set_digest(canonical.as_bytes()) != stored_digest
        || rows.iter().any(|row| {
            let reservation = &row.0;
            reservation.logical_effect_id != identity.logical_effect_id
                || row.2 != identity.delegation_id
                || row.3 != identity.continuation_id
                || row.4 != identity.claim_receipt_id
                || row.5 != identity.continuation_fencing_token
                || row.6 != identity.runtime_host_generation
                || row.7 != identity.runtime_fencing_token
                || (require_live && (row.1 != "reserved" || row.8 <= trusted_now_ms))
                || (!require_live && row.1 != "reserved" && row.1 != "reconciliation_required")
        })
    {
        return Err(VerificationError::NotAuthoritative);
    }
    Ok(current
        .into_iter()
        .map(|reservation| VerifiedTechnicalResourceReservation {
            reservation_id: reservation.reservation_id,
            amount_reserved: reservation.amount_reserved,
        })
        .collect())
}

fn reservation_set_digest(canonical_json: &[u8]) -> String {
    let mut digest = Sha256::new();
    digest.update(b"carsinos.execass.technical-resource.reservation-set.v1\0");
    digest.update(canonical_json);
    format!("sha256:{}", crate::hex_encode(&digest.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reservation_fixture() -> (Connection, StoredReservationIdentity, String, String, i64) {
        let connection = Connection::open_in_memory().unwrap();
        connection
            .execute_batch(
                r#"CREATE TABLE execass_technical_resource_reservations (
                     reservation_id TEXT NOT NULL,
                     quota_snapshot_id TEXT NOT NULL,
                     logical_effect_id TEXT NOT NULL,
                     technical_resource_kind TEXT NOT NULL,
                     unit TEXT NOT NULL,
                     amount_reserved INTEGER NOT NULL,
                     status TEXT NOT NULL,
                     delegation_id TEXT NOT NULL,
                     continuation_id TEXT NOT NULL,
                     claim_event_id TEXT NOT NULL,
                     claim_receipt_id TEXT NOT NULL,
                     continuation_fencing_token INTEGER NOT NULL,
                     runtime_host_generation INTEGER NOT NULL,
                     runtime_fencing_token INTEGER NOT NULL,
                     expires_at INTEGER NOT NULL
                   );"#,
            )
            .unwrap();
        let reservation = StoredReservationIdentity {
            reservation_id: "reservation-1".into(),
            quota_snapshot_id: "quota-1".into(),
            logical_effect_id: "effect-1".into(),
            technical_resource_kind: "connector_calls".into(),
            unit: format!("connector:{}", "a".repeat(64)),
            amount_reserved: 1,
        };
        let expires_at = 10_000;
        connection
            .execute(
                r#"INSERT INTO execass_technical_resource_reservations (
                     reservation_id,quota_snapshot_id,logical_effect_id,
                     technical_resource_kind,unit,amount_reserved,status,
                     delegation_id,continuation_id,claim_event_id,claim_receipt_id,
                     continuation_fencing_token,runtime_host_generation,
                     runtime_fencing_token,expires_at
                   ) VALUES (?1,?2,?3,?4,?5,?6,'reserved','delegation-1',
                             'continuation-1','claim-1','receipt-1',2,3,4,?7)"#,
                rusqlite::params![
                    reservation.reservation_id,
                    reservation.quota_snapshot_id,
                    reservation.logical_effect_id,
                    reservation.technical_resource_kind,
                    reservation.unit,
                    reservation.amount_reserved,
                    expires_at,
                ],
            )
            .unwrap();
        let json = serde_json::to_string(std::slice::from_ref(&reservation)).unwrap();
        let digest = reservation_set_digest(json.as_bytes());
        (connection, reservation, json, digest, expires_at)
    }

    fn query_identity() -> ReservationQueryIdentity<'static> {
        ReservationQueryIdentity {
            claim_event_id: "claim-1",
            claim_receipt_id: "receipt-1",
            delegation_id: "delegation-1",
            continuation_id: "continuation-1",
            logical_effect_id: "effect-1",
            continuation_fencing_token: 2,
            runtime_host_generation: 3,
            runtime_fencing_token: 4,
        }
    }

    #[test]
    fn reservation_proof_is_nonempty_exact_and_caller_independent() {
        let (connection, reservation, json, digest, expires_at) = reservation_fixture();
        let verified = load_verified_reservations_for_identity(
            &connection,
            &query_identity(),
            expires_at - 1,
            &json,
            &digest,
            true,
            true,
        )
        .unwrap();
        assert_eq!(
            verified,
            vec![VerifiedTechnicalResourceReservation {
                reservation_id: reservation.reservation_id,
                amount_reserved: reservation.amount_reserved,
            }]
        );

        connection
            .execute("DELETE FROM execass_technical_resource_reservations", [])
            .unwrap();
        assert!(matches!(
            load_verified_reservations_for_identity(
                &connection,
                &query_identity(),
                expires_at - 1,
                &json,
                &digest,
                true,
                true,
            ),
            Err(VerificationError::NotAuthoritative)
        ));
    }

    #[test]
    fn reservation_set_digest_or_live_identity_mismatch_fails_closed() {
        let (connection, _, json, digest, expires_at) = reservation_fixture();
        assert!(matches!(
            load_verified_reservations_for_identity(
                &connection,
                &query_identity(),
                expires_at - 1,
                &json,
                &format!("sha256:{}", "0".repeat(64)),
                true,
                true,
            ),
            Err(VerificationError::NotAuthoritative)
        ));
        connection
            .execute(
                "UPDATE execass_technical_resource_reservations SET amount_reserved=2",
                [],
            )
            .unwrap();
        assert!(matches!(
            load_verified_reservations_for_identity(
                &connection,
                &query_identity(),
                expires_at - 1,
                &json,
                &digest,
                true,
                true,
            ),
            Err(VerificationError::NotAuthoritative)
        ));
    }
}
