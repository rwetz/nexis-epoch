//! Windows: no console window, and one job object per job.
//!
//! The job object is created with `KILL_ON_JOB_CLOSE`, so the child and
//! everything it starts (data-loader workers, a shell it spawned) end when
//! the job is killed, when the job finishes and its handle is closed, and
//! when Epoch itself exits, even if it crashes: the OS closes the handle.
//!
//! The child is assigned right after it is spawned. A grandchild started
//! in the few milliseconds before that would escape; interpreters take far
//! longer than that to reach user code.
#![allow(unsafe_code)]

use std::io;
use std::os::windows::io::AsRawHandle;
use std::os::windows::process::CommandExt;
use std::process::{Child, Command};

use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
use windows_sys::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
    SetInformationJobObject, TerminateJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
    JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
};

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Start without a console window (Epoch is a GUI app; a console would
/// flash up for every job).
pub(crate) fn configure(cmd: &mut Command) {
    cmd.creation_flags(CREATE_NO_WINDOW);
}

/// A job object holding one child and its descendants. The handle is kept
/// as an integer so the type is `Send + Sync` without an unsafe impl.
pub(crate) struct ProcessGroup {
    job: isize,
}

impl ProcessGroup {
    pub(crate) fn attach(child: &Child) -> io::Result<Self> {
        // SAFETY: both arguments may be null (default security, unnamed).
        let job = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
        if job.is_null() {
            return Err(io::Error::last_os_error());
        }
        let group = Self { job: job as isize };
        // SAFETY: the struct is plain data; all-zero is a valid value.
        let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { std::mem::zeroed() };
        info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        // SAFETY: `job` is a live handle we own; the pointer and size
        // describe `info`, which outlives the call.
        let ok = unsafe {
            SetInformationJobObject(
                group.handle(),
                JobObjectExtendedLimitInformation,
                std::ptr::from_ref(&info).cast(),
                std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            )
        };
        if ok == 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: `job` is live; the process handle is owned by `child`,
        // which the caller keeps alive for the duration of the call.
        let ok = unsafe { AssignProcessToJobObject(group.handle(), child.as_raw_handle()) };
        if ok == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(group)
    }

    fn handle(&self) -> HANDLE {
        self.job as HANDLE
    }

    /// End every process in the job.
    pub(crate) fn kill(&self) {
        // SAFETY: the handle is live until `drop`.
        unsafe {
            TerminateJobObject(self.handle(), 1);
        }
    }

    /// Windows has no polite stop for a windowless process tree (there is
    /// no console to send Ctrl+Break to). Returns `false`: the caller kills.
    pub(crate) fn interrupt(&self) -> bool {
        false
    }
}

impl Drop for ProcessGroup {
    fn drop(&mut self) {
        // SAFETY: we own the handle and close it once. KILL_ON_JOB_CLOSE
        // ends whatever is still in the job.
        unsafe {
            CloseHandle(self.handle());
        }
    }
}
