//! `Std.Net.Sockets` builtins — sync blocking TCP / UDP sockets and DNS, all
//! cross-platform differences (BSD / Winsock / mobile) delegated to `std::net` + `socket2`.
//!
//! | file | what |
//! |---|---|
//! | `network/tcp.rs` / `network/udp.rs` | builtin ABI v2 (`__tcp_*` / `__udp_*` / `__dns_lookup`): plain results, failures thrown from Rust, blocking calls parked |
//! | `network/error.rs` | the .NET-shaped exception mapping (`SocketException` + `SocketErrorCode`, closed handle, unsupported platform) |
//! | `network/listener.rs` | listener slot + the interruptible accept wait (used by the ABI v2 accept) |
//! | `network/wasm.rs` | wasm32 stubs of the ABI v2 builtins (throw `NetUnsupportedException`) |
//!
//! TLS (`__tls_*`) is in `corelib/tls.rs`; it shares `network/error.rs`.

pub(crate) mod error;

#[cfg(not(target_arch = "wasm32"))]
pub(crate) mod listener;
#[cfg(not(target_arch = "wasm32"))]
pub(crate) use listener::ListenerSlot;
#[cfg(not(target_arch = "wasm32"))]
pub(crate) mod tcp;
#[cfg(not(target_arch = "wasm32"))]
pub(crate) mod udp;
#[cfg(target_arch = "wasm32")]
pub(crate) mod wasm;

#[cfg(all(test, not(target_arch = "wasm32")))]
pub(crate) mod network_tests;
