//! Unix: each job leads its own process group, so a stop reaches the
//! child and everything it started.
#![allow(unsafe_code)]

use std::io;
use std::os::unix::process::CommandExt;
use std::process::{Child, Command};

pub(crate) fn configure(cmd: &mut Command) {
    // A new group whose id is the child's pid.
    cmd.process_group(0);
}

pub(crate) struct ProcessGroup {
    pgid: libc::pid_t,
}

impl ProcessGroup {
    pub(crate) fn attach(child: &Child) -> io::Result<Self> {
        let pgid = libc::pid_t::try_from(child.id())
            .map_err(|_| io::Error::other("process id out of range"))?;
        Ok(Self { pgid })
    }

    fn signal(&self, signal: libc::c_int) -> bool {
        // SAFETY: killpg has no memory-safety preconditions; a stale group
        // id fails with ESRCH.
        unsafe { libc::killpg(self.pgid, signal) == 0 }
    }

    pub(crate) fn kill(&self) {
        self.signal(libc::SIGKILL);
    }

    /// SIGTERM to the group. Returns whether it was delivered.
    pub(crate) fn interrupt(&self) -> bool {
        self.signal(libc::SIGTERM)
    }
}
