//! `Std.Net.Sockets` builtins — sync blocking TCP / UDP sockets and DNS, all
//! cross-platform differences (BSD / Winsock / mobile) delegated to `std::net` + `socket2`.
//!
//! | file | what |
//! |---|---|
//! | `net/tcp.rs` / `net/udp.rs` | builtin ABI v2 (`__tcp_*` / `__udp_*` / `__dns_lookup`): plain results, failures thrown from Rust, blocking calls parked |
//! | `net/error.rs` | the .NET-shaped exception mapping (`SocketException` + `SocketErrorCode`, closed handle, unsupported platform) |
//! | `net/listener.rs` | listener slot + the interruptible accept wait (used by the ABI v2 accept) |
//! | `net/unsupported.rs` | stubs of the ABI v2 builtins for builds without the `net` / `tls` feature (e.g. wasm32): throw `NetUnsupportedException` |
//!
//! TLS (`__tls_*`) is in `corelib/tls/mod.rs`; it shares `net/error.rs`.

pub(crate) mod error;

#[cfg(feature = "net")]
pub(crate) mod listener;
#[cfg(feature = "net")]
pub(crate) mod tcp;
#[cfg(feature = "net")]
pub(crate) mod udp;
#[cfg(not(feature = "tls"))]
pub(crate) mod unsupported;

#[cfg(all(test, feature = "net"))]
pub(crate) mod network_tests;
