//! Tuple-bound operating-system ownership for the one ExecAss runtime host.

use anyhow::{bail, Context, Result};
use sha2::{Digest, Sha256};
use std::path::Path;

const LOCK_DOMAIN: &[u8] = b"carsinos.execass.runtime-host-ownership.v1";

/// Held for the complete lifetime of the gateway runtime host. The private
/// platform guard is the authority; the public strings are diagnostics only.
pub(crate) struct RuntimeHostOwnership {
    pub(crate) lock_path: String,
    pub(crate) owner: String,
    #[cfg(windows)]
    _guard: WindowsMutexGuard,
    #[cfg(unix)]
    _guard: std::fs::File,
}

impl std::fmt::Debug for RuntimeHostOwnership {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RuntimeHostOwnership")
            .field("lock_path", &self.lock_path)
            .field("owner", &self.owner)
            .finish_non_exhaustive()
    }
}

pub(crate) fn acquire(
    _state_root: &Path,
    canonical_root_identity: &str,
    installation_identity: &str,
    os_user_identity_digest: &str,
) -> Result<RuntimeHostOwnership> {
    validate_scope(
        canonical_root_identity,
        installation_identity,
        os_user_identity_digest,
    )?;
    let lock_id = ownership_lock_id(
        canonical_root_identity,
        installation_identity,
        os_user_identity_digest,
    );
    let owner = owner_label();

    #[cfg(windows)]
    {
        let name = format!("Local\\CarsinOS.ExecAss.RuntimeHost.{lock_id}");
        let guard = WindowsMutexGuard::acquire(&name)?;
        Ok(RuntimeHostOwnership {
            lock_path: format!("windows-mutex://{name}"),
            owner,
            _guard: guard,
        })
    }

    #[cfg(unix)]
    {
        use fs2::FileExt;
        use std::io::ErrorKind;

        let lock_dir = _state_root.join("locks");
        std::fs::create_dir_all(&lock_dir).with_context(|| {
            format!(
                "failed to create runtime-host lock directory {}",
                lock_dir.display()
            )
        })?;
        let path = lock_dir.join(format!("runtime-host-{lock_id}.lock"));
        let file = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(&path)
            .with_context(|| format!("failed to open runtime-host lock {}", path.display()))?;
        match file.try_lock_exclusive() {
            Ok(()) => Ok(RuntimeHostOwnership {
                lock_path: path.display().to_string(),
                owner,
                _guard: file,
            }),
            Err(error) if error.kind() == ErrorKind::WouldBlock => {
                bail!("the tuple-bound ExecAss runtime host is already owned")
            }
            Err(error) => Err(error)
                .with_context(|| format!("failed to acquire runtime-host lock {}", path.display())),
        }
    }

    #[cfg(not(any(windows, unix)))]
    {
        let _ = (_state_root, lock_id, owner);
        bail!("this platform has no supported runtime-host ownership primitive")
    }
}

fn validate_scope(
    canonical_root_identity: &str,
    installation_identity: &str,
    os_user_identity_digest: &str,
) -> Result<()> {
    if canonical_root_identity.len() != 71
        || !canonical_root_identity.starts_with("sha256:")
        || !canonical_root_identity[7..]
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        bail!("runtime-host canonical root identity is malformed");
    }
    if installation_identity.trim().is_empty() {
        bail!("runtime-host installation identity is empty");
    }
    if os_user_identity_digest.len() != 64
        || !os_user_identity_digest
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        bail!("runtime-host OS user identity digest is malformed");
    }
    Ok(())
}

fn ownership_lock_id(
    canonical_root_identity: &str,
    installation_identity: &str,
    os_user_identity_digest: &str,
) -> String {
    let mut digest = Sha256::new();
    digest.update(LOCK_DOMAIN);
    digest.update([0]);
    digest.update(os_user_identity_digest.as_bytes());
    digest.update([0]);
    digest.update(canonical_root_identity.as_bytes());
    digest.update([0]);
    digest.update(installation_identity.as_bytes());
    format!("{:x}", digest.finalize())
}

fn owner_label() -> String {
    let hostname = std::env::var("HOSTNAME")
        .ok()
        .or_else(|| std::env::var("COMPUTERNAME").ok())
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "unknown-host".to_string());
    format!("pid:{}@{}", std::process::id(), hostname)
}

#[cfg(windows)]
struct WindowsMutexGuard(windows_sys::Win32::Foundation::HANDLE);

#[cfg(windows)]
unsafe impl Send for WindowsMutexGuard {}

#[cfg(windows)]
unsafe impl Sync for WindowsMutexGuard {}

#[cfg(windows)]
impl WindowsMutexGuard {
    fn acquire(name: &str) -> Result<Self> {
        use std::ffi::c_void;
        use std::ptr;
        use windows_sys::Win32::Foundation::{GetLastError, ERROR_ALREADY_EXISTS};
        use windows_sys::Win32::Security::Authorization::{
            ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW,
            SDDL_REVISION_1,
        };
        use windows_sys::Win32::Security::{
            GetLengthSid, GetTokenInformation, IsValidSid, TokenUser, SECURITY_ATTRIBUTES,
            TOKEN_QUERY, TOKEN_USER,
        };
        use windows_sys::Win32::System::Threading::{
            CreateMutexW, GetCurrentProcess, OpenProcessToken,
        };

        let mut token = ptr::null_mut();
        if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) } == 0 {
            bail!("opening the runtime-host owner token failed");
        }
        let token = OwnedHandle(token);
        let mut needed = 0u32;
        unsafe {
            let _ = GetTokenInformation(token.0, TokenUser, ptr::null_mut(), 0, &mut needed);
        }
        if needed < std::mem::size_of::<TOKEN_USER>() as u32 {
            bail!("runtime-host owner SID is unavailable");
        }
        let mut token_buffer = vec![0u8; needed as usize];
        if unsafe {
            GetTokenInformation(
                token.0,
                TokenUser,
                token_buffer.as_mut_ptr().cast(),
                needed,
                &mut needed,
            )
        } == 0
        {
            bail!("reading the runtime-host owner SID failed");
        }
        let token_user = token_buffer.as_ptr().cast::<TOKEN_USER>();
        let sid = unsafe { (*token_user).User.Sid };
        if sid.is_null() || unsafe { IsValidSid(sid) } == 0 || unsafe { GetLengthSid(sid) } == 0 {
            bail!("runtime-host owner SID is invalid");
        }
        let mut sid_string = ptr::null_mut();
        if unsafe { ConvertSidToStringSidW(sid, &mut sid_string) } == 0 || sid_string.is_null() {
            bail!("converting the runtime-host owner SID failed");
        }
        let sid_string = LocalAllocation(sid_string.cast());
        let mut sid_len = 0usize;
        unsafe {
            while *sid_string.0.cast::<u16>().add(sid_len) != 0 {
                sid_len += 1;
            }
        }
        let sid_text = String::from_utf16(unsafe {
            std::slice::from_raw_parts(sid_string.0.cast::<u16>(), sid_len)
        })
        .context("runtime-host owner SID is invalid UTF-16")?;
        let sddl = format!("D:P(A;;GA;;;SY)(A;;GA;;;{sid_text})");
        let sddl_wide = sddl.encode_utf16().chain(Some(0)).collect::<Vec<_>>();
        let mut descriptor = ptr::null_mut();
        if unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                sddl_wide.as_ptr(),
                SDDL_REVISION_1,
                &mut descriptor,
                ptr::null_mut(),
            )
        } == 0
            || descriptor.is_null()
        {
            bail!("creating the owner-only runtime-host mutex DACL failed");
        }
        let descriptor = LocalAllocation(descriptor);
        let attributes = SECURITY_ATTRIBUTES {
            nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: descriptor.0,
            bInheritHandle: 0,
        };
        let wide_name = name.encode_utf16().chain(Some(0)).collect::<Vec<_>>();
        let handle = unsafe { CreateMutexW(&attributes, 1, wide_name.as_ptr()) };
        if handle.is_null() {
            bail!("creating the tuple-bound runtime-host mutex failed");
        }
        let error = unsafe { GetLastError() };
        if error == ERROR_ALREADY_EXISTS {
            unsafe {
                windows_sys::Win32::Foundation::CloseHandle(handle);
            }
            bail!("the tuple-bound ExecAss runtime host is already owned");
        }
        let _ = descriptor;
        let _ = sid_string;
        let _ = token;
        let _ = std::marker::PhantomData::<c_void>;
        Ok(Self(handle))
    }
}

#[cfg(windows)]
impl Drop for WindowsMutexGuard {
    fn drop(&mut self) {
        unsafe {
            let _ = windows_sys::Win32::System::Threading::ReleaseMutex(self.0);
            let _ = windows_sys::Win32::Foundation::CloseHandle(self.0);
        }
    }
}

#[cfg(windows)]
struct OwnedHandle(windows_sys::Win32::Foundation::HANDLE);

#[cfg(windows)]
impl Drop for OwnedHandle {
    fn drop(&mut self) {
        unsafe {
            let _ = windows_sys::Win32::Foundation::CloseHandle(self.0);
        }
    }
}

#[cfg(windows)]
struct LocalAllocation(*mut std::ffi::c_void);

#[cfg(windows)]
impl Drop for LocalAllocation {
    fn drop(&mut self) {
        unsafe {
            let _ = windows_sys::Win32::Foundation::LocalFree(self.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root() -> String {
        format!("sha256:{}", "a".repeat(64))
    }

    #[test]
    fn ownership_id_is_tuple_bound_and_stable() {
        let base = ownership_lock_id(&root(), "install-a", &"b".repeat(64));
        assert_eq!(
            base,
            ownership_lock_id(&root(), "install-a", &"b".repeat(64))
        );
        assert_ne!(
            base,
            ownership_lock_id(&root(), "install-b", &"b".repeat(64))
        );
        assert_ne!(
            base,
            ownership_lock_id(&root(), "install-a", &"c".repeat(64))
        );
        assert_ne!(
            base,
            ownership_lock_id(
                &format!("sha256:{}", "d".repeat(64)),
                "install-a",
                &"b".repeat(64)
            )
        );
    }

    #[test]
    fn malformed_scope_is_rejected() {
        assert!(validate_scope("wrong", "install", &"b".repeat(64)).is_err());
        assert!(validate_scope(&root(), "", &"b".repeat(64)).is_err());
        assert!(validate_scope(&root(), "install", "wrong").is_err());
    }

    #[test]
    fn one_tuple_has_exactly_one_live_os_owner_and_can_be_reacquired_after_release() {
        let state_root = tempfile::tempdir().unwrap();
        let first = acquire(state_root.path(), &root(), "install-a", &"b".repeat(64)).unwrap();
        let collision = acquire(state_root.path(), &root(), "install-a", &"b".repeat(64));
        assert!(collision.unwrap_err().to_string().contains("already owned"));
        drop(first);
        acquire(state_root.path(), &root(), "install-a", &"b".repeat(64)).unwrap();
    }
}
