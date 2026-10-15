//! The two places the runtime needs the OS directly: putting a child in a
//! group that can be ended as a whole, and asking that group to stop.
//!
//! Only these modules may use `unsafe` (see Cargo.toml's lints).

#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub(crate) use self::windows::{configure, ProcessGroup};

#[cfg(unix)]
mod unix;
#[cfg(unix)]
pub(crate) use self::unix::{configure, ProcessGroup};
