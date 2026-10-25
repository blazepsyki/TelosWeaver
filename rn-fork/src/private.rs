//! Only this OS user reaches the engine's pipes and its RPC token (TelosWeaver, Spec v1.5 §12.1, PH0-RN-06): on
//! Windows a DACL that allows the current user's SID alone, on Unix mode 0600. The same rule as TelosWeaver Core's
//! Local API (`crates/core/src/api/security.rs`), which this follows; when it can't be applied, the caller opens
//! nothing (fail closed). Other processes of the same user are not kept out (SAFE-01).

use std::{io, path::Path};

/// Makes the file at `path` this user's alone.
pub fn protect(path: &Path) -> io::Result<()> {
    #[cfg(unix)]
    return crate::util::chmod(path, 0o600);
    #[cfg(windows)]
    return windows::protect(path);
}

#[cfg(windows)]
pub use windows::pipe;

#[cfg(windows)]
mod windows {
    use std::{
        ffi::c_void,
        io,
        os::windows::{
            ffi::OsStrExt,
            fs::MetadataExt,
            io::{AsRawHandle, FromRawHandle, OwnedHandle},
        },
        path::Path,
        ptr,
    };
    use tokio::net::windows::named_pipe::{NamedPipeServer, ServerOptions};
    use windows_sys::Win32::{
        Foundation::LocalFree,
        Security::{
            Authorization::{
                ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW, GetNamedSecurityInfoW,
                SDDL_REVISION_1, SE_FILE_OBJECT,
            },
            *,
        },
        System::Threading::{GetCurrentProcess, OpenProcessToken},
    };

    fn wide(value: &std::ffi::OsStr) -> Vec<u16> {
        value.encode_wide().chain(Some(0)).collect()
    }

    /// # Safety
    /// `sid` is a valid SID.
    pub(super) unsafe fn sid_string(sid: PSID) -> io::Result<String> {
        let mut text = ptr::null_mut();
        if unsafe { ConvertSidToStringSidW(sid, &mut text) } == 0 {
            return Err(io::Error::last_os_error());
        }
        let mut len = 0;
        while unsafe { *text.add(len) } != 0 {
            len += 1;
        }
        let s = String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(text, len) });
        unsafe { LocalFree(text.cast()) };
        Ok(s)
    }

    pub(super) fn current_sid() -> io::Result<String> {
        let mut raw = ptr::null_mut();
        if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut raw) } == 0 {
            return Err(io::Error::last_os_error());
        }
        let token = unsafe { OwnedHandle::from_raw_handle(raw) };
        let mut size = 0;
        unsafe { GetTokenInformation(token.as_raw_handle(), TokenUser, ptr::null_mut(), 0, &mut size) };
        if size == 0 {
            return Err(io::Error::last_os_error());
        }
        let mut buf = vec![0_usize; (size as usize).div_ceil(size_of::<usize>())];
        if unsafe { GetTokenInformation(token.as_raw_handle(), TokenUser, buf.as_mut_ptr().cast(), size, &mut size) }
            == 0
        {
            return Err(io::Error::last_os_error());
        }
        unsafe { sid_string((*(buf.as_ptr().cast::<TOKEN_USER>())).User.Sid) }
    }

    /// `D:P(A;;GA;;;<this user>)`: a protected DACL with one entry.
    struct Descriptor(PSECURITY_DESCRIPTOR);

    impl Drop for Descriptor {
        fn drop(&mut self) {
            unsafe { LocalFree(self.0) };
        }
    }

    impl Descriptor {
        fn user_only() -> io::Result<Self> {
            let sddl = wide(std::ffi::OsStr::new(&format!("D:P(A;;GA;;;{})", current_sid()?)));
            let mut d = ptr::null_mut();
            if unsafe {
                ConvertStringSecurityDescriptorToSecurityDescriptorW(
                    sddl.as_ptr(),
                    SDDL_REVISION_1,
                    &mut d,
                    ptr::null_mut(),
                )
            } == 0
            {
                return Err(io::Error::last_os_error());
            }
            Ok(Self(d))
        }
    }

    pub(super) fn protect(path: &Path) -> io::Result<()> {
        if std::fs::symlink_metadata(path)?.file_attributes() & 0x400 != 0 {
            return Err(io::Error::other("a reparse point can't hold the engine's private files"));
        }
        let name = wide(path.as_os_str());
        let (mut owner, mut sd) = (ptr::null_mut(), ptr::null_mut());
        let r = unsafe {
            GetNamedSecurityInfoW(
                name.as_ptr(),
                SE_FILE_OBJECT,
                OWNER_SECURITY_INFORMATION,
                &mut owner,
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null_mut(),
                &mut sd,
            )
        };
        if r != 0 {
            return Err(io::Error::from_raw_os_error(r as i32));
        }
        let owner_id = unsafe { sid_string(owner) };
        unsafe { LocalFree(sd) };
        if owner_id? != current_sid()? {
            return Err(io::Error::other("the file belongs to another user"));
        }
        let d = Descriptor::user_only()?;
        if unsafe {
            SetFileSecurityW(name.as_ptr(), DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION, d.0)
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }

    /// An instance of the pipe `name` only this user can open (`first`: none may exist yet).
    pub fn pipe(name: &Path, first: bool) -> io::Result<NamedPipeServer> {
        let d = Descriptor::user_only()?;
        let mut sa = SECURITY_ATTRIBUTES {
            nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: d.0,
            bInheritHandle: 0,
        };
        let mut options = ServerOptions::new();
        options.first_pipe_instance(first).reject_remote_clients(true);
        // SAFETY: `sa` and the descriptor it points to outlive the call.
        unsafe {
            options.create_with_security_attributes_raw(name, (&mut sa as *mut SECURITY_ATTRIBUTES).cast::<c_void>())
        }
    }

    /// The DACL of a kernel object or file `handle`: its entries as (allowed, SID).
    #[cfg(test)]
    pub(super) fn entries(handle: windows_sys::Win32::Foundation::HANDLE, file: bool) -> Vec<(bool, String)> {
        use windows_sys::Win32::Security::Authorization::{GetSecurityInfo, SE_KERNEL_OBJECT};
        let (mut dacl, mut sd) = (ptr::null_mut(), ptr::null_mut());
        let r = unsafe {
            GetSecurityInfo(
                handle,
                if file { SE_FILE_OBJECT } else { SE_KERNEL_OBJECT },
                DACL_SECURITY_INFORMATION,
                ptr::null_mut(),
                ptr::null_mut(),
                &mut dacl,
                ptr::null_mut(),
                &mut sd,
            )
        };
        assert_eq!(r, 0);
        let mut out = Vec::new();
        for i in 0..unsafe { (*dacl).AceCount } as u32 {
            let mut ace = ptr::null_mut();
            assert_ne!(unsafe { GetAce(dacl, i, &mut ace) }, 0);
            let ace = ace.cast::<ACCESS_ALLOWED_ACE>();
            let sid = unsafe { ptr::addr_of_mut!((*ace).SidStart).cast() };
            out.push((unsafe { (*ace).Header.AceType } == 0, unsafe { sid_string(sid) }.unwrap()));
        }
        unsafe { LocalFree(sd) };
        out
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[tokio::test]
        async fn a_pipe_and_a_protected_file_allow_this_user_alone() {
            let name = format!(r"\\.\pipe\telos-rn-acl-test-{}", ulid::Ulid::generate());
            let server = pipe(Path::new(&name), true).unwrap();
            assert_eq!(entries(server.as_raw_handle(), false), [(true, current_sid().unwrap())]);
            // Its name is taken: no second first instance.
            assert!(pipe(Path::new(&name), true).is_err());
            let file = std::env::temp_dir().join(format!("telos-rn-acl-test-{}", ulid::Ulid::generate()));
            std::fs::write(&file, "x").unwrap();
            super::super::protect(&file).unwrap();
            let f = std::fs::File::open(&file).unwrap();
            assert_eq!(entries(f.as_raw_handle(), true), [(true, current_sid().unwrap())]);
            drop(f);
            let _ = std::fs::remove_file(file);
        }
    }
}
