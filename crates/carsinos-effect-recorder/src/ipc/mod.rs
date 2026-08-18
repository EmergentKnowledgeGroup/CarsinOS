use crate::service::RecorderService;
use crate::RecorderIdentity;
use carsinos_protocol::execass_recorder::{
    decode_frame, encode_frame, recorder_observation_signing_bytes, QueryOnlyV1, RecorderBindingV1,
    RecorderHandshakeAttestationV1, RecorderHandshakeChallengeV1, RecorderReplyV1,
    RecorderRequestV1, RECORDER_HANDSHAKE_VERSION, RECORDER_MAX_FRAME_BYTES,
};
use ed25519_dalek::{Signature, VerifyingKey};
use sha2::{Digest, Sha256};
use std::path::Path;
#[cfg(unix)]
use std::path::PathBuf;
use std::sync::Arc;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

#[cfg(unix)]
mod unix;
#[cfg(windows)]
mod windows;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecorderEndpoint {
    #[cfg(windows)]
    WindowsPipe(String),
    #[cfg(unix)]
    UnixSocket(PathBuf),
}

impl RecorderEndpoint {
    pub fn for_binding(
        state_root: &Path,
        installation_id: &str,
        state_root_generation: i64,
    ) -> Self {
        #[cfg(windows)]
        {
            let _ = state_root;
            let identity = format!("{installation_id}\0{state_root_generation}");
            let digest = crate::hex_encode(&Sha256::digest(identity.as_bytes()));
            Self::WindowsPipe(format!(
                r"\\.\pipe\carsinos-execass-recorder-v1-{}",
                &digest[..32]
            ))
        }
        #[cfg(unix)]
        {
            let _ = (installation_id, state_root_generation);
            Self::UnixSocket(state_root.join("runtime/effect-recorder/v1/recorder.sock"))
        }
    }
}

pub struct RecorderClient {
    endpoint: RecorderEndpoint,
    channel_key: zeroize::Zeroizing<[u8; 32]>,
    expected_identity: RecorderIdentity,
    #[cfg(feature = "test-support")]
    test_transport: Option<TestRecorderTransport>,
}

impl Clone for RecorderClient {
    fn clone(&self) -> Self {
        Self {
            endpoint: self.endpoint.clone(),
            channel_key: zeroize::Zeroizing::new(*self.channel_key),
            expected_identity: self.expected_identity.clone(),
            #[cfg(feature = "test-support")]
            test_transport: self.test_transport.clone(),
        }
    }
}

impl std::fmt::Debug for RecorderClient {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RecorderClient")
            .field("endpoint", &self.endpoint)
            .field("expected_identity", &self.expected_identity)
            .field("channel_key", &"[REDACTED]")
            .finish()
    }
}

impl RecorderClient {
    pub fn new(
        endpoint: RecorderEndpoint,
        channel_key: [u8; 32],
        expected_identity: RecorderIdentity,
    ) -> Self {
        Self {
            endpoint,
            channel_key: zeroize::Zeroizing::new(channel_key),
            expected_identity,
            #[cfg(feature = "test-support")]
            test_transport: None,
        }
    }

    pub async fn send(&self, mut request: RecorderRequestV1) -> anyhow::Result<RecorderReplyV1> {
        self.expected_identity
            .validate_for_root(&request.binding().canonical_root_identity)?;
        crate::sign_request(&mut request, &self.channel_key)?;
        #[cfg(feature = "test-support")]
        if let Some(transport) = &self.test_transport {
            return transport.exchange(request).await;
        }
        platform::client_exchange(&self.endpoint, &request, &self.expected_identity).await
    }

    /// Proves that the sidecar holding the recorder endpoint possesses the
    /// validated recorder signing key before the caller pins a first-use
    /// identity. QueryOnly has no provider invocation authority.
    pub async fn prove_recorder_possession(
        &self,
        binding: &RecorderBindingV1,
    ) -> anyhow::Result<()> {
        self.expected_identity
            .validate_for_root(&binding.canonical_root_identity)?;
        let mut nonce = [0u8; 32];
        getrandom::fill(&mut nonce)?;
        let probe = RecorderRequestV1::QueryOnly(Box::new(QueryOnlyV1 {
            binding: binding.clone(),
            request_id: format!("recorder-possession-proof-{}", crate::hex_encode(&nonce)),
            // This reserved synthetic attempt is never an ExecuteOnce target.
            attempt_id: format!("__recorder-possession-proof-{}", crate::hex_encode(&nonce)),
            expected_command_digest: None,
            known_journal_head: None,
            client_nonce: crate::hex_encode(&nonce),
            command_mac: String::new(),
        }));
        let _ = self.send(probe).await?;
        Ok(())
    }
}

/// Test-only sealed IPC substitute. It is attached to `RecorderClient`, so
/// gateway tests still execute production client signing and routing, but it
/// cannot be named by production dependency builds.
#[cfg(feature = "test-support")]
#[derive(Clone, Default)]
pub struct TestRecorderTransport {
    requests: Arc<tokio::sync::Mutex<Vec<RecorderRequestV1>>>,
    service: Option<Arc<RecorderService>>,
}

#[cfg(feature = "test-support")]
impl TestRecorderTransport {
    /// Routes signed client requests through the real recorder service while
    /// retaining the in-memory request capture used by gateway tests.
    pub fn backed_by_service(service: Arc<RecorderService>) -> Self {
        Self {
            requests: Arc::default(),
            service: Some(service),
        }
    }

    pub fn requests(&self) -> Arc<tokio::sync::Mutex<Vec<RecorderRequestV1>>> {
        Arc::clone(&self.requests)
    }

    async fn exchange(&self, request: RecorderRequestV1) -> anyhow::Result<RecorderReplyV1> {
        let request_id = request.request_id().to_owned();
        self.requests.lock().await.push(request.clone());
        match &self.service {
            Some(service) => Ok(service.handle(request).await),
            None => Ok(RecorderReplyV1::NotFound { request_id }),
        }
    }
}

#[cfg(feature = "test-support")]
impl RecorderClient {
    pub fn with_test_transport(mut self, transport: TestRecorderTransport) -> Self {
        self.test_transport = Some(transport);
        self
    }
}

#[derive(Debug)]
pub struct RecorderServer {
    endpoint: RecorderEndpoint,
    expected_peer_identity_digest: String,
    service: Arc<RecorderService>,
}

pub fn current_peer_identity_digest() -> anyhow::Result<String> {
    platform::current_peer_identity_digest()
}

impl RecorderServer {
    pub fn new(
        endpoint: RecorderEndpoint,
        expected_peer_identity_digest: String,
        service: Arc<RecorderService>,
    ) -> Self {
        Self {
            endpoint,
            expected_peer_identity_digest,
            service,
        }
    }

    pub async fn serve(self) -> anyhow::Result<()> {
        platform::serve(
            self.endpoint,
            self.expected_peer_identity_digest,
            self.service,
        )
        .await
    }
}

async fn exchange_stream<S>(
    stream: &mut S,
    request: &RecorderRequestV1,
    expected_identity: &RecorderIdentity,
) -> anyhow::Result<RecorderReplyV1>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let challenge = RecorderHandshakeChallengeV1 {
        handshake_version: RECORDER_HANDSHAKE_VERSION.into(),
        binding: request.binding().clone(),
        client_nonce: request.client_nonce().to_owned(),
        request_authentication_digest: format!(
            "sha256:{}",
            crate::hex_encode(&Sha256::digest(request.authentication_bytes()?))
        ),
    };
    write_message(stream, &challenge).await?;
    let attestation: RecorderHandshakeAttestationV1 = read_message(stream).await?;
    verify_handshake(&challenge, &attestation, expected_identity)?;
    write_message(stream, request).await?;
    let reply: RecorderReplyV1 = read_message(stream).await?;
    verify_reply_binding(request, &reply)?;
    if let RecorderReplyV1::Observation { observation, .. } = &reply {
        verify_observation(observation, expected_identity)?;
    }
    Ok(reply)
}

fn verify_reply_binding(
    request: &RecorderRequestV1,
    reply: &RecorderReplyV1,
) -> anyhow::Result<()> {
    let reply_request_id = match reply {
        RecorderReplyV1::Observation { request_id, .. }
        | RecorderReplyV1::NotFound { request_id }
        | RecorderReplyV1::Rejected { request_id, .. } => request_id,
    };
    if reply_request_id != request.request_id() {
        anyhow::bail!("recorder reply request ID does not match the call");
    }
    if let RecorderReplyV1::Observation { observation, .. } = reply {
        if observation.installation_id != request.binding().installation_id
            || observation.state_root_generation != request.binding().state_root_generation
            || observation.canonical_root_identity != request.binding().canonical_root_identity
            || observation.os_user_identity_digest != request.binding().os_user_identity_digest
        {
            anyhow::bail!("recorder observation binding does not match the call");
        }
        let (attempt_id, expected_digest) = match request {
            RecorderRequestV1::ExecuteOnce(command) => (
                command.attempt_id.as_str(),
                Some(crate::Journal::command_digest(command)?),
            ),
            RecorderRequestV1::QueryOnly(query) => (
                query.attempt_id.as_str(),
                query.expected_command_digest.clone(),
            ),
            RecorderRequestV1::Reconcile(reconcile) => (
                reconcile.attempt_id.as_str(),
                Some(reconcile.expected_command_digest.clone()),
            ),
        };
        if observation.attempt_id != attempt_id
            || expected_digest
                .as_ref()
                .is_some_and(|expected| observation.command_digest != *expected)
        {
            anyhow::bail!("recorder observation attempt does not match the call");
        }
        if let RecorderRequestV1::Reconcile(reconcile) = request {
            if observation.reconciliation_key_digest.as_deref()
                != Some(reconcile.reconciliation_key_digest.as_str())
            {
                anyhow::bail!("recorder observation reconciliation identity does not match");
            }
        }
    }
    Ok(())
}

async fn handle_stream<S>(mut stream: S, service: Arc<RecorderService>) -> anyhow::Result<()>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let challenge: RecorderHandshakeChallengeV1 = read_message(&mut stream).await?;
    let mut nonce = [0u8; 32];
    getrandom::fill(&mut nonce)?;
    let attestation = service
        .handshake(challenge.clone(), crate::hex_encode(&nonce))
        .await?;
    write_message(&mut stream, &attestation).await?;
    let request: RecorderRequestV1 = read_message(&mut stream).await?;
    verify_request_transcript(&challenge, &attestation, &request)?;
    let reply = service.handle(request).await;
    write_message(&mut stream, &reply).await?;
    stream.shutdown().await?;
    Ok(())
}

fn verify_request_transcript(
    challenge: &RecorderHandshakeChallengeV1,
    attestation: &RecorderHandshakeAttestationV1,
    request: &RecorderRequestV1,
) -> anyhow::Result<()> {
    let request_digest = format!(
        "sha256:{}",
        crate::hex_encode(&Sha256::digest(request.authentication_bytes()?))
    );
    if request.binding() != &attestation.binding
        || request.client_nonce() != attestation.client_nonce
        || request_digest != challenge.request_authentication_digest
        || request_digest != attestation.request_authentication_digest
    {
        anyhow::bail!("recorder request does not match its signed handshake transcript");
    }
    Ok(())
}

fn verify_observation(
    observation: &carsinos_protocol::execass_recorder::SignedRecorderObservationV1,
    expected: &RecorderIdentity,
) -> anyhow::Result<()> {
    if observation.recorder_key_id != expected.key_id
        || observation.recorder_key_generation != expected.key_generation
    {
        anyhow::bail!("recorder observation key does not match the pinned identity");
    }
    let key = VerifyingKey::from_bytes(&crate::hex_decode::<32>(&expected.verifying_key_hex)?)?;
    let signature = Signature::from_bytes(&crate::hex_decode::<64>(&observation.signature_hex)?);
    key.verify_strict(
        &recorder_observation_signing_bytes(observation)?,
        &signature,
    )
    .map_err(|_| anyhow::anyhow!("recorder observation signature is invalid"))
}

fn verify_handshake(
    challenge: &RecorderHandshakeChallengeV1,
    attestation: &RecorderHandshakeAttestationV1,
    expected: &RecorderIdentity,
) -> anyhow::Result<()> {
    if attestation.handshake_version != RECORDER_HANDSHAKE_VERSION
        || attestation.binding != challenge.binding
        || attestation.client_nonce != challenge.client_nonce
        || attestation.request_authentication_digest != challenge.request_authentication_digest
        || attestation.server_nonce.is_empty()
        || attestation.recorder_key_id != expected.key_id
        || attestation.recorder_key_generation != expected.key_generation
        || attestation.recorder_verifying_key_hex != expected.verifying_key_hex
    {
        anyhow::bail!("recorder handshake does not match the pinned transcript");
    }
    let key = VerifyingKey::from_bytes(&crate::hex_decode::<32>(
        &attestation.recorder_verifying_key_hex,
    )?)?;
    let signature = Signature::from_bytes(&crate::hex_decode::<64>(&attestation.signature_hex)?);
    key.verify_strict(&attestation.signing_bytes()?, &signature)
        .map_err(|_| anyhow::anyhow!("recorder handshake signature is invalid"))
}

async fn write_message<S, T>(stream: &mut S, message: &T) -> anyhow::Result<()>
where
    S: AsyncWrite + Unpin,
    T: serde::Serialize,
{
    let frame = encode_frame(message)?;
    stream.write_all(&frame).await?;
    stream.flush().await?;
    Ok(())
}

async fn read_message<S, T>(stream: &mut S) -> anyhow::Result<T>
where
    S: AsyncRead + Unpin,
    T: serde::de::DeserializeOwned,
{
    let mut prefix = [0u8; 4];
    stream.read_exact(&mut prefix).await?;
    let length = u32::from_be_bytes(prefix) as usize;
    if length > RECORDER_MAX_FRAME_BYTES {
        anyhow::bail!("recorder frame exceeds the fixed limit");
    }
    let mut payload = vec![0u8; length];
    stream.read_exact(&mut payload).await?;
    let mut frame = Vec::with_capacity(4 + length);
    frame.extend_from_slice(&prefix);
    frame.extend_from_slice(&payload);
    Ok(decode_frame(&frame)?)
}

#[cfg(windows)]
mod platform {
    pub(super) use super::windows::{client_exchange, current_peer_identity_digest, serve};
}

#[cfg(test)]
mod tests {
    use super::*;
    use carsinos_protocol::execass_recorder::{RecorderBindingV1, RECORDER_PROTOCOL_VERSION};
    use ed25519_dalek::{Signer, SigningKey};
    #[cfg(feature = "test-support")]
    use std::fs;

    fn signed_transcript() -> (
        RecorderHandshakeChallengeV1,
        RecorderHandshakeAttestationV1,
        RecorderIdentity,
    ) {
        let signing = SigningKey::from_bytes(&[0x42; 32]);
        let verifying = signing.verifying_key().to_bytes();
        let identity = RecorderIdentity {
            key_id: "key-1".into(),
            key_generation: 1,
            verifying_key_hex: crate::hex_encode(&verifying),
            verifying_key_digest: crate::hex_encode(&Sha256::digest(verifying)),
        };
        let binding = RecorderBindingV1 {
            protocol_version: RECORDER_PROTOCOL_VERSION.into(),
            canonical_root_identity: "root-1".into(),
            installation_id: "installation-1".into(),
            state_root_generation: 1,
            os_user_identity_digest: "user-1".into(),
            runtime_host_generation: 1,
            runtime_host_instance_id: "host-1".into(),
            runtime_fencing_token: 1,
        };
        let challenge = RecorderHandshakeChallengeV1 {
            handshake_version: RECORDER_HANDSHAKE_VERSION.into(),
            binding: binding.clone(),
            client_nonce: "client-nonce-1".into(),
            request_authentication_digest: format!("sha256:{}", "a".repeat(64)),
        };
        let mut attestation = RecorderHandshakeAttestationV1 {
            handshake_version: RECORDER_HANDSHAKE_VERSION.into(),
            binding,
            client_nonce: challenge.client_nonce.clone(),
            request_authentication_digest: challenge.request_authentication_digest.clone(),
            server_nonce: "server-nonce-1".into(),
            recorder_key_id: identity.key_id.clone(),
            recorder_key_generation: identity.key_generation,
            recorder_verifying_key_hex: identity.verifying_key_hex.clone(),
            signature_hex: String::new(),
        };
        attestation.signature_hex = crate::hex_encode(
            &signing
                .sign(&attestation.signing_bytes().unwrap())
                .to_bytes(),
        );
        (challenge, attestation, identity)
    }

    #[test]
    fn handshake_rejects_wrong_key_replayed_nonce_root_and_fence() {
        let (challenge, attestation, identity) = signed_transcript();
        verify_handshake(&challenge, &attestation, &identity).unwrap();

        let mut wrong_key = identity.clone();
        wrong_key.key_id = "other-key".into();
        assert!(verify_handshake(&challenge, &attestation, &wrong_key).is_err());

        let mut replayed_nonce = challenge.clone();
        replayed_nonce.client_nonce = "old-client-nonce".into();
        assert!(verify_handshake(&replayed_nonce, &attestation, &identity).is_err());

        let mut wrong_root = challenge.clone();
        wrong_root.binding.canonical_root_identity = "other-root".into();
        assert!(verify_handshake(&wrong_root, &attestation, &identity).is_err());

        let mut wrong_fence = challenge;
        wrong_fence.binding.runtime_fencing_token += 1;
        assert!(verify_handshake(&wrong_fence, &attestation, &identity).is_err());
    }

    #[test]
    fn client_debug_redacts_channel_key() {
        let (_, _, identity) = signed_transcript();
        let client = RecorderClient::new(
            RecorderEndpoint::for_binding(Path::new("."), "installation-1", 1),
            [0x7a; 32],
            identity,
        );
        let debug = format!("{client:?}");
        assert!(debug.contains("[REDACTED]"));
        assert!(!debug.contains("122, 122"));
    }

    #[test]
    fn handshake_transcript_rejects_a_swapped_request() {
        let (mut challenge, mut attestation, _) = signed_transcript();
        let request = RecorderRequestV1::QueryOnly(Box::new(
            carsinos_protocol::execass_recorder::QueryOnlyV1 {
                binding: challenge.binding.clone(),
                request_id: "request-1".into(),
                attempt_id: "attempt-1".into(),
                expected_command_digest: None,
                known_journal_head: None,
                client_nonce: challenge.client_nonce.clone(),
                command_mac: String::new(),
            },
        ));
        let digest = format!(
            "sha256:{}",
            crate::hex_encode(&Sha256::digest(request.authentication_bytes().unwrap()))
        );
        challenge.request_authentication_digest = digest.clone();
        attestation.request_authentication_digest = digest;
        verify_request_transcript(&challenge, &attestation, &request).unwrap();

        let mut swapped = request;
        if let RecorderRequestV1::QueryOnly(query) = &mut swapped {
            query.attempt_id = "attempt-2".into();
        }
        assert!(verify_request_transcript(&challenge, &attestation, &swapped).is_err());
    }

    #[cfg(feature = "test-support")]
    #[tokio::test]
    async fn service_backed_test_transport_records_and_calls_real_service() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join(format!(".service-transport-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir(&root).unwrap();
        let root_identity = "sha256:service-transport-root";
        let fixture = crate::TestRecorderFixture::for_root(root_identity);
        let journal = fixture.open_journal(&root).unwrap();
        let service = fixture.production_service(
            crate::ReadOnlyBeganVerifier::new(root.join("unused.db")),
            journal,
        );
        let transport = TestRecorderTransport::backed_by_service(service);
        let requests = transport.requests();
        let client = fixture
            .client_with_channel_key(
                RecorderEndpoint::for_binding(&root, "installation-1", 1),
                [0x55; 32],
            )
            .with_test_transport(transport);
        let request = RecorderRequestV1::QueryOnly(Box::new(QueryOnlyV1 {
            binding: RecorderBindingV1 {
                protocol_version: RECORDER_PROTOCOL_VERSION.into(),
                canonical_root_identity: root_identity.into(),
                installation_id: "installation-1".into(),
                state_root_generation: 1,
                os_user_identity_digest: "user-1".into(),
                runtime_host_generation: 1,
                runtime_host_instance_id: "host-1".into(),
                runtime_fencing_token: 1,
            },
            request_id: "service-transport-request".into(),
            attempt_id: "attempt-1".into(),
            expected_command_digest: None,
            known_journal_head: None,
            client_nonce: "service-transport-nonce".into(),
            command_mac: String::new(),
        }));
        let reply = client.send(request).await.unwrap();
        assert!(matches!(
            reply,
            RecorderReplyV1::Rejected { code, .. } if code == "authentication_failed"
        ));
        assert_eq!(requests.lock().await.len(), 1);
        drop(client);
        let _ = fs::remove_dir_all(&root);
    }
}

#[cfg(unix)]
mod platform {
    pub(super) use super::unix::{client_exchange, current_peer_identity_digest, serve};
}
