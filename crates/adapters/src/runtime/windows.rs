//! Atomic Windows Job containment (Spec §7.3, SAFE-03).

use super::{CommandSpec, ExitStatus, Spawned, StdinMode, TerminationCause};
use std::{
    collections::BTreeMap,
    ffi::{OsStr, OsString, c_void},
    fs::{File, OpenOptions},
    io,
    mem::{size_of, zeroed},
    os::windows::{
        ffi::OsStrExt,
        io::{AsRawHandle, FromRawHandle, OwnedHandle},
    },
    ptr,
    sync::Arc,
};
use windows_sys::Win32::{
    Foundation::*,
    Security::SECURITY_ATTRIBUTES,
    System::{JobObjects::*, Pipes::CreatePipe, Threading::*},
};

pub struct Control {
    process: OwnedHandle,
    job: OwnedHandle,
}

fn raw(handle: &OwnedHandle) -> HANDLE {
    handle.as_raw_handle()
}
fn check(ok: i32) -> io::Result<()> {
    if ok == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}
fn wide(value: &OsStr) -> io::Result<Vec<u16>> {
    let mut value: Vec<_> = value.encode_wide().collect();
    if value.contains(&0) {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "embedded NUL"));
    }
    value.push(0);
    Ok(value)
}

fn pipe(child_reads: bool) -> io::Result<(File, File)> {
    unsafe {
        let sa = SECURITY_ATTRIBUTES {
            nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: ptr::null_mut(),
            bInheritHandle: 1,
        };
        let (mut read, mut write) = (ptr::null_mut(), ptr::null_mut());
        check(CreatePipe(&mut read, &mut write, &sa, 0))?;
        let read = File::from_raw_handle(read);
        let write = File::from_raw_handle(write);
        let parent = if child_reads {
            write.as_raw_handle()
        } else {
            read.as_raw_handle()
        };
        check(SetHandleInformation(parent, HANDLE_FLAG_INHERIT, 0))?;
        Ok((read, write))
    }
}

fn append_quoted(output: &mut Vec<u16>, argument: &OsStr) -> io::Result<()> {
    let value = wide(argument)?;
    output.push(b'"' as u16);
    let mut slashes = 0;
    for ch in &value[..value.len() - 1] {
        if *ch == b'\\' as u16 {
            slashes += 1;
            continue;
        }
        output.extend(std::iter::repeat_n(
            b'\\' as u16,
            if *ch == b'"' as u16 {
                slashes * 2 + 1
            } else {
                slashes
            },
        ));
        output.push(*ch);
        slashes = 0;
    }
    output.extend(std::iter::repeat_n(b'\\' as u16, slashes * 2));
    output.push(b'"' as u16);
    Ok(())
}

struct Attributes {
    storage: Vec<usize>,
    initialized: bool,
}
impl Attributes {
    fn new(job: &HANDLE, handles: &[HANDLE]) -> io::Result<Self> {
        unsafe {
            let mut bytes = 0;
            InitializeProcThreadAttributeList(ptr::null_mut(), 2, 0, &mut bytes);
            if bytes == 0 {
                return Err(io::Error::last_os_error());
            }
            let mut this = Self {
                storage: vec![0; bytes.div_ceil(size_of::<usize>())],
                initialized: false,
            };
            check(InitializeProcThreadAttributeList(
                this.pointer(),
                2,
                0,
                &mut bytes,
            ))?;
            this.initialized = true;
            check(UpdateProcThreadAttribute(
                this.pointer(),
                0,
                PROC_THREAD_ATTRIBUTE_JOB_LIST as usize,
                job as *const _ as *const c_void,
                size_of::<HANDLE>(),
                ptr::null_mut(),
                ptr::null(),
            ))?;
            check(UpdateProcThreadAttribute(
                this.pointer(),
                0,
                PROC_THREAD_ATTRIBUTE_HANDLE_LIST as usize,
                handles.as_ptr().cast(),
                size_of_val(handles),
                ptr::null_mut(),
                ptr::null(),
            ))?;
            Ok(this)
        }
    }
    fn pointer(&mut self) -> LPPROC_THREAD_ATTRIBUTE_LIST {
        self.storage.as_mut_ptr().cast()
    }
}
impl Drop for Attributes {
    fn drop(&mut self) {
        if self.initialized {
            unsafe {
                DeleteProcThreadAttributeList(self.pointer());
            }
        }
    }
}

pub fn spawn(spec: CommandSpec) -> io::Result<Spawned> {
    let program = spec.program.canonicalize()?;
    let extension = program.extension().and_then(OsStr::to_str).unwrap_or("");
    if !["exe", "com"]
        .iter()
        .any(|ext| extension.eq_ignore_ascii_case(ext))
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "native Windows executable required; shell shims are not executed",
        ));
    }
    let cwd = spec.cwd.canonicalize()?;
    if !cwd.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "cwd is not a directory",
        ));
    }
    let program_wide = wide(program.as_os_str())?;
    let cwd_wide = wide(cwd.as_os_str())?;
    let mut command = Vec::new();
    append_quoted(&mut command, program.as_os_str())?;
    for arg in &spec.args {
        command.push(b' ' as u16);
        append_quoted(&mut command, arg)?;
    }
    command.push(0);
    let mut environment: BTreeMap<String, (OsString, OsString)> = std::env::vars_os()
        .map(|(key, value)| (key.to_string_lossy().to_uppercase(), (key, value)))
        .collect();
    for (key, value) in spec.env {
        let normalized = key.to_string_lossy().to_uppercase();
        if let Some(value) = value {
            environment.insert(normalized, (key, value));
        } else {
            environment.remove(&normalized);
        }
    }
    let mut env_block = Vec::new();
    for (_, (key, value)) in environment {
        let key = wide(&key)?;
        if key.len() == 1
            || (key[..key.len() - 1].contains(&(b'=' as u16)) && key[0] != b'=' as u16)
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid environment key",
            ));
        }
        env_block.extend_from_slice(&key[..key.len() - 1]);
        env_block.push(b'=' as u16);
        env_block.extend(wide(&value)?);
    }
    if env_block.is_empty() {
        env_block.push(0);
    }
    env_block.push(0);
    let (child_in, stdin) = if spec.stdin == StdinMode::Piped {
        let (read, write) = pipe(true)?;
        (read, Some(write))
    } else {
        let null = OpenOptions::new().read(true).open("NUL")?;
        unsafe {
            check(SetHandleInformation(
                null.as_raw_handle(),
                HANDLE_FLAG_INHERIT,
                HANDLE_FLAG_INHERIT,
            ))?;
        }
        (null, None)
    };
    let (stdout, child_out) = pipe(false)?;
    let (stderr, child_err) = pipe(false)?;
    unsafe {
        let job = CreateJobObjectW(ptr::null(), ptr::null());
        if job.is_null() {
            return Err(io::Error::last_os_error());
        }
        let job = OwnedHandle::from_raw_handle(job);
        let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = zeroed();
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        check(SetInformationJobObject(
            raw(&job),
            JobObjectExtendedLimitInformation,
            &limits as *const _ as *const c_void,
            size_of_val(&limits) as u32,
        ))?;
        let job_raw = raw(&job);
        let child_handles = [
            child_in.as_raw_handle(),
            child_out.as_raw_handle(),
            child_err.as_raw_handle(),
        ];
        let mut attributes = Attributes::new(&job_raw, &child_handles)?;
        let mut startup: STARTUPINFOEXW = zeroed();
        startup.StartupInfo.cb = size_of::<STARTUPINFOEXW>() as u32;
        startup.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
        startup.StartupInfo.hStdInput = child_handles[0];
        startup.StartupInfo.hStdOutput = child_handles[1];
        startup.StartupInfo.hStdError = child_handles[2];
        startup.lpAttributeList = attributes.pointer();
        let mut info: PROCESS_INFORMATION = zeroed();
        // JOB_LIST assigns containment atomically, before any child instruction executes.
        check(CreateProcessW(
            program_wide.as_ptr(),
            command.as_mut_ptr(),
            ptr::null(),
            ptr::null(),
            1,
            CREATE_NO_WINDOW | CREATE_UNICODE_ENVIRONMENT | EXTENDED_STARTUPINFO_PRESENT,
            env_block.as_ptr().cast(),
            cwd_wide.as_ptr(),
            &startup.StartupInfo,
            &mut info,
        ))?;
        let process = OwnedHandle::from_raw_handle(info.hProcess);
        drop(OwnedHandle::from_raw_handle(info.hThread));
        Ok(Spawned {
            control: Arc::new(Control { process, job }),
            stdin,
            stdout,
            stderr,
            pid: info.dwProcessId,
        })
    }
}

impl Control {
    pub fn try_wait(&self) -> io::Result<Option<i32>> {
        unsafe {
            match WaitForSingleObject(raw(&self.process), 0) {
                WAIT_TIMEOUT => Ok(None),
                WAIT_OBJECT_0 => {
                    let mut code = 0;
                    check(GetExitCodeProcess(raw(&self.process), &mut code))?;
                    Ok(Some(code as i32))
                }
                _ => Err(io::Error::last_os_error()),
            }
        }
    }
    pub fn wait(&self) -> io::Result<ExitStatus> {
        unsafe {
            if WaitForSingleObject(raw(&self.process), INFINITE) != WAIT_OBJECT_0 {
                return Err(io::Error::last_os_error());
            }
        }
        self.try_wait()?
            .map(|code| ExitStatus {
                code: Some(code),
                cause: TerminationCause::Exited,
            })
            .ok_or_else(|| io::Error::other("missing process exit status"))
    }
    pub fn request_stop(&self) -> io::Result<()> {
        // Spec §7.3: no extra stop signal is sent here. NativeHandle closes stdin
        // when possible, waits for the grace period, then terminates the Job.
        Ok(())
    }
    pub fn kill_tree(&self) -> io::Result<()> {
        unsafe { check(TerminateJobObject(raw(&self.job), 1)) }
    }
    pub fn tree_empty(&self) -> io::Result<bool> {
        unsafe {
            let mut accounting: JOBOBJECT_BASIC_ACCOUNTING_INFORMATION = zeroed();
            check(QueryInformationJobObject(
                raw(&self.job),
                JobObjectBasicAccountingInformation,
                &mut accounting as *mut _ as *mut c_void,
                size_of_val(&accounting) as u32,
                ptr::null_mut(),
            ))?;
            Ok(accounting.ActiveProcesses == 0)
        }
    }
}
