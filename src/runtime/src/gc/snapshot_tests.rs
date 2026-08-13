//! Unit tests for V8 heap-snapshot export
//! (add-gc-heap-snapshot-export, 2026-05-24).
//!
//! Covers graph build correctness (id assignment, edge typing,
//! cycles) and V8 JSON layout shape (top-level keys, meta layout,
//! flat array lengths matching node/edge counts, string dedup).

use super::*;
use crate::gc::{ArcMagrGC, GcRef, MagrGC};
use crate::metadata::{NativeData, ScriptObject, TypeDesc, Value};
use std::sync::Arc;

fn dummy_type_desc(name: &str) -> Arc<TypeDesc> {
    let fields = vec![
        crate::metadata::FieldSlot { name: "head".to_string().into(), type_tag: "object".to_string().into(), visibility: 0 },
        crate::metadata::FieldSlot { name: "tail".to_string().into(), type_tag: "object".to_string().into(), visibility: 0 },
    ];
    let mut field_index = crate::metadata::NameIndex::new();
    field_index.insert("head".to_string(), 0usize);
    field_index.insert("tail".to_string(), 1usize);
    Arc::new(TypeDesc {
        class_flags: 0,
        visibility: 0,
        name: name.to_string(),
        base_name: None,
        cold: Some(Box::new(crate::metadata::types::TypeDescCold {
            own_fields: fields.clone().into(),
            ..Default::default()
        })),
        fields,
        field_index,
        vtable: Vec::new(),
        vtable_index: crate::metadata::NameIndex::new(),
        id: crate::metadata::tokens::TypeId::UNRESOLVED,
    })
}

fn alloc_obj_with_fields(heap: &ArcMagrGC, td: Arc<TypeDesc>, slots: Vec<Value>) -> Value {
    heap.alloc_object(td, slots, NativeData::None)
}

#[test]
fn empty_heap_produces_root_only() {
    let heap = ArcMagrGC::default();
    let snap = build_graph_snapshot(&heap);
    assert_eq!(snap.nodes.len(), 1, "only the synthetic root");
    assert_eq!(snap.edges.len(), 0, "no edges from a 0-object heap");
    assert_eq!(snap.nodes[0].id, 0, "root id is 0");
    assert_eq!(snap.nodes[0].node_type, NodeType::Synthetic);
    assert!(snap.strings.iter().any(|s| s == "(GC roots)"));
}

#[test]
fn single_object_creates_node_with_property_edges() {
    let heap = ArcMagrGC::default();
    let td = dummy_type_desc("Foo");
    let v = alloc_obj_with_fields(&heap, td, vec![Value::Null, Value::Null]);
    let root = heap.pin_root(v.clone());

    let snap = build_graph_snapshot(&heap);
    // root + 1 object
    assert_eq!(snap.nodes.len(), 2);
    // The pinned root contributes 1 shortcut edge from root → Foo.
    // No interior heap refs (slots are Null), so 0 property edges.
    assert_eq!(snap.edges.len(), 1);
    assert_eq!(snap.edges[0].edge_type, EdgeType::Shortcut);

    // Foo node has the right metadata
    let foo_node = &snap.nodes[1];
    assert_eq!(foo_node.node_type, NodeType::Object);
    assert_eq!(snap.strings[foo_node.name_idx as usize], "Foo");

    heap.unpin_root(root);
}

#[test]
fn linked_objects_emit_property_edges() {
    let heap = ArcMagrGC::default();
    let td = dummy_type_desc("Node");

    let b = alloc_obj_with_fields(&heap, td.clone(), vec![Value::Null, Value::Null]);
    let a = alloc_obj_with_fields(&heap, td.clone(), vec![b.clone(), Value::Null]);
    let root = heap.pin_root(a.clone());

    let snap = build_graph_snapshot(&heap);
    // root + a + b = 3 nodes
    assert_eq!(snap.nodes.len(), 3);

    let property_edges: Vec<_> = snap.edges.iter()
        .filter(|e| e.edge_type == EdgeType::Property)
        .collect();
    assert_eq!(property_edges.len(), 1, "exactly one a.head = b edge");

    // The edge's name should be "head" (slot 0)
    let head_name_idx = snap.strings.iter()
        .position(|s| s == "head")
        .expect("string table must contain 'head'") as u32;
    assert_eq!(property_edges[0].name_or_index, head_name_idx);

    heap.unpin_root(root);
}

#[test]
fn array_emits_element_edges_with_index() {
    let heap = ArcMagrGC::default();
    let td = dummy_type_desc("Elem");
    let e0 = alloc_obj_with_fields(&heap, td.clone(), vec![Value::Null, Value::Null]);
    let e1 = alloc_obj_with_fields(&heap, td.clone(), vec![Value::Null, Value::Null]);

    let arr = heap.alloc_array(vec![e0.clone(), e1.clone(), Value::Null]);
    let root = heap.pin_root(arr.clone());

    let snap = build_graph_snapshot(&heap);
    // root + 2 objects + 1 array = 4 nodes
    assert_eq!(snap.nodes.len(), 4);

    let elem_edges: Vec<_> = snap.edges.iter()
        .filter(|e| e.edge_type == EdgeType::Element)
        .collect();
    assert_eq!(elem_edges.len(), 2);
    // The two element-edge name_or_index values should be 0 and 1 (the
    // indices), not string-table indices. Order may vary by walk order
    // so collect into a set.
    let mut indices: Vec<u32> = elem_edges.iter().map(|e| e.name_or_index).collect();
    indices.sort();
    assert_eq!(indices, vec![0, 1]);

    heap.unpin_root(root);
}

#[test]
fn cycle_does_not_loop() {
    let heap = ArcMagrGC::default();
    let td = dummy_type_desc("Node");

    let a = alloc_obj_with_fields(&heap, td.clone(), vec![Value::Null, Value::Null]);
    let b = alloc_obj_with_fields(&heap, td.clone(), vec![Value::Null, Value::Null]);
    // a.head = b
    if let Value::Object(gc) = &a {
        gc.borrow_mut().slots[0] = b.clone();
    }
    // b.head = a — completes the cycle
    if let Value::Object(gc) = &b {
        gc.borrow_mut().slots[0] = a.clone();
    }
    let root = heap.pin_root(a.clone());

    // If the builder loops, this test times out / OOMs; we assert it
    // terminates AND produces exactly the expected graph.
    let snap = build_graph_snapshot(&heap);
    assert_eq!(snap.nodes.len(), 3, "root + 2 objects");
    let property_edges: Vec<_> = snap.edges.iter()
        .filter(|e| e.edge_type == EdgeType::Property)
        .collect();
    assert_eq!(property_edges.len(), 2, "a→b and b→a property edges");

    heap.unpin_root(root);
}

#[test]
fn node_ids_are_odd_except_root() {
    let heap = ArcMagrGC::default();
    let td = dummy_type_desc("Foo");
    let v1 = alloc_obj_with_fields(&heap, td.clone(), vec![Value::Null, Value::Null]);
    let v2 = alloc_obj_with_fields(&heap, td.clone(), vec![Value::Null, Value::Null]);
    let r1 = heap.pin_root(v1.clone());
    let r2 = heap.pin_root(v2.clone());

    let snap = build_graph_snapshot(&heap);
    assert_eq!(snap.nodes[0].id, 0, "root is id 0 by convention");
    for n in &snap.nodes[1..] {
        assert_eq!(n.id % 2, 1, "non-root node ids must be odd (V8 convention)");
    }

    heap.unpin_root(r1);
    heap.unpin_root(r2);
}

#[test]
fn serialized_json_has_expected_structure() {
    let heap = ArcMagrGC::default();
    let td = dummy_type_desc("Foo");
    let v = alloc_obj_with_fields(&heap, td, vec![Value::Null, Value::Null]);
    let root = heap.pin_root(v.clone());

    let snap = build_graph_snapshot(&heap);
    let json = serialize_v8_heapsnapshot(&snap);

    // Top-level keys.
    for key in [
        "\"snapshot\"",
        "\"nodes\"",
        "\"edges\"",
        "\"trace_function_infos\"",
        "\"trace_tree\"",
        "\"samples\"",
        "\"locations\"",
        "\"strings\"",
    ] {
        assert!(json.contains(key), "JSON must contain top-level key {}", key);
    }
    // Meta sub-keys.
    assert!(json.contains("\"node_fields\":[\"type\",\"name\",\"id\",\"self_size\",\"edge_count\",\"trace_node_id\",\"detachedness\"]"));
    assert!(json.contains("\"edge_fields\":[\"type\",\"name_or_index\",\"to_node\"]"));
    // Counts match.
    assert!(json.contains(&format!("\"node_count\":{}", snap.nodes.len())));
    assert!(json.contains(&format!("\"edge_count\":{}", snap.edges.len())));
    // Empty trace/sample/location arrays.
    assert!(json.contains("\"trace_function_infos\":[]"));
    assert!(json.contains("\"trace_tree\":[]"));
    assert!(json.contains("\"samples\":[]"));
    assert!(json.contains("\"locations\":[]"));
    // String table contains the root sentinel.
    assert!(json.contains("\"(GC roots)\""));
    // The class-name "Foo" should appear in strings.
    assert!(json.contains("\"Foo\""));

    heap.unpin_root(root);
}

#[test]
fn flat_arrays_have_correct_lengths() {
    let heap = ArcMagrGC::default();
    let td = dummy_type_desc("Node");

    let b = alloc_obj_with_fields(&heap, td.clone(), vec![Value::Null, Value::Null]);
    let a = alloc_obj_with_fields(&heap, td.clone(), vec![b.clone(), Value::Null]);
    let root = heap.pin_root(a.clone());

    let snap = build_graph_snapshot(&heap);
    let node_count = snap.nodes.len();
    let edge_count = snap.edges.len();

    let json = serialize_v8_heapsnapshot(&snap);

    // Extract the nodes array contents and count comma-separated ints.
    // Robust enough for the simple layout we emit (no nested arrays
    // inside `nodes` / `edges`).
    let nodes_marker = "\"nodes\":[";
    let edges_marker = "\"edges\":[";
    let nodes_start = json.find(nodes_marker).unwrap() + nodes_marker.len();
    let nodes_end = json[nodes_start..].find(']').unwrap() + nodes_start;
    let edges_start = json.find(edges_marker).unwrap() + edges_marker.len();
    let edges_end = json[edges_start..].find(']').unwrap() + edges_start;

    let nodes_body = &json[nodes_start..nodes_end];
    let edges_body = &json[edges_start..edges_end];

    let node_ints = nodes_body.split(',').filter(|s| !s.is_empty()).count();
    let edge_ints = edges_body.split(',').filter(|s| !s.is_empty()).count();

    // 7 fields per node, 3 fields per edge (V8 layout).
    assert_eq!(node_ints, node_count * 7, "nodes flat array length");
    assert_eq!(edge_ints, edge_count * 3, "edges flat array length");

    heap.unpin_root(root);
}

#[test]
fn string_table_is_deduped() {
    let heap = ArcMagrGC::default();
    let td_foo = dummy_type_desc("Foo");

    // Allocate 5 Foo objects → they all share the type name "Foo".
    let mut pins = Vec::new();
    for _ in 0..5 {
        let v = alloc_obj_with_fields(&heap, td_foo.clone(), vec![Value::Null, Value::Null]);
        pins.push(heap.pin_root(v.clone()));
    }

    let snap = build_graph_snapshot(&heap);
    let foo_count = snap.strings.iter().filter(|s| s.as_str() == "Foo").count();
    assert_eq!(foo_count, 1, "'Foo' must be interned exactly once");

    for p in pins { heap.unpin_root(p); }
}

#[test]
fn root_edge_count_matches_pinned_root_count() {
    let heap = ArcMagrGC::default();
    let td = dummy_type_desc("Foo");
    let v1 = alloc_obj_with_fields(&heap, td.clone(), vec![Value::Null, Value::Null]);
    let v2 = alloc_obj_with_fields(&heap, td.clone(), vec![Value::Null, Value::Null]);
    let v3 = alloc_obj_with_fields(&heap, td.clone(), vec![Value::Null, Value::Null]);
    let r1 = heap.pin_root(v1.clone());
    let r2 = heap.pin_root(v2.clone());
    let r3 = heap.pin_root(v3.clone());

    let snap = build_graph_snapshot(&heap);
    // Root node (index 0) edge_count must equal the number of unique
    // pinned roots (3 in this case — no dedup across distinct objects).
    let root_edges: Vec<_> = snap.edges.iter()
        .filter(|e| e.from_node_idx == 0)
        .collect();
    assert_eq!(root_edges.len(), 3);
    assert_eq!(snap.nodes[0].edge_count, 3, "root node edge_count field");

    heap.unpin_root(r1);
    heap.unpin_root(r2);
    heap.unpin_root(r3);

    // Touch ScriptObject import so it's not flagged as unused.
    let _ = std::mem::size_of::<ScriptObject>();
    let _ = GcRef::<Vec<Value>>::clone;
}

// ── add-gc-snapshot-streaming (2026-05-25) ───────────────────────────────────

/// Build a small but non-trivial snapshot the streaming tests can share.
/// Three nodes (root + 2 objects, one referencing the other) + a few
/// shortcut + property edges. Includes string-table entries that exercise
/// escape paths (newline + double-quote inside a class name to force
/// `escape_json_str_to` through its replace branches).
fn build_sample_snapshot_with_escapes() -> (ArcMagrGC, crate::gc::GraphSnapshot, Vec<crate::gc::RootHandle>) {
    let heap = ArcMagrGC::default();
    let td = dummy_type_desc("Has\"weird\nname");
    let b = alloc_obj_with_fields(&heap, td.clone(), vec![Value::Null, Value::Null]);
    let a = alloc_obj_with_fields(&heap, td.clone(), vec![b.clone(), Value::Null]);
    let r1 = heap.pin_root(a.clone());
    let r2 = heap.pin_root(b.clone());
    let snap = build_graph_snapshot(&heap);
    (heap, snap, vec![r1, r2])
}

#[test]
fn streaming_and_in_memory_produce_identical_bytes() {
    let (heap, snap, roots) = build_sample_snapshot_with_escapes();

    let from_string = serialize_v8_heapsnapshot(&snap);
    let mut from_stream: Vec<u8> = Vec::new();
    let n = serialize_v8_heapsnapshot_to(&snap, &mut from_stream).unwrap();

    assert_eq!(
        n as usize,
        from_string.len(),
        "streaming byte count must equal in-memory string length",
    );
    assert_eq!(
        from_stream,
        from_string.as_bytes(),
        "streaming output must be byte-identical to in-memory output",
    );

    for r in roots { heap.unpin_root(r); }
}

#[test]
fn streaming_byte_count_matches_string_length() {
    // Empty heap variant — exercises the minimal-content path.
    let heap = ArcMagrGC::default();
    let snap = build_graph_snapshot(&heap);

    let from_string = serialize_v8_heapsnapshot(&snap);
    let mut buf: Vec<u8> = Vec::new();
    let n = serialize_v8_heapsnapshot_to(&snap, &mut buf).unwrap();

    assert_eq!(n as usize, from_string.len());
    assert_eq!(n as usize, buf.len());
}

#[test]
fn streaming_writes_to_buffered_writer_roundtrip() {
    use std::io::{BufWriter, Read, Write as _};
    let (heap, snap, roots) = build_sample_snapshot_with_escapes();

    // Write to a tempfile via BufWriter — the path the production
    // builtin takes.
    let tmp = std::env::temp_dir().join(format!(
        "z42-snapshot-stream-test-{}.heapsnapshot",
        std::process::id(),
    ));
    {
        let file = std::fs::File::create(&tmp).expect("create tmp");
        let mut writer = BufWriter::new(file);
        let n = serialize_v8_heapsnapshot_to(&snap, &mut writer).unwrap();
        writer.flush().expect("flush");
        // Bytes written should match the in-memory string length.
        let expected = serialize_v8_heapsnapshot(&snap);
        assert_eq!(n as usize, expected.len());
    }

    // Read back; bytes must equal the in-memory version exactly.
    let mut got: Vec<u8> = Vec::new();
    std::fs::File::open(&tmp).unwrap()
        .read_to_end(&mut got).unwrap();
    let expected = serialize_v8_heapsnapshot(&snap);
    assert_eq!(got, expected.as_bytes());

    let _ = std::fs::remove_file(&tmp);
    for r in roots { heap.unpin_root(r); }
}
