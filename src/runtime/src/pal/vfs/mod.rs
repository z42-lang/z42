//! [`Vfs`](super::fs::Vfs) implementations: [`LocalFs`] (the OS file system) and
//! [`MemoryFs`] (an instance-owned `path → bytes` store — what wasm runs on, and what an
//! embedder mounts files into before creating a VM).

mod local;
mod memory;

pub use local::LocalFs;
pub use memory::MemoryFs;
