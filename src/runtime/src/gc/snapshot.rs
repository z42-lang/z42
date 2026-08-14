//! V8 `.heapsnapshot` graph export (add-gc-heap-snapshot-export, 2026-05-24).
//!
//! Walks the live heap via the existing [`MagrGC`] trait methods
//! ([`iterate_live_objects`](super::heap::MagrGC::iterate_live_objects) +
//! [`scan_object_refs`](super::heap::MagrGC::scan_object_refs) +
//! [`for_each_root`](super::heap::MagrGC::for_each_root)) and emits a
//! flat node/edge graph serializable to the canonical V8 heap-snapshot
//! JSON format. Chrome DevTools (Memory → Load), speedscope, and
//! heapviewer.com all open the resulting `.heapsnapshot` directly.
//!
//! **Out of scope** (deferred to follow-up specs):
//! - allocation-site stack traces (`trace_function_infos` / `trace_tree`
//!   / `samples` arrays are emitted empty; requires IR alloc-site IDs
//!   from B4 backlog spec)
//! - streaming JSON serializer for multi-GB heaps
//! - server-side dominator-tree pre-computation
//! - explicit weak edges (current weak refs are skipped to avoid
//!   retention confusion)
//!
//! See [`docs/design/runtime/gc.md`] § "Heap snapshot export" for the
//! design notes and node/edge type mapping table.

use std::collections::HashMap;
use std::io::{self, Write};

use super::heap::MagrGC;
use crate::metadata::Value;

// ── Node / edge type discriminants ───────────────────────────────────────────

/// V8 node-type indices into the `node_types[0]` enum:
/// `["hidden","array","string","object","code","closure","regexp",
///   "number","native","synthetic","concatenated string",
///   "sliced string","symbol","bigint"]`.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeType {
    Array     = 1,
    Object    = 3,
    Synthetic = 9,
}

/// V8 edge-type indices into the `edge_types[0]` enum:
/// `["context","element","property","internal","hidden","shortcut","weak"]`.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EdgeType {
    Element  = 1,
    Property = 2,
    Shortcut = 5,
}

// ── Records ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct NodeRec {
    pub node_type:     NodeType,
    pub name_idx:      u32,    // index into `strings`
    pub id:            u32,    // V8 ids: root = 0, others = 2k+1 (odd)
    pub self_size:     u32,    // bytes
    pub edge_count:    u32,    // populated in pass 2
    pub trace_node_id: u32,    // always 0 in v1
}

#[derive(Debug, Clone)]
pub struct EdgeRec {
    pub from_node_idx: u32,    // index into `nodes`; used to update edge_count
    pub edge_type:     EdgeType,
    pub name_or_index: u32,    // string idx (Property/Shortcut) or numeric index (Element)
    pub to_node_id:    u32,
}

// ── GraphSnapshot ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default)]
pub struct GraphSnapshot {
    pub nodes:        Vec<NodeRec>,
    pub edges:        Vec<EdgeRec>,
    pub strings:      Vec<String>,
    pub root_node_id: u32,
    string_index:     HashMap<String, u32>,
}

impl GraphSnapshot {
    /// Construct an empty snapshot with the synthetic root pre-installed
    /// at node-array index 0 / id 0.
    pub fn new() -> Self {
        let mut s = Self {
            nodes:        Vec::new(),
            edges:        Vec::new(),
            strings:      Vec::new(),
            root_node_id: 0,
            string_index: HashMap::new(),
        };
        // Intern the canonical root name first so that index 0 is stable.
        let root_name_idx = s.intern_str("(GC roots)");
        s.nodes.push(NodeRec {
            node_type:     NodeType::Synthetic,
            name_idx:      root_name_idx,
            id:            0,
            self_size:     0,
            edge_count:    0,
            trace_node_id: 0,
        });
        s
    }

    pub fn intern_str(&mut self, s: &str) -> u32 {
        if let Some(&i) = self.string_index.get(s) {
            return i;
        }
        let i = self.strings.len() as u32;
        self.strings.push(s.to_string());
        self.string_index.insert(s.to_string(), i);
        i
    }

    /// Append a node, returning its V8 id. Sequential `2 * (k-1) + 1`
    /// (root reserved id=0; others are odd per V8 convention).
    pub fn push_node(
        &mut self,
        node_type: NodeType,
        name_idx:  u32,
        self_size: u32,
    ) -> u32 {
        let k = self.nodes.len() as u32; // 1, 2, 3, ...
        let id = 2 * k - 1; // 1, 3, 5, ...  (root is k=0 → handled in `new`)
        self.nodes.push(NodeRec {
            node_type,
            name_idx,
            id,
            self_size,
            edge_count: 0,
            trace_node_id: 0,
        });
        id
    }

    pub fn push_edge(
        &mut self,
        from_node_idx: u32,
        edge_type:     EdgeType,
        name_or_index: u32,
        to_node_id:    u32,
    ) {
        self.edges.push(EdgeRec {
            from_node_idx,
            edge_type,
            name_or_index,
            to_node_id,
        });
        // Bump the source node's edge_count for V8's flat layout
        // (edges are emitted in node-index order during serialization).
        let i = from_node_idx as usize;
        if let Some(n) = self.nodes.get_mut(i) {
            n.edge_count = n.edge_count.saturating_add(1);
        }
    }
}

// ── Builder ──────────────────────────────────────────────────────────────────

/// Stable pointer-identity key for `Value::Object` / `Value::Array`.
/// Returns `None` for non-heap values (primitives, refs, closures).
fn value_ptr(v: &Value) -> Option<usize> {
    match v {
        Value::Object(gc) => Some(crate::gc::GcRef::as_ptr(gc) as usize),
        Value::Array(gc)  => Some(crate::gc::GcRef::as_ptr(gc) as usize),
        _ => None,
    }
}

/// Resolve `(NodeType, display_name, self_size_bytes)` for a heap value.
fn node_desc(v: &Value, heap: &dyn MagrGC) -> (NodeType, String, u32) {
    let size = heap.object_size_bytes(v) as u32;
    match v {
        Value::Object(gc) => {
            let name = {
                let obj = gc.borrow();
                obj.type_desc.name.clone()
            };
            (NodeType::Object, name, size)
        }
        Value::Array(gc) => {
            let len = gc.borrow().len();
            (NodeType::Array, format!("Array[{}]", len), size)
        }
        _ => unreachable!("node_desc called on non-heap value"),
    }
}

/// Walk the heap and assemble a graph snapshot.
///
/// Algorithm (two-pass; see design.md Decision 2):
/// 1. Enumerate every live `Value::Object` / `Value::Array` via
///    [`iterate_live_objects`], assign each a unique V8 node id, push
///    the corresponding `NodeRec`.
/// 2. For each node, walk its outgoing references via
///    [`scan_object_refs`]; for refs whose target was indexed in pass
///    1, emit one [`EdgeRec`] of the appropriate type
///    (Property for object slots, Element for array indices).
/// 3. Walk pinned + external roots via [`for_each_root`]; for each
///    rooted heap value, emit a Shortcut edge from the synthetic root
///    (node index 0 / id 0).
pub fn build_graph_snapshot(heap: &dyn MagrGC) -> GraphSnapshot {
    let mut snap = GraphSnapshot::new();
    let mut id_map: HashMap<usize, u32> = HashMap::new();   // ptr → V8 id
    let mut idx_map: HashMap<u32, u32>   = HashMap::new();  // id  → nodes[] index

    // Pass 1: collect alive values, assign ids, emit nodes.
    let mut alive: Vec<Value> = Vec::new();
    heap.iterate_live_objects(&mut |v| {
        if value_ptr(v).is_some() {
            alive.push(v.clone());
        }
    });

    for v in &alive {
        let ptr = match value_ptr(v) { Some(p) => p, None => continue };
        if id_map.contains_key(&ptr) {
            continue;
        }
        let (ty, name, size) = node_desc(v, heap);
        let name_idx = snap.intern_str(&name);
        let id = snap.push_node(ty, name_idx, size);
        let idx = (snap.nodes.len() - 1) as u32;
        id_map.insert(ptr, id);
        idx_map.insert(id, idx);
    }

    // Pass 2: edges.
    for v in &alive {
        let from_id  = match value_ptr(v) { Some(p) => id_map[&p], None => continue };
        let from_idx = idx_map[&from_id];

        // Capture slot names (for object) or indices (for array).
        // We can't borrow inside the closure passed to scan_object_refs
        // without locking conflicts on Mutex (scan walks the same
        // borrow). So pre-materialise slot-info into a list.
        match v {
            Value::Object(gc) => {
                let obj = gc.borrow();
                let nfields = obj.type_desc.fields.len();
                let names: Vec<String> = (0..nfields)
                    .map(|i| {
                        obj.type_desc
                            .fields
                            .get(i)
                            .map(|f| f.name.to_string())
                            .unwrap_or_else(|| format!("slot{}", i))
                    })
                    .collect();
                // unify-object-byte-layout (PR-2): materialize each field's Value
                // (primitives decode; reference fields yield the `refs` cell). Only
                // reference children matter to the snapshot graph (`value_ptr`).
                let slots: Vec<Value> = (0..nfields).map(|i| obj.field_value(i)).collect();
                drop(obj);
                for (i, child) in slots.iter().enumerate() {
                    if let Some(ptr) = value_ptr(child) {
                        if let Some(&to_id) = id_map.get(&ptr) {
                            let name_idx = snap.intern_str(&names[i]);
                            snap.push_edge(from_idx, EdgeType::Property, name_idx, to_id);
                        }
                    }
                }
            }
            Value::Array(gc) => {
                let arr = gc.borrow();
                let elems: Vec<Value> = arr.iter_boxed().collect();
                drop(arr);
                for (i, child) in elems.iter().enumerate() {
                    if let Some(ptr) = value_ptr(child) {
                        if let Some(&to_id) = id_map.get(&ptr) {
                            snap.push_edge(from_idx, EdgeType::Element, i as u32, to_id);
                        }
                    }
                }
            }
            _ => {}
        }
    }

    // Pass 3: synthetic root → pinned + external roots (shortcut edges).
    // Use a set to dedupe in case a root is reachable multiple ways.
    let mut root_targets: Vec<u32> = Vec::new();
    let mut seen_root: std::collections::HashSet<u32> = std::collections::HashSet::new();
    heap.for_each_root(&mut |v| {
        if let Some(ptr) = value_ptr(v) {
            if let Some(&to_id) = id_map.get(&ptr) {
                if seen_root.insert(to_id) {
                    root_targets.push(to_id);
                }
            }
        }
    });
    let empty_name_idx = snap.intern_str("");
    for to_id in root_targets {
        snap.push_edge(0, EdgeType::Shortcut, empty_name_idx, to_id);
    }

    snap
}

// ── V8 JSON serializer ───────────────────────────────────────────────────────

const NODE_FIELD_COUNT: u32 = 7;
const META_HEADER: &str = r##""meta":{"node_fields":["type","name","id","self_size","edge_count","trace_node_id","detachedness"],"node_types":[["hidden","array","string","object","code","closure","regexp","number","native","synthetic","concatenated string","sliced string","symbol","bigint"],"string","number","number","number","number","number"],"edge_fields":["type","name_or_index","to_node"],"edge_types":[["context","element","property","internal","hidden","shortcut","weak"],"string_or_number","node"],"trace_function_info_fields":["function_id","name","script_name","script_id","line","column"],"trace_node_fields":["id","function_info_index","count","size","children"],"sample_fields":["timestamp_us","last_assigned_id"],"location_fields":["object_index","script_id","line","column"]}"##;

fn to_node_offset(to_node_id: u32, idx_map: &HashMap<u32, u32>) -> u32 {
    // V8 `to_node` is a byte-offset into the nodes array measured in
    // node-field count: `(node_index_in_array) * NODE_FIELD_COUNT`.
    let idx = idx_map.get(&to_node_id).copied().unwrap_or(0);
    idx * NODE_FIELD_COUNT
}

/// Streaming JSON string escape — writes a `"...string..."` token
/// directly to `writer`. Returns bytes written (including the
/// surrounding quotes). Sole implementation of escape rules; both
/// the streaming `serialize_v8_heapsnapshot_to` and the in-memory
/// `serialize_v8_heapsnapshot` wrapper drive it.
fn escape_json_str_to<W: Write>(s: &str, writer: &mut W) -> io::Result<u64> {
    let mut n: u64 = 0;
    writer.write_all(b"\"")?; n += 1;
    let mut buf = [0u8; 4];
    for c in s.chars() {
        match c {
            '"'    => { writer.write_all(b"\\\"")?; n += 2; }
            '\\'   => { writer.write_all(b"\\\\")?; n += 2; }
            '\n'   => { writer.write_all(b"\\n")?;  n += 2; }
            '\r'   => { writer.write_all(b"\\r")?;  n += 2; }
            '\t'   => { writer.write_all(b"\\t")?;  n += 2; }
            '\x08' => { writer.write_all(b"\\b")?;  n += 2; }
            '\x0c' => { writer.write_all(b"\\f")?;  n += 2; }
            c if (c as u32) < 0x20 => {
                let s = format!("\\u{:04x}", c as u32);
                writer.write_all(s.as_bytes())?;
                n += s.len() as u64;
            }
            c => {
                let enc = c.encode_utf8(&mut buf);
                writer.write_all(enc.as_bytes())?;
                n += enc.len() as u64;
            }
        }
    }
    writer.write_all(b"\"")?; n += 1;
    Ok(n)
}

/// Streaming serializer: writes V8 `.heapsnapshot` JSON directly to
/// `writer`. Eliminates the ~30 MB intermediate `String` that the
/// historical in-memory `serialize_v8_heapsnapshot` allocates for a
/// large heap. Returns total bytes written.
///
/// Output is byte-identical to [`serialize_v8_heapsnapshot`]
/// (regression-tested in `snapshot_tests::streaming_and_in_memory_produce_identical_bytes`).
pub fn serialize_v8_heapsnapshot_to<W: Write>(
    snap: &GraphSnapshot,
    writer: &mut W,
) -> io::Result<u64> {
    let mut n_bytes: u64 = 0;
    let node_count = snap.nodes.len();
    let edge_count = snap.edges.len();

    // Build the id → node-array-index map (snapshot doesn't keep it).
    let mut idx_map: HashMap<u32, u32> = HashMap::with_capacity(node_count);
    for (i, n) in snap.nodes.iter().enumerate() {
        idx_map.insert(n.id, i as u32);
    }

    // Group edges by source-node index for V8's expected flat order.
    let mut edges_by_src: Vec<Vec<&EdgeRec>> = vec![Vec::new(); node_count];
    for e in &snap.edges {
        let i = e.from_node_idx as usize;
        if i < edges_by_src.len() {
            edges_by_src[i].push(e);
        }
    }

    // Lightweight emit helpers — keep call sites compact.
    macro_rules! wb {
        ($lit:expr) => {{
            let bytes: &[u8] = $lit;
            writer.write_all(bytes)?;
            n_bytes += bytes.len() as u64;
        }};
    }
    macro_rules! wfmt {
        ($($arg:tt)*) => {{
            let s = format!($($arg)*);
            writer.write_all(s.as_bytes())?;
            n_bytes += s.len() as u64;
        }};
    }

    wb!(b"{\"snapshot\":{");
    wb!(META_HEADER.as_bytes());
    wfmt!(",\"node_count\":{}", node_count);
    wfmt!(",\"edge_count\":{}", edge_count);
    wb!(b",\"trace_function_count\":0},");

    wb!(b"\"nodes\":[");
    for (i, n) in snap.nodes.iter().enumerate() {
        if i > 0 { wb!(b","); }
        wfmt!(
            "{},{},{},{},{},{},0",
            n.node_type as u8,
            n.name_idx,
            n.id,
            n.self_size,
            n.edge_count,
            n.trace_node_id,
        );
    }
    wb!(b"],");

    wb!(b"\"edges\":[");
    let mut first_edge = true;
    for src_edges in &edges_by_src {
        for e in src_edges {
            if !first_edge { wb!(b","); }
            first_edge = false;
            wfmt!(
                "{},{},{}",
                e.edge_type as u8,
                e.name_or_index,
                to_node_offset(e.to_node_id, &idx_map),
            );
        }
    }
    wb!(b"],");

    wb!(b"\"trace_function_infos\":[],");
    wb!(b"\"trace_tree\":[],");
    wb!(b"\"samples\":[],");
    wb!(b"\"locations\":[],");

    wb!(b"\"strings\":[");
    for (i, s) in snap.strings.iter().enumerate() {
        if i > 0 { wb!(b","); }
        n_bytes += escape_json_str_to(s, writer)?;
    }
    wb!(b"]");

    wb!(b"}");
    Ok(n_bytes)
}

/// In-memory wrapper: drives [`serialize_v8_heapsnapshot_to`] over a
/// `Vec<u8>` sink and returns the result as a `String`. Retained for
/// backward compatibility with existing unit tests + any caller that
/// needs the whole snapshot as a value (rather than streaming to a
/// file). For large heaps prefer the streaming form directly.
pub fn serialize_v8_heapsnapshot(snap: &GraphSnapshot) -> String {
    let mut buf: Vec<u8> = Vec::with_capacity(
        1024 + snap.nodes.len() * 48 + snap.edges.len() * 24,
    );
    // SAFETY: `Vec<u8>` `Write` impl is infallible.
    serialize_v8_heapsnapshot_to(snap, &mut buf)
        .expect("Vec<u8>::write_all never fails");
    // SAFETY: `escape_json_str_to` emits ASCII-safe JSON escape sequences
    // and `char::encode_utf8` always produces valid UTF-8 — the buffer
    // is well-formed UTF-8 by construction.
    unsafe { String::from_utf8_unchecked(buf) }
}

#[cfg(test)]
#[path = "snapshot_tests.rs"]
mod snapshot_tests;
