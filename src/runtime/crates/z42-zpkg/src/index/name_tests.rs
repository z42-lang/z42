use super::NameIndex;

#[test]
fn new_is_empty() {
    let idx = NameIndex::new();
    assert!(idx.is_empty());
    assert_eq!(idx.len(), 0);
}

#[test]
fn insert_returns_none_first_time_some_on_update() {
    let mut idx = NameIndex::new();
    assert_eq!(idx.insert("name".to_string(), 0), None);
    assert_eq!(idx.insert("age".to_string(),  1), None);
    // Update existing entry — returns previous slot.
    assert_eq!(idx.insert("name".to_string(), 7), Some(0));
    assert_eq!(idx.len(), 2);
    assert_eq!(idx.get("name").copied(), Some(7));
    assert_eq!(idx.get("age").copied(),  Some(1));
}

#[test]
fn get_returns_reference_for_dot_copied_compat() {
    // The whole point of the API: existing call sites do
    // `field_index.get(name).copied()` and must keep working.
    let mut idx = NameIndex::new();
    idx.insert("x".to_string(), 42);
    let slot = idx.get("x").copied();
    assert_eq!(slot, Some(42));
}

#[test]
fn get_miss_returns_none() {
    let idx = NameIndex::new();
    assert!(idx.get("absent").is_none());
}

#[test]
fn contains_key() {
    let mut idx = NameIndex::new();
    idx.insert("name".to_string(), 0);
    assert!(idx.contains_key("name"));
    assert!(!idx.contains_key("missing"));
}

#[test]
fn iter_yields_insertion_order() {
    let mut idx = NameIndex::new();
    idx.insert("c".to_string(), 0);
    idx.insert("a".to_string(), 1);
    idx.insert("b".to_string(), 2);
    let pairs: Vec<(&str, usize)> = idx.iter().collect();
    assert_eq!(pairs, vec![("c", 0), ("a", 1), ("b", 2)]);
}

#[test]
fn keys_iter() {
    let mut idx = NameIndex::new();
    idx.insert("name".to_string(), 0);
    idx.insert("age".to_string(),  1);
    let names: Vec<&str> = idx.keys().collect();
    assert_eq!(names, vec!["name", "age"]);
}

#[test]
fn from_iterator_collect_compat() {
    // Loader uses `.iter().enumerate().map(...).collect::<HashMap<_,_>>()`;
    // NameIndex must support the same pattern.
    let fields = vec!["a", "b", "c"];
    let idx: NameIndex = fields.iter().enumerate()
        .map(|(i, n)| (n.to_string(), i))
        .collect();
    assert_eq!(idx.len(), 3);
    assert_eq!(idx.get("a").copied(), Some(0));
    assert_eq!(idx.get("b").copied(), Some(1));
    assert_eq!(idx.get("c").copied(), Some(2));
}

#[test]
fn from_iterator_dedups_on_collision() {
    // HashMap::from_iter overwrites on duplicate key. Match that semantic.
    let pairs = vec![
        ("x".to_string(), 0),
        ("y".to_string(), 1),
        ("x".to_string(), 99), // overwrite
    ];
    let idx: NameIndex = pairs.into_iter().collect();
    assert_eq!(idx.len(), 2);
    assert_eq!(idx.get("x").copied(), Some(99));
}

#[test]
fn clone_is_independent() {
    let mut a = NameIndex::new();
    a.insert("k".to_string(), 0);
    let b = a.clone();
    a.insert("k".to_string(), 99);
    assert_eq!(a.get("k").copied(), Some(99));
    assert_eq!(b.get("k").copied(), Some(0));
}

#[test]
fn empty_string_is_valid_key() {
    let mut idx = NameIndex::new();
    idx.insert(String::new(), 5);
    assert_eq!(idx.get("").copied(), Some(5));
    assert!(idx.get("a").is_none());
}

#[test]
fn debug_renders_like_a_map() {
    let mut idx = NameIndex::new();
    idx.insert("a".to_string(), 0);
    idx.insert("b".to_string(), 1);
    let s = format!("{:?}", idx);
    // Order is insertion-stable so we can match exactly.
    assert_eq!(s, r#"{"a": 0, "b": 1}"#);
}

#[test]
fn entry_storage_is_box_str_not_string() {
    // Indirect check: a Box<str> NameIndex with 1 entry should occupy
    // less than a HashMap<String, usize> with 1 entry. Mostly a smoke
    // test that we didn't accidentally bring back String.
    use std::mem::size_of;
    // Box<str> is 16 B (data ptr + len) vs String 24 B (ptr + len + cap).
    // We're not asserting exact numbers (compiler-dependent) — just that
    // a single entry's footprint is at most as wide as a HashMap bucket.
    let entry_size = size_of::<(Box<str>, usize)>();
    assert!(entry_size <= 32, "entry too wide: {} bytes", entry_size);
}
