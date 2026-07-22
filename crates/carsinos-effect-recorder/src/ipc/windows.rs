use super::{exchange_stream, handle_stream, RecorderEndpoint};
use crate::service::RecorderService;
use crate::RecorderIdentity;
use anyhow::{bail, Context, Result};
use carsinos_protocol::execass_recorder::{RecorderReplyV1, RecorderRequestV1};
use sha2::{Digest, Sha256};
use std::ffi::c_void;
use std::os::windows::io::AsRawHandle;
use std::ptr;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::windows::named_pipe::{ClientOptions, NamedPipeServer, PipeMode, ServerOptions};
use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, LocalFree, HANDLE};
use windows_sys::Win32::Security::Authorization::{
    ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
};
use windows_sys::Win32::Security::{
    GetLengthSid, GetTokenInformation, IsValidSid, TokenUser, SECURITY_ATTRIBUTES, TOKEN_QUERY,
    TOKEN_USER,
};
use windows_sys::Win32::System::Pipes::GetNamedPipeClientProcessId;
use windows_sys::Win32::System::Threading::{
    GetCurrentProcess, OpenProcess, OpenProcessToken, PROCESS_QUERY_LIMITED_INFORMATION,
};

pub(super) async fn client_exchange(
    endpoint: &RecorderEndpoint,
    request: &RecorderRequestV1,
    expected_identity: &RecorderIdentity,
) -> Result<RecorderReplyV1> {
    let RecorderEndpoint::WindowsPipe(name) = endpoint;
    let mut last = None;
    for _ in 0..100 {
        match ClientOptions::new().open(name) {
            Ok(mut client) => {
                return exchange_stream(&mut client, request, expected_identity).await
            }
            Err(error) => {
                last = Some(error);
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        }
    }
    Err(last
        .map(anyhow::Error::from)
        .unwrap_or_else(|| anyhow::anyhow!("named-pipe client failed")))
}

pub(super) async fn serve(
    endpoint: RecorderEndpoint,
    expected_peer_identity_digest: String,
    service: Arc<RecorderService>,
) -> Result<()> {
    let RecorderEndpoint::WindowsPipe(name) = endpoint;
    let security = PipeSecurity::owner_and_system()?;
    let mut first = true;
    loop {
        let mut options = ServerOptions::new();
        options
            .pipe_mode(PipeMode::Byte)
            .reject_remote_clients(true)
            .first_pipe_instance(first);
        let server = unsafe {
            options.create_with_security_attributes_raw(
                &name,
                (&security.attributes as *const SECURITY_ATTRIBUTES)
                    .cast_mut()
                    .cast::<c_void>(),
            )
        }
        .context("creating owner-only recorder named pipe")?;
        first = false;
        server.connect().await?;
        verify_pipe_peer(&server, &expected_peer_identity_digest)?;
        let service = Arc::clone(&service);
        tokio::spawn(async move {
            if let Err(error) = handle_stream(server, service).await {
                tracing::warn!(error = %error, "recorder named-pipe request failed closed");
            }
        });
    }
}

struct PipeSecurity {
    descriptor: *mut c_void,
    attributes: SECURITY_ATTRIBUTES,
}

unsafe impl Send for PipeSecurity {}
unsafe impl Sync for PipeSecurity {}

impl PipeSecurity {
    fn owner_and_system() -> Result<Self> {
        let sid = current_process_sid()?;
        let sid_string = sid_to_string(&sid)?;
        let sddl = format!("D:P(A;;GA;;;SY)(A;;GA;;;{sid_string})");
        let wide = sddl.encode_utf16().chain(Some(0)).collect::<Vec<_>>();
        let mut descriptor = ptr::null_mut();
        let converted = unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                wide.as_ptr(),
                SDDL_REVISION_1,
                &mut descriptor,
                ptr::null_mut(),
            )
        };
        if converted == 0 || descriptor.is_null() {
            bail!("creating recorder named-pipe DACL failed: {}", unsafe {
                GetLastError()
            });
        }
        Ok(Self {
            descriptor,
            attributes: SECURITY_ATTRIBUTES {
                nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
                lpSecurityDescriptor: descriptor,
                bInheritHandle: 0,
            },
        })
    }
}

impl Drop for PipeSecurity {
    fn drop(&mut self) {
        unsafe {
            let _ = LocalFree(self.descriptor);
        }
    }
}

fn verify_pipe_peer(server: &NamedPipeServer, expected_digest: &str) -> Result<()> {
    let mut process_id = 0u32;
    let handle = server.as_raw_handle() as HANDLE;
    if unsafe { GetNamedPipeClientProcessId(handle, &mut process_id) } == 0 || process_id == 0 {
        bail!("recorder named-pipe client PID is unavailable");
    }
    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, process_id) };
    if process.is_null() {
        bail!("opening recorder named-pipe client process failed");
    }
    let _process_guard = OwnedHandle(process);
    let sid = process_sid(process)?;
    let digest = sid_digest(&sid);
    if digest != expected_digest {
        bail!("recorder named-pipe peer SID does not match the pinned OS user");
    }
    Ok(())
}

fn current_process_sid() -> Result<Vec<u8>> {
    process_sid(unsafe { GetCurrentProcess() })
}

fn process_sid(process: HANDLE) -> Result<Vec<u8>> {
    let mut token: HANDLE = ptr::null_mut();
    if unsafe { OpenProcessToken(process, TOKEN_QUERY, &mut token) } == 0 {
        bail!("opening process token failed: {}", unsafe {
            GetLastError()
        });
    }
    let _token_guard = OwnedHandle(token);
    let mut needed = 0u32;
    unsafe {
        let _ = GetTokenInformation(token, TokenUser, ptr::null_mut(), 0, &mut needed);
    }
    if needed < std::mem::size_of::<TOKEN_USER>() as u32 {
        bail!("process token SID is unavailable");
    }
    let mut buffer = vec![0u8; needed as usize];
    if unsafe {
        GetTokenInformation(
            token,
            TokenUser,
            buffer.as_mut_ptr().cast(),
            needed,
            &mut needed,
        )
    } == 0
    {
        bail!("reading process token SID failed: {}", unsafe {
            GetLastError()
        });
    }
    let token_user = buffer.as_ptr().cast::<TOKEN_USER>();
    let sid = unsafe { (*token_user).User.Sid };
    if sid.is_null() || unsafe { IsValidSid(sid) } == 0 {
        bail!("process token SID is invalid");
    }
    let length = unsafe { GetLengthSid(sid) } as usize;
    if length == 0 {
        bail!("process token SID is empty");
    }
    let sid_start = sid as usize;
    let buffer_start = buffer.as_ptr() as usize;
    let buffer_end = buffer_start + buffer.len();
    if sid_start < buffer_start || sid_start + length > buffer_end {
        bail!("process token SID is outside its token buffer");
    }
    Ok(unsafe { std::slice::from_raw_parts(sid.cast::<u8>(), length) }.to_vec())
}

fn sid_to_string(sid: &[u8]) -> Result<String> {
    if sid.is_empty() || unsafe { IsValidSid(sid.as_ptr().cast_mut().cast()) } == 0 {
        bail!("owner SID is invalid");
    }
    let mut pointer = ptr::null_mut();
    if unsafe { ConvertSidToStringSidW(sid.as_ptr().cast_mut().cast(), &mut pointer) } == 0
        || pointer.is_null()
    {
        bail!("converting owner SID to SDDL failed");
    }
    let mut length = 0usize;
    unsafe {
        while *pointer.add(length) != 0 {
            length += 1;
        }
    }
    let value = String::from_utf16(unsafe { std::slice::from_raw_parts(pointer, length) })
        .context("owner SID SDDL is invalid UTF-16")?;
    unsafe {
        let _ = LocalFree(pointer.cast());
    }
    Ok(value)
}

fn sid_digest(sid: &[u8]) -> String {
    let mut digest = Sha256::new();
    digest.update(b"carsinos.execass.windows-token-sid.v1");
    digest.update([0]);
    digest.update(sid);
    crate::hex_encode(&digest.finalize())
}

pub(super) fn current_peer_identity_digest() -> Result<String> {
    Ok(sid_digest(&current_process_sid()?))
}

struct OwnedHandle(HANDLE);

impl Drop for OwnedHandle {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe {
                let _ = CloseHandle(self.0);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_owner_dacl_and_sid_digest_are_available() {
        let sid = current_process_sid().unwrap();
        assert!(sid_to_string(&sid).unwrap().starts_with("S-1-"));
        assert_eq!(sid_digest(&sid).len(), 64);
        let security = PipeSecurity::owner_and_system().unwrap();
        assert!(!security.descriptor.is_null());
    }
}
