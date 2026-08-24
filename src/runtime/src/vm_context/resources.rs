use super::*;

impl VmContext {
    // ── Process slot table (add-std-process, 2026-05-13) ──────────────────

    /// Allocate a new slot id and store `slot` under it. Returns the id
    /// for the z42 `ProcessHandle` to carry. Counter is monotonic and
    /// never reused; u64 overflow is not a practical concern (10^19
    /// spawns).
    pub fn alloc_process_slot(&self, slot: crate::corelib::process::ProcessSlot) -> u64 {
        // M2: id counter is now embedded in the registry (was
        // `VmContext::process_next_id`). Relaxed ordering is fine — slot IDs
        // only need to be unique within the VM and the table lock orders
        // observations.
        self.core.processes.insert_new(slot)
    }

    /// Remove and return the slot. Used by `wait` / `kill`+reap / `drop`
    /// which take ownership of `child` etc.
    pub fn take_process_slot(&self, slot_id: u64) -> Option<crate::corelib::process::ProcessSlot> {
        self.core.processes.take(slot_id)
    }

    /// Peek at the slot in-place. Returns `None` if the slot id is
    /// unknown. Callback runs while the table lock is held — callers must
    /// not invoke other slot methods inside.
    pub fn with_process_slot<T>(
        &self,
        slot_id: u64,
        f: impl FnOnce(&mut crate::corelib::process::ProcessSlot) -> T,
    ) -> Option<T> {
        self.core.processes.with_mut(slot_id, f)
    }

    /// Number of currently allocated process slots. Used by tests to
    /// verify cleanup paths drop entries.
    pub fn process_slot_count(&self) -> usize {
        self.core.processes.count()
    }

    // ── add-z42-net K1 (2026-05-24): TCP socket / listener slot helpers ──
    //
    // Pattern mirrors `alloc_process_slot` exactly. The registry (counter +
    // table) lives in `core` (shared across threads) so a server thread
    // accepting connections + a worker thread reading from them can't collide
    // on slot ids.

    #[cfg(not(target_arch = "wasm32"))]
    pub fn alloc_tcp_socket_slot(&self, stream: std::net::TcpStream) -> u64 {
        self.core.tcp_sockets.insert_new(stream)
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn alloc_tcp_listener_slot(&self, listener: std::net::TcpListener) -> u64 {
        self.core.tcp_listeners.insert_new(listener)
    }

    /// add-z42-net-tls (2026-06-03): register a connected + handshaken rustls
    /// stream and return its slot id. `__net_tls_drop` removes (closing the fd).
    #[cfg(not(target_arch = "wasm32"))]
    pub fn alloc_tls_socket_slot(
        &self,
        stream: rustls::StreamOwned<rustls::ClientConnection, std::net::TcpStream>,
    ) -> u64 {
        self.core.tls_sockets.insert_new(stream)
    }

    /// Number of currently allocated TLS socket slots. Used by tests to
    /// verify cleanup paths drop entries.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn tls_socket_slot_count(&self) -> usize {
        self.core.tls_sockets.count()
    }

    /// Number of currently allocated TCP socket slots. Used by tests to
    /// verify cleanup paths drop entries.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn tcp_socket_slot_count(&self) -> usize {
        self.core.tcp_sockets.count()
    }

    /// Number of currently allocated TCP listener slots.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn tcp_listener_slot_count(&self) -> usize {
        self.core.tcp_listeners.count()
    }

    // ── add-z42-net-udp (K2, 2026-05-25) ────────────────────────────────

    #[cfg(not(target_arch = "wasm32"))]
    pub fn alloc_udp_socket_slot(&self, sock: std::net::UdpSocket) -> u64 {
        self.core.udp_sockets.insert_new(sock)
    }

    /// Number of currently allocated UDP socket slots.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn udp_socket_slot_count(&self) -> usize {
        self.core.udp_sockets.count()
    }
}
