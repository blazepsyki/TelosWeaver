use std::{
    fs, io,
    path::{Path, PathBuf},
};

pub(super) struct Credentials {
    pub token_path: PathBuf,
    pub token: String,
}

pub(super) fn private_directory(path: &Path) -> io::Result<()> {
    if !path.exists() {
        let parent = path
            .parent()
            .ok_or_else(|| io::Error::other("missing state parent"))?;
        if !parent.exists() {
            private_directory(parent)?;
        }
        match fs::create_dir(path) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error),
        }
    }
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(io::Error::other("state directory must be a real directory"));
    }
    platform::protect(path, true)
}

impl Credentials {
    pub fn create(directory: &Path, project: &Path) -> io::Result<Self> {
        if !directory.is_absolute() {
            return Err(io::Error::other("token directory must be absolute"));
        }
        let mut existing = directory;
        while !existing.exists() {
            existing = existing
                .parent()
                .ok_or_else(|| io::Error::other("state parent is unavailable"))?;
        }
        if existing.canonicalize()?.starts_with(project) {
            return Err(io::Error::other(
                "token directory must be outside the project",
            ));
        }
        private_directory(directory)?;
        if directory.canonicalize()?.starts_with(project) {
            return Err(io::Error::other(
                "token directory must be outside the project",
            ));
        }
        let mut random = [0_u8; 32];
        getrandom::fill(&mut random)
            .map_err(|_| io::Error::other("secure token generation failed"))?;
        let token: String = random.iter().map(|byte| format!("{byte:02x}")).collect();
        let path = directory.join("api-token");
        if let Ok(metadata) = fs::symlink_metadata(&path) {
            if !metadata.is_file() || metadata.file_type().is_symlink() {
                return Err(io::Error::other("token path must be a regular file"));
            }
            platform::protect(&path, false)?;
        }
        let temporary = directory.join(format!("api-token-{}.tmp", &token[..16]));
        let result = (|| {
            use std::io::Write;
            let mut file = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temporary)?;
            platform::protect(&temporary, false)?;
            file.write_all(token.as_bytes())?;
            file.sync_all()?;
            drop(file);
            fs::rename(&temporary, &path)
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        result?;
        Ok(Self {
            token_path: path,
            token,
        })
    }

    pub fn authenticates(&self, candidate: &str) -> bool {
        // Tokens have a fixed public length. Compare every byte for equal-length input.
        if candidate.len() != self.token.len() {
            return false;
        }
        candidate
            .bytes()
            .zip(self.token.bytes())
            .fold(0_u8, |difference, (a, b)| difference | (a ^ b))
            == 0
    }
}

#[cfg(unix)]
mod platform {
    use super::*;
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    pub fn protect(path: &Path, directory: bool) -> io::Result<()> {
        let metadata = fs::symlink_metadata(path)?;
        if metadata.uid() != unsafe { libc::geteuid() } {
            return Err(io::Error::other("state entry has a different owner"));
        }
        fs::set_permissions(
            path,
            fs::Permissions::from_mode(if directory { 0o700 } else { 0o600 }),
        )
    }
}

#[cfg(windows)]
pub(super) mod platform {
    use super::*;
    use std::{
        ffi::c_void,
        os::windows::{
            ffi::OsStrExt,
            fs::MetadataExt,
            io::{AsRawHandle, FromRawHandle, OwnedHandle},
        },
        ptr,
    };
    use windows_sys::Win32::{
        Foundation::LocalFree,
        Security::{
            Authorization::{
                ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW,
                GetNamedSecurityInfoW, SDDL_REVISION_1, SE_FILE_OBJECT,
            },
            *,
        },
        System::Threading::{GetCurrentProcess, OpenProcessToken},
    };

    fn wide(value: &std::ffi::OsStr) -> Vec<u16> {
        value.encode_wide().chain(Some(0)).collect()
    }
    unsafe fn sid_string(sid: PSID) -> io::Result<String> {
        let mut text = ptr::null_mut();
        if unsafe { ConvertSidToStringSidW(sid, &mut text) } == 0 {
            return Err(io::Error::last_os_error());
        }
        let mut len = 0;
        unsafe {
            while *text.add(len) != 0 {
                len += 1;
            }
        }
        let result = String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(text, len) });
        unsafe {
            LocalFree(text.cast());
        }
        Ok(result)
    }
    fn current_sid() -> io::Result<String> {
        let mut raw = ptr::null_mut();
        if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut raw) } == 0 {
            return Err(io::Error::last_os_error());
        }
        let handle = unsafe { OwnedHandle::from_raw_handle(raw) };
        let mut size = 0;
        unsafe {
            GetTokenInformation(
                handle.as_raw_handle(),
                TokenUser,
                ptr::null_mut(),
                0,
                &mut size,
            );
        }
        if size == 0 {
            return Err(io::Error::last_os_error());
        }
        let mut buffer = vec![0_usize; (size as usize).div_ceil(size_of::<usize>())];
        if unsafe {
            GetTokenInformation(
                handle.as_raw_handle(),
                TokenUser,
                buffer.as_mut_ptr().cast(),
                size,
                &mut size,
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        unsafe { sid_string((*(buffer.as_ptr().cast::<TOKEN_USER>())).User.Sid) }
    }
    pub struct Descriptor(PSECURITY_DESCRIPTOR);
    impl Drop for Descriptor {
        fn drop(&mut self) {
            unsafe {
                LocalFree(self.0);
            }
        }
    }
    impl Descriptor {
        pub fn user_only(directory: bool) -> io::Result<Self> {
            let inheritance = if directory { "OICI" } else { "" };
            let sddl = wide(std::ffi::OsStr::new(&format!(
                "D:P(A;{inheritance};GA;;;{})",
                current_sid()?
            )));
            let mut descriptor = ptr::null_mut();
            if unsafe {
                ConvertStringSecurityDescriptorToSecurityDescriptorW(
                    sddl.as_ptr(),
                    SDDL_REVISION_1,
                    &mut descriptor,
                    ptr::null_mut(),
                )
            } == 0
            {
                return Err(io::Error::last_os_error());
            }
            Ok(Self(descriptor))
        }
        pub fn attributes(&self) -> SECURITY_ATTRIBUTES {
            SECURITY_ATTRIBUTES {
                nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
                lpSecurityDescriptor: self.0,
                bInheritHandle: 0,
            }
        }
    }
    pub fn protect(path: &Path, directory: bool) -> io::Result<()> {
        if fs::symlink_metadata(path)?.file_attributes() & 0x400 != 0 {
            return Err(io::Error::other(
                "reparse points are not allowed in private state",
            ));
        }
        let name = wide(path.as_os_str());
        let mut owner = ptr::null_mut();
        let mut security = ptr::null_mut();
        let result = unsafe {
            GetNamedSecurityInfoW(
                name.as_ptr(),
                SE_FILE_OBJECT,
                OWNER_SECURITY_INFORMATION,
                &mut owner,
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null_mut(),
                &mut security,
            )
        };
        if result != 0 {
            return Err(io::Error::from_raw_os_error(result as i32));
        }
        let owner_id = unsafe { sid_string(owner) };
        unsafe {
            LocalFree(security);
        }
        if owner_id? != current_sid()? {
            return Err(io::Error::other("state entry has a different owner"));
        }
        let descriptor = Descriptor::user_only(directory)?;
        if unsafe {
            SetFileSecurityW(
                name.as_ptr(),
                DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
                descriptor.0,
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }
    pub fn pipe(
        options: &tokio::net::windows::named_pipe::ServerOptions,
        name: &str,
    ) -> io::Result<tokio::net::windows::named_pipe::NamedPipeServer> {
        let descriptor = Descriptor::user_only(false)?;
        let mut attributes = descriptor.attributes();
        unsafe {
            options.create_with_security_attributes_raw(
                name,
                (&mut attributes as *mut SECURITY_ATTRIBUTES).cast::<c_void>(),
            )
        }
    }

    #[cfg(test)]
    pub(in crate::api) unsafe fn assert_user_only(handle: windows_sys::Win32::Foundation::HANDLE) {
        use windows_sys::Win32::Security::Authorization::{GetSecurityInfo, SE_KERNEL_OBJECT};
        let mut dacl = ptr::null_mut();
        let mut descriptor = ptr::null_mut();
        assert_eq!(
            unsafe {
                GetSecurityInfo(
                    handle,
                    SE_KERNEL_OBJECT,
                    DACL_SECURITY_INFORMATION,
                    ptr::null_mut(),
                    ptr::null_mut(),
                    &mut dacl,
                    ptr::null_mut(),
                    &mut descriptor,
                )
            },
            0
        );
        assert!(!dacl.is_null());
        assert_eq!(unsafe { (*dacl).AceCount }, 1);
        let mut ace = ptr::null_mut();
        assert_ne!(unsafe { GetAce(dacl, 0, &mut ace) }, 0);
        let ace = ace.cast::<ACCESS_ALLOWED_ACE>();
        assert_eq!(unsafe { (*ace).Header.AceType }, 0);
        let sid = unsafe { ptr::addr_of_mut!((*ace).SidStart).cast() };
        assert_eq!(unsafe { sid_string(sid) }.unwrap(), current_sid().unwrap());
        unsafe {
            LocalFree(descriptor);
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        #[test]
        fn token_acl_contains_only_current_user() {
            let root =
                std::env::temp_dir().join(crate::execution::new_id("telos-acl-test").unwrap());
            let project = root.join("project");
            fs::create_dir_all(&project).unwrap();
            let credentials =
                Credentials::create(&root.join("state"), &project.canonicalize().unwrap()).unwrap();
            let file = fs::File::open(&credentials.token_path).unwrap();
            unsafe {
                assert_user_only(file.as_raw_handle());
            }
            drop(file);
            fs::remove_dir_all(root).unwrap();
        }
    }
}
