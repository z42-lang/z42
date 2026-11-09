//! Native monitor storage owned by a managed object's payload.
//! Fields contain only thread identities, counters and synchronization primitives.
//! Checked operations, contention reporting and GC parking are supplied by corelib.

use parking_lot::{Condvar, Mutex};
use std::thread::ThreadId;

#[derive(Debug, Default)]
pub struct Monitor {
    pub(crate) state:     Mutex<MonitorState>,
    pub(crate) entry_cv:  Condvar,
    pub(crate) signal_cv: Condvar,
}

#[derive(Debug, Default)]
pub struct MonitorState {
    pub(crate) owner:          Option<ThreadId>,
    pub(crate) entry_waiters:  u32,
    pub(crate) signal_waiters: u32,
}
