use crate::{
    client_exchange_stream, handle_stream, hex_encode, RuntimeControlEndpoint, RuntimeControlError,
    RuntimeControlReplyV1, RuntimeControlRequestV1, ServerState,
};
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

pub(crate) async fn client_exchange(
    endpoint: &RuntimeControlEndpoint,
    request: &RuntimeControlRequestV1,
) -> Result<RuntimeControlReplyV1, RuntimeControlError> {
    let mut last_error = None;
    for _ in 0..100 {
        match ClientOptions::new().open(&endpoint.pipe_name) {
            Ok(mut client) => return client_exchange_stream(&mut client, request).await,
            Err(error) => {
                last_error = Some(error);
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        }
    }
    let _ = last_error;
    Err(RuntimeControlError::Transport)
}

pub(crate) async fn serve(
    endpoint: RuntimeControlEndpoint,
    state: Arc<ServerState>,
) -> Result<(), RuntimeControlError> {
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
                &endpoint.pipe_name,
                (&security.attributes as *const SECURITY_ATTRIBUTES)
                    .cast_mut()
                    .cast::<c_void>(),
            )
        }
        .map_err(|_| RuntimeControlError::Transport)?;
        first = false;
        server
            .connect()
            .await
            .map_err(|_| RuntimeControlError::Transport)?;
        if verify_pipe_peer(&server, &state.scope.os_user_identity_digest).is_err() {
            continue;
        }
        let state = Arc::clone(&state);
        tokio::spawn(async move {
            let _ = handle_stream(server, state).await;
        });
    }
}

pub(crate) fn current_os_user_identity_digest() -> Result<String, RuntimeControlError> {
    Ok(sid_digest(&current_process_sid()?))
}

struct PipeSecurity {
    descriptor: *mut c_void,
    attributes: SECURITY_ATTRIBUTES,
}

unsafe impl Send for PipeSecurity {}
unsafe impl Sync for PipeSecurity {}

impl PipeSecurity {
    fn owner_and_system() -> Result<Self, RuntimeControlError> {
        let sid_string = sid_to_string(&current_process_sid()?)?;
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
            let _ = unsafe { GetLastError() };
            return Err(RuntimeControlError::Transport);
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

fn verify_pipe_peer(
    server: &NamedPipeServer,
    expected_digest: &str,
) -> Result<(), RuntimeControlError> {
    let mut process_id = 0u32;
    let handle = server.as_raw_handle() as HANDLE;
    if unsafe { GetNamedPipeClientProcessId(handle, &mut process_id) } == 0 || process_id == 0 {
        return Err(RuntimeControlError::Transport);
    }
    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, process_id) };
    if process.is_null() {
        return Err(RuntimeControlError::Transport);
    }
    let _process = OwnedHandle(process);
    if sid_digest(&process_sid(process)?) != expected_digest {
        return Err(RuntimeControlError::Authentication);
    }
    Ok(())
}

fn current_process_sid() -> Result<Vec<u8>, RuntimeControlError> {
    process_sid(unsafe { GetCurrentProcess() })
}

fn process_sid(process: HANDLE) -> Result<Vec<u8>, RuntimeControlError> {
    let mut token = ptr::null_mut();
    if unsafe { OpenProcessToken(process, TOKEN_QUERY, &mut token) } == 0 {
        return Err(RuntimeControlError::Transport);
    }
    let _token = OwnedHandle(token);
    let mut needed = 0u32;
    unsafe {
        let _ = GetTokenInformation(token, TokenUser, ptr::null_mut(), 0, &mut needed);
    }
    if needed < std::mem::size_of::<TOKEN_USER>() as u32 {
        return Err(RuntimeControlError::Transport);
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
        return Err(RuntimeControlError::Transport);
    }
    let sid = unsafe { (*buffer.as_ptr().cast::<TOKEN_USER>()).User.Sid };
    if sid.is_null() || unsafe { IsValidSid(sid) } == 0 {
        return Err(RuntimeControlError::Transport);
    }
    let length = unsafe { GetLengthSid(sid) } as usize;
    if length == 0 {
        return Err(RuntimeControlError::Transport);
    }
    let sid_start = sid as usize;
    let buffer_start = buffer.as_ptr() as usize;
    let buffer_end = buffer_start + buffer.len();
    if sid_start < buffer_start
        || sid_start
            .checked_add(length)
            .is_none_or(|end| end > buffer_end)
    {
        return Err(RuntimeControlError::Transport);
    }
    Ok(unsafe { std::slice::from_raw_parts(sid.cast::<u8>(), length) }.to_vec())
}

fn sid_to_string(sid: &[u8]) -> Result<String, RuntimeControlError> {
    if sid.is_empty() || unsafe { IsValidSid(sid.as_ptr().cast_mut().cast()) } == 0 {
        return Err(RuntimeControlError::Transport);
    }
    let mut pointer = ptr::null_mut();
    if unsafe { ConvertSidToStringSidW(sid.as_ptr().cast_mut().cast(), &mut pointer) } == 0
        || pointer.is_null()
    {
        return Err(RuntimeControlError::Transport);
    }
    let mut length = 0usize;
    unsafe {
        while *pointer.add(length) != 0 {
            length += 1;
        }
    }
    let value = String::from_utf16(unsafe { std::slice::from_raw_parts(pointer, length) })
        .map_err(|_| RuntimeControlError::Transport)?;
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
    hex_encode(&digest.finalize())
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
