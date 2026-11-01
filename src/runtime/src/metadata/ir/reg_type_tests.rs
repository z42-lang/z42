use super::*;

#[test]
fn default_is_unknown() {
    assert_eq!(IrType::default(), IrType::Unknown);
}

#[test]
fn discriminants_match_csharp() {
    // The byte values MUST stay in lockstep with `IrType : byte` in
    // src/compiler/z42.IR/IrModule.cs. Drift here breaks the wire
    // format and silently mis-types every register on the JIT side.
    assert_eq!(IrType::Unknown as u8, 0);
    assert_eq!(IrType::I8      as u8, 1);
    assert_eq!(IrType::I16     as u8, 2);
    assert_eq!(IrType::I32     as u8, 3);
    assert_eq!(IrType::I64     as u8, 4);
    assert_eq!(IrType::U8      as u8, 5);
    assert_eq!(IrType::U16     as u8, 6);
    assert_eq!(IrType::U32     as u8, 7);
    assert_eq!(IrType::U64     as u8, 8);
    assert_eq!(IrType::F32     as u8, 9);
    assert_eq!(IrType::F64     as u8, 10);
    assert_eq!(IrType::Bool    as u8, 11);
    assert_eq!(IrType::Char    as u8, 12);
    assert_eq!(IrType::Str     as u8, 13);
    assert_eq!(IrType::Ref     as u8, 14);
    assert_eq!(IrType::Void    as u8, 15);
}

#[test]
fn from_u8_roundtrips_known_variants() {
    for v in [
        IrType::Unknown, IrType::I8, IrType::I16, IrType::I32, IrType::I64,
        IrType::U8, IrType::U16, IrType::U32, IrType::U64,
        IrType::F32, IrType::F64,
        IrType::Bool, IrType::Char, IrType::Str, IrType::Ref, IrType::Void,
    ] {
        assert_eq!(IrType::from_u8(v as u8), v, "round-trip {:?}", v);
    }
}

#[test]
fn from_u8_unknown_for_future_bytes() {
    // Forward compat: writer may add variants 16+ before reader knows
    // them. They must decode as Unknown, not panic.
    for b in [16u8, 99, 200, u8::MAX] {
        assert_eq!(IrType::from_u8(b), IrType::Unknown, "byte {}", b);
    }
}

#[test]
fn classification_predicates() {
    assert!(IrType::I64.is_i64());
    assert!(!IrType::I32.is_i64());
    assert!(!IrType::F64.is_i64());

    assert!(IrType::F32.is_float());
    assert!(IrType::F64.is_float());
    assert!(!IrType::I64.is_float());

    assert!(IrType::I8.is_integer());
    assert!(IrType::I64.is_integer());
    assert!(IrType::U64.is_integer());
    assert!(!IrType::F64.is_integer());
    assert!(!IrType::Bool.is_integer());
    assert!(!IrType::Str.is_integer());
}

#[test]
fn enum_size_is_one_byte() {
    // `repr(u8)` keeps each `IrType` slot in a per-fn `Box<[IrType]>`
    // at one byte. Stdlib has ~3000 functions × ~10 regs ≈ 30 KB
    // resident — acceptable. If anyone accidentally drops `repr(u8)`,
    // this test traps it.
    assert_eq!(std::mem::size_of::<IrType>(), 1);
}
