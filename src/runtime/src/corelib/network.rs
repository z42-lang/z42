//! `Std.Net.Sockets` builtins — sync blocking TCP sockets.
//!
//! add-z42-net (K1, 2026-05-24): pattern mirrors `process.rs` (slot-id
//! handle + per-builtin slot lookup). All cross-platform differences
//! (BSD vs Winsock vs iOS/Android) are delegated to Rust
//! `std::net::{TcpStream, TcpListener}`.
//!
//! ## Return shape
//!
//! All builtins (except `*_drop`, which return `Value::Null`) return a
//! discriminated `Value::Array` tuple. The first element is always a
//! `KIND_*` tag so z42 decoding is uniform:
//!
//! ```text
//!   [I64(0), I64(slot)]                       // KIND_OK — connect / accept
//!   [I64(0), I64(slot), I64(actual_port)]     // KIND_OK — listen
//!   [I64(0), I64(nbytes)]                     // KIND_OK — read / write (0 = EOF)
//!   [I64(1), Str(message)]                    // KIND_SOCKET_ERR — io fail
//!   [I64(2)]                                  // KIND_HANDLE_INVALID — slot missing
//!   [I64(3)]                                  // KIND_UNSUPPORTED — wasm32
//! ```
//!
//! Drops always return `Value::Null` (idempotent; no error path needed).

use super::convert::{arg_i64, arg_str};
use crate::metadata::Value;
use crate::vm_context::VmContext;
use anyhow::{bail, Result};

pub(crate) const KIND_OK:              i64 = 0;
pub(crate) const KIND_SOCKET_ERR:      i64 = 1;
pub(crate) const KIND_HANDLE_INVALID:  i64 = 2;
#[cfg(target_arch = "wasm32")]
pub(crate) const KIND_UNSUPPORTED:     i64 = 3;

fn ok_value(ctx: &VmContext, v: i64) -> Value {
    ctx.heap().alloc_array(vec![Value::I64(KIND_OK), Value::I64(v)])
}

fn ok_two(ctx: &VmContext, a: i64, b: i64) -> Value {
    ctx.heap().alloc_array(vec![Value::I64(KIND_OK), Value::I64(a), Value::I64(b)])
}

/// add-z42-net-udp-multicast (2026-05-27): success tuple without payload
/// for void-shaped ops (multicast join/leave/set_loop).
fn ok_unit(ctx: &VmContext) -> Value {
    ctx.heap().alloc_array(vec![Value::I64(KIND_OK)])
}

fn socket_err(ctx: &VmContext, msg: String) -> Value {
    ctx.heap().alloc_array(vec![Value::I64(KIND_SOCKET_ERR), Value::Str(msg.into())])
}

fn handle_invalid(ctx: &VmContext) -> Value {
    ctx.heap().alloc_array(vec![Value::I64(KIND_HANDLE_INVALID)])
}

#[cfg(target_arch = "wasm32")]
fn unsupported(ctx: &VmContext) -> Value {
    ctx.heap().alloc_array(vec![Value::I64(KIND_UNSUPPORTED)])
}

fn require_slot_id(args: &[Value], idx: usize, name: &str) -> Result<u64> {
    let n = arg_i64(args, idx, name)?;
    if n < 0 {
        bail!("{}: slot id must be non-negative, got {}", name, n);
    }
    Ok(n as u64)
}

fn require_port(args: &[Value], idx: usize, name: &str) -> Result<u16> {
    let p = arg_i64(args, idx, name)?;
    if !(0..=65535).contains(&p) {
        bail!("{}: port out of range [0, 65535]: {}", name, p);
    }
    Ok(p as u16)
}

// ── desktop / mobile (non-wasm32) implementations ─────────────────────────

// refactor-split-network（2026-09-03）：内联 `mod imp`（922 行）按 TCP / TCP 选项 / UDP·DNS 分到 `network/`，
// wasm32 桩在 `network/wasm.rs`；全量 `pub use` 使 `network::builtin_*` 路径不变。
#[cfg(not(target_arch = "wasm32"))] mod tcp;
#[cfg(not(target_arch = "wasm32"))] mod tcp_options;
#[cfg(not(target_arch = "wasm32"))] mod udp;
#[cfg(not(target_arch = "wasm32"))] pub use tcp::*;
#[cfg(not(target_arch = "wasm32"))] pub use tcp_options::*;
#[cfg(not(target_arch = "wasm32"))] pub use udp::*;
#[cfg(target_arch = "wasm32")] mod wasm;
#[cfg(target_arch = "wasm32")] pub use wasm::*;


#[cfg(test)]
#[path = "network_tests.rs"]
mod network_tests;
