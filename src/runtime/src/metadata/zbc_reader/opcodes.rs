//! Centralized zbc opcode + tag byte constants (M3: single reader-side source,
//! mirror of C# `Opcodes.cs`). Consumed by `instr_decode::decode_instr`.
//!
//! Deliberately *just constants* — no `OpcodeInfo` name/metadata table is built
//! here: the decoder matches raw bytes and nothing in the runtime iterates
//! opcodes by name, so a table would be infrastructure without a consumer. Add
//! one only alongside a real consumer (e.g. a disassembler/linter).

// ── Opcode constants (must match C# Opcodes.cs) ───────────────────────────────

pub(super) const OP_CONST_I: u8     = 0x00;
pub(super) const OP_CONST_F: u8     = 0x01;
pub(super) const OP_CONST_BOOL: u8  = 0x02;
pub(super) const OP_CONST_STR: u8   = 0x03;
pub(super) const OP_CONST_NULL: u8  = 0x04;
pub(super) const OP_COPY: u8        = 0x05;
pub(super) const OP_CONST_CHAR: u8  = 0x08;

pub(super) const OP_ADD: u8         = 0x10;
pub(super) const OP_SUB: u8         = 0x11;
pub(super) const OP_MUL: u8         = 0x12;
pub(super) const OP_DIV: u8         = 0x13;
pub(super) const OP_REM: u8         = 0x14;
pub(super) const OP_NEG: u8         = 0x15;
pub(super) const OP_AND: u8         = 0x16;
pub(super) const OP_OR: u8          = 0x17;
pub(super) const OP_NOT: u8         = 0x18;
pub(super) const OP_BIT_AND: u8     = 0x19;
pub(super) const OP_BIT_OR: u8      = 0x1A;
pub(super) const OP_BIT_XOR: u8     = 0x1B;
pub(super) const OP_BIT_NOT: u8     = 0x1C;
pub(super) const OP_SHL: u8         = 0x1D;
pub(super) const OP_SHR: u8         = 0x1E;
pub(super) const OP_TO_STR: u8      = 0x1F;

pub(super) const OP_EQ: u8          = 0x30;
pub(super) const OP_NE: u8          = 0x31;
pub(super) const OP_LT: u8          = 0x32;
pub(super) const OP_LE: u8          = 0x33;
pub(super) const OP_GT: u8          = 0x34;
pub(super) const OP_GE: u8          = 0x35;

pub(super) const OP_BR: u8          = 0x40;
pub(super) const OP_BR_COND: u8     = 0x41;
pub(super) const OP_RET: u8         = 0x42;
pub(super) const OP_RET_VAL: u8     = 0x43;
pub(super) const OP_THROW: u8       = 0x44;

pub(super) const OP_CALL: u8                = 0x50;
pub(super) const OP_BUILTIN: u8             = 0x51;
pub(super) const OP_VCALL: u8               = 0x52;
pub(super) const OP_CALL_NATIVE: u8         = 0x53;
pub(super) const OP_CALL_NATIVE_VTABLE: u8  = 0x54;
pub(super) const OP_LOAD_FN: u8             = 0x55;
pub(super) const OP_CALL_INDIRECT: u8       = 0x56;
pub(super) const OP_MK_CLOS: u8             = 0x57;
pub(super) const OP_LOAD_FN_CACHED: u8      = 0x58;  // D1b add-method-group-conversion

pub(super) const OP_FIELD_GET: u8   = 0x60;
pub(super) const OP_FIELD_SET: u8   = 0x61;
pub(super) const OP_STATIC_GET: u8  = 0x62;
pub(super) const OP_STATIC_SET: u8  = 0x63;

pub(super) const OP_OBJ_NEW: u8     = 0x70;
pub(super) const OP_IS_INSTANCE: u8 = 0x71;
pub(super) const OP_AS_CAST: u8     = 0x72;
pub(super) const OP_TYPEOF: u8      = 0x73;

pub(super) const OP_ARRAY_NEW: u8     = 0x80;
pub(super) const OP_ARRAY_NEW_LIT: u8 = 0x81;
pub(super) const OP_ARRAY_GET: u8     = 0x82;
pub(super) const OP_ARRAY_SET: u8     = 0x83;
pub(super) const OP_ARRAY_LEN: u8     = 0x84;
pub(super) const OP_STR_CONCAT: u8    = 0x85;

pub(super) const OP_PIN_PTR: u8       = 0x90;
pub(super) const OP_UNPIN_PTR: u8     = 0x91;

// Spec impl-ref-out-in-runtime: address-load opcodes producing Value::Ref.
pub(super) const OP_LOAD_LOCAL_ADDR: u8 = 0xA0;
pub(super) const OP_LOAD_ELEM_ADDR:  u8 = 0xA1;
pub(super) const OP_LOAD_FIELD_ADDR: u8 = 0xA2;

// add-default-generic-typeparam (D-8b-3 Phase 2): runtime resolution of
// `default(T)` where T is a generic type-parameter on the receiver class.
pub(super) const OP_DEFAULT_OF: u8 = 0xB0;
// fix-numeric-cast-lowering (2026-05-13): explicit numeric type conversion.
pub(super) const OP_CONVERT: u8 = 0xB1;
// add-generic-methods (2026-08-21, zbc 1.36): method-level generic type_args.
// Non-generic Call/VCall keep 0x50/0x52 (byte-identical); generic calls carry a
// method_type_args string list via CallGeneric/VCallGeneric.
pub(super) const OP_METHOD_TYPE_ARG: u8 = 0xB2;
pub(super) const OP_METHOD_DEFAULT: u8  = 0xB3;
pub(super) const OP_CALL_GENERIC: u8    = 0xB4;
pub(super) const OP_VCALL_GENERIC: u8   = 0xB5;

// add-struct-value-semantics Phase A: blob value type instructions (byte-region ops).
pub(super) const OP_STRUCT_ALLOC: u8          = 0xC0;
pub(super) const OP_STRUCT_COPY: u8           = 0xC1;
pub(super) const OP_STRUCT_FIELD_GET_PRIM: u8 = 0xC2;
pub(super) const OP_STRUCT_FIELD_SET_PRIM: u8 = 0xC3;

// ── Type tag constants ────────────────────────────────────────────────────────

pub(super) const TAG_I64: u8 = 0x05;
