//! Validates `docs/design/compiler/manifest-schema.json` against representative inputs.
//!
//! C1 freezes the .z42abi manifest schema; later specs that read the manifest
//! (C5) rely on these scenarios staying green to know the schema is intact.

use std::fs;
use std::path::PathBuf;

use jsonschema::{Draft, Validator};
use serde_json::Value;

fn project_root() -> PathBuf {
    // CARGO_MANIFEST_DIR points at src/runtime/; project root is two up.
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("project root resolves above src/runtime/")
        .to_path_buf()
}

fn load_schema() -> Validator {
    let path = project_root().join("docs/design/compiler/manifest-schema.json");
    let raw = fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("read schema {}: {}", path.display(), e));
    let value: Value = serde_json::from_str(&raw).expect("schema is valid JSON");
    jsonschema::options()
        .with_draft(Draft::Draft202012)
        .build(&value)
        .expect("schema compiles under Draft 2020-12")
}

fn load_data(name: &str) -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/data")
        .join(name);
    let raw = fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("read {}: {}", path.display(), e));
    serde_json::from_str(&raw).expect("data file is valid JSON")
}

#[test]
fn schema_compiles_under_draft_2020_12() {
    let _ = load_schema();
}

#[test]
fn example_manifest_validates() {
    let schema = load_schema();
    let data = load_data("example-manifest.json");
    let messages: Vec<_> = schema.iter_errors(&data).map(|e| e.to_string()).collect();
    if !messages.is_empty() {
        panic!("example-manifest.json should validate:\n{}", messages.join("\n"));
    }
}

#[test]
fn missing_required_field_fails() {
    let schema = load_schema();
    let data = load_data("invalid-manifest-missing-types.json");
    assert!(
        !schema.is_valid(&data),
        "manifest missing `types` field MUST be rejected by schema"
    );
}

#[test]
fn unknown_fields_are_tolerated() {
    let schema = load_schema();
    let data = load_data("manifest-with-extra-fields.json");
    let messages: Vec<_> = schema.iter_errors(&data).map(|e| e.to_string()).collect();
    if !messages.is_empty() {
        panic!(
            "manifest with unknown fields should validate (forward compatibility):\n{}",
            messages.join("\n")
        );
    }
}
