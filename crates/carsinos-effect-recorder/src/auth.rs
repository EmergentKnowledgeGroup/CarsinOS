use carsinos_protocol::execass_recorder::RecorderRequestV1;
use hmac::{Hmac, Mac};
use sha2::Sha256;
use zeroize::Zeroizing;

type HmacSha256 = Hmac<Sha256>;

pub fn sign_request(request: &mut RecorderRequestV1, channel_key: &[u8; 32]) -> anyhow::Result<()> {
    request.set_command_mac(String::new());
    let bytes = Zeroizing::new(request.authentication_bytes()?);
    let mut mac = HmacSha256::new_from_slice(channel_key)?;
    mac.update(&bytes);
    request.set_command_mac(crate::hex_encode(&mac.finalize().into_bytes()));
    Ok(())
}

pub fn authenticate_request(
    request: &RecorderRequestV1,
    channel_key: &[u8; 32],
) -> anyhow::Result<()> {
    request.validate()?;
    let supplied = crate::hex_decode::<32>(request.command_mac())?;
    let bytes = Zeroizing::new(request.authentication_bytes()?);
    let mut mac = HmacSha256::new_from_slice(channel_key)?;
    mac.update(&bytes);
    mac.verify_slice(&supplied)
        .map_err(|_| anyhow::anyhow!("recorder request MAC is invalid"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use carsinos_protocol::execass_recorder::*;

    fn query() -> RecorderRequestV1 {
        RecorderRequestV1::QueryOnly(Box::new(QueryOnlyV1 {
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
            attempt_id: "attempt".into(),
            expected_command_digest: None,
            known_journal_head: None,
            client_nonce: "nonce".into(),
            command_mac: String::new(),
        }))
    }

    #[test]
    fn wrong_mac_and_mutation_are_rejected() {
        let key = [7u8; 32];
        let mut request = query();
        sign_request(&mut request, &key).unwrap();
        authenticate_request(&request, &key).unwrap();
        assert!(authenticate_request(&request, &[8u8; 32]).is_err());
        if let RecorderRequestV1::QueryOnly(value) = &mut request {
            value.attempt_id.push('x');
        }
        assert!(authenticate_request(&request, &key).is_err());
    }
}
