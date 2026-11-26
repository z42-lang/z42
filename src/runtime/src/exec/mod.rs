//! Shared instruction execution semantics.
//!
//! Consumers name the execution contract, not its implementation files:
//! ```
//! use z42::exec::{Engine, Initialization, TierBackend, invoke_mode};
//! let _ = invoke_mode;
//! ```
//! ```compile_fail
//! use z42::exec::engine::Engine;
//! ```
//! ```compile_fail
//! use z42::exec::init::Initialization;
//! ```
//! ```compile_fail
//! fn jit_slot(ctx: &z42::vm::VmContext) {
//!     let _ = &ctx.exec().pending_exception;
//! }
//! ```
//! ```compile_fail
//! fn callback_slot(ctx: &z42::vm::VmContext) {
//!     let _ = &ctx.exec().pending_thrown;
//! }
//! ```
//! The activation vocabulary is reached through the exec root re-exports:
//! ```
//! use z42::exec::frames::VmFrame;
//! let _ = std::mem::size_of::<*const VmFrame>();
//! ```
//! ```compile_fail
//! use z42::exec::call::frames::VmFrame;
//! ```
//! ```
//! use z42::{exec::{current_tier, TierApi}, vm::VmContext};
//! fn active_tier(ctx: &VmContext) -> Option<&TierApi> { current_tier(ctx) }
//! ```
//! ```compile_fail
//! use z42::exec::EngineRegistry;
//! ```
//! ```compile_fail
//! fn engine_state(ctx: &z42::vm::VmContext) {
//!     let _ = ctx.core().exec();
//! }
//! ```
//! ```compile_fail
//! use z42::{exec::frames::VmFrame, vm::VmContext};
//! fn push(stack_owner: &VmContext, active_vm: &VmContext, frame: VmFrame) {
//!     stack_owner.exec().push_frame(active_vm, frame);
//! }
//! ```
//! ```compile_fail
//! use z42::exec::frame_stack::FrameStack;
//! ```
//! ```compile_fail
//! fn raw_frames(ctx: &z42::vm::VmContext) {
//!     let _ = &ctx.exec().call_stack;
//! }
//! ```
//! ```compile_fail
//! use z42::exec::transient_arena::TransientArena;
//! ```
//! ```compile_fail
//! fn raw_transient_values(ctx: &z42::vm::VmContext) {
//!     let _ = &ctx.exec().transient_arena;
//! }
//! ```
//! ```compile_fail
//! use z42::exec::stack_alloc::StackArena;
//! ```
//! ```compile_fail
//! fn raw_stack_allocations(ctx: &z42::vm::VmContext) {
//!     let _ = &ctx.exec().stack_arena;
//! }
//! ```
//! ```compile_fail
//! use z42::exec::struct_arena::StructArena;
//! ```
//! ```compile_fail
//! fn raw_struct_allocations(ctx: &z42::vm::VmContext) {
//!     let _ = &ctx.exec().struct_arena;
//! }
//! ```
//! ```compile_fail
//! use z42::exec::reg_pool::RegPool;
//! ```
//! ```compile_fail
//! fn raw_register_pool(ctx: &z42::vm::VmContext) {
//!     let _ = &ctx.exec().reg_pool;
//! }
//! ```
pub mod exception;
pub mod symbol_resolution;
pub mod stack_guard;
pub mod ops;
#[cfg(feature = "native-interop")]
pub mod native;
#[cfg(feature = "native-interop")]
pub mod active_vm;
/// Engine-shared activation and call machinery. The stable vocabulary is
/// re-exported at the exec root (frames / stack allocation policy / arguments /
/// outcome / forwarding / builtin ABI and dispatch). The frame stack, register
/// pool, arenas and activation guards are private to execution.
pub(crate) mod call;
pub(crate) use call::{invoke_entry, EntryError};
pub use call::{
    args, builtin_abi, builtins, forward, frames, indirect_return,
    outcome, stack_alloc, type_param_env,
};
pub mod stack;
mod engine;
pub(crate) use engine::EngineRegistry;
pub use engine::{Engine, JitConfig, OsrState, TierApi, TierBackend, TierTarget,
    invoke, invoke_args, invoke_mode};

mod init;
pub use init::{Initialization, make_type_init_exception};
#[cfg(test)]
pub(crate) use init::{claim as claim_initialization, claim_with as claim_initialization_with};

mod state;
pub(crate) use state::ExecState;

mod thread_state;
pub use thread_state::{ThreadExecState, current_tier, push_frame, refresh_tier};
