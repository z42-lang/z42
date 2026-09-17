# Native Interoperability (FFI)

> **Status**: §1–§10 describe the long-term L2+ design (three-tier ABI). §11 documents the current L1 interim mechanism. §11.5 covers the migration plan.

---

## §1 Overview & Design Principles

z42 is designed to be **embedded in and callable from** native applications. FFI is a core concern, not an afterthought.

Five design pillars (derived from analysis of Python C API, C# P/Invoke, and Rust extern):

1. **Zero marshal** — cross-FFI data is 100% blittable. No automatic conversion. High-level types (`String`, `Array<T>`) cross via `pinned` blocks (see §6.3), not by copying.
2. **Native-defined script types** — native libraries can register complete z42 classes (fields, methods, ctor/dtor, trait impls). They participate in the z42 type system as first-class citizens. This closes C#'s biggest interop gap; Python C API does this, C# does not.
3. **Three-tier ABI** — stable C foundation + ergonomic Rust frontend + compile-time source generator. Each upper tier is sugar over the lower.
4. **Compile-time binding resolution** — bindings resolved at z42 compile time via `.z42abi` manifests; no runtime reflection. AOT-friendly. Errors caught at compile time.
5. **Strict safety boundary** — `unsafe` block required for raw FFI; native types managed by z42 RC; no GC-pointer leakage; panic does not cross the FFI line.

---

## §2 Three-Tier ABI Architecture

```
┌──────────────────────────────────────────────────────────────┐
│  Tier 3: Source Generator (compile-time bindings)            │
│    z42 compiler reads .z42abi manifest → emits IR vtable    │
│    Analogous to C# 11+ [LibraryImport] source generator      │
├──────────────────────────────────────────────────────────────┤
│  Tier 2: Rust Registration API (ergonomic frontend)          │
│    #[derive(Z42Type)] + #[z42::methods] + z42::module!      │
│    proc macros emit Tier 1 descriptors + extern "C" shims   │
├──────────────────────────────────────────────────────────────┤
│  Tier 1: C ABI (stable foundation)                          │
│    Z42TypeDescriptor_v1 + z42_register_type                 │
│    Cross-language portable; all upper tiers reduce to this  │
└──────────────────────────────────────────────────────────────┘
                              ↓
                    z42 VM TypeRegistry
```

**Layering principle**: anything an upper tier expresses is achievable by hand-writing the lower. This guarantees:

- Tier 1 stays small and ABI-stable
- Tier 2/3 evolve independently without breaking Tier 1 consumers
- Languages without Rust support (C, C++, Zig, Go cgo) target Tier 1 directly

---

## §3 Tier 1: C ABI (Stable Foundation)

### 3.1 Type Descriptor

```c
// z42_abi.h — versioned; new fields only appended

typedef struct Z42TypeDescriptor_v1 {
    uint32_t  abi_version;          // = 1
    uint32_t  flags;                // VALUE_TYPE | SEALED | ABSTRACT | TRACEABLE
    const char* module_name;        // e.g. "numz42"
    const char* type_name;          // e.g. "Tensor"
    size_t    instance_size;
    size_t    instance_align;

    // Lifecycle
    void*   (*alloc)(void);
    void    (*ctor)(void* self, const Z42Args* args);
    void    (*dtor)(void* self);
    void    (*dealloc)(void* self);
    void    (*retain)(void* self);     // default: atomic inc on rc field
    void    (*release)(void* self);    // default: atomic dec; on zero -> dtor + dealloc

    // Methods
    size_t                method_count;
    const Z42MethodDesc*  methods;

    // Fields (optional exposure)
    size_t                field_count;
    const Z42FieldDesc*   fields;

    // Trait implementations
    size_t                trait_impl_count;
    const Z42TraitImpl*   trait_impls;
} Z42TypeDescriptor_v1;

typedef struct Z42MethodDesc {
    const char* name;
    const char* signature;     // "(&Self, i64) -> Self" — parsed by compiler
    void*       fn_ptr;        // extern "C" shim, z42 calling convention
    uint32_t    flags;         // STATIC | VIRTUAL | OVERRIDE | CTOR
} Z42MethodDesc;

typedef struct Z42FieldDesc {
    const char* name;
    const char* type_name;     // "i64", "*const Self", etc.
    size_t      offset;
    uint32_t    flags;         // READONLY | INTERNAL
} Z42FieldDesc;
```

### 3.2 VM-Exposed API

```c
// Native lib calls these
Z42TypeRef z42_register_type(const Z42TypeDescriptor_v1* desc);
Z42Value   z42_invoke(Z42TypeRef ty, const char* method, Z42Value* args, size_t n);
Z42Value   z42_invoke_method(Z42Value receiver, const char* method, Z42Value* args, size_t n);
Z42TypeRef z42_resolve_type(const char* module, const char* name);
Z42Error   z42_last_error(void);
```

### 3.3 ABI Evolution Rules

Lessons from CPython's `PyTypeObject` evolution:

- `abi_version` is the first field; new versions only **append** fields
- VM reads descriptor by `abi_version`-aware size, never assumes layout
- All access through `z42_*` functions, never direct struct manipulation
- Major version bump = explicit break (acknowledged in semver)

### 3.4 Typical Users

- Direct C library bindings (sqlite3, ffmpeg, openssl)
- z42 bindings authored from non-Rust hosts
- Foundation under Tier 2/3 (everything reduces here)

---

## §4 Tier 2: Rust Registration API

VM is implemented in Rust. The Rust ecosystem (regex, serde, tokio, polars, reqwest, tracing, …) is the primary source of high-quality libraries to expose to z42. Tier 2 makes wrapping Rust crates mechanical.

### 4.1 Crate Layout (Planned)

```
z42-rs/
├── z42-abi/        ← Tier 1 in Rust types (FFI bindings; no_std-friendly)
├── z42-rs/         ← User-facing crate (re-exports + helpers)
└── z42-macros/     ← proc macros: Z42Type derive, methods attr, module! macro
```

### 4.2 User-Side Authoring

```rust
use z42::prelude::*;

#[derive(Z42Type)]
#[z42(module = "numz42", name = "Tensor")]
pub struct Tensor {
    #[z42(field, readonly)]
    rank: u32,
    shape: Vec<i64>,    // not exposed to z42
    data:  Vec<f64>,
}

#[z42::methods]
impl Tensor {
    #[z42(ctor)]
    pub fn new(shape: &[i64]) -> Self {
        Tensor { rank: shape.len() as u32,
                 shape: shape.to_vec(),
                 data:  vec![0.0; shape.iter().product::<i64>() as usize] }
    }

    pub fn ndim(&self) -> usize { self.rank as usize }

    pub fn dot(&self, other: &Tensor) -> Tensor { /* ... */ }
}

#[z42::trait_impl("z42.core::Display")]
impl Tensor {
    fn fmt(&self) -> z42::Str {
        format!("Tensor[rank={}]", self.rank).into()
    }
}

z42::module! {
    name:    "numz42",
    version: "0.1.0",
    types:   [Tensor],
}
```

### 4.3 Macro Responsibilities

| Macro | Generates |
|---|---|
| `#[derive(Z42Type)]` | static `Z42TypeDescriptor_v1`; `impl Z42Type for T`; field accessors |
| `#[z42::methods]` | `extern "C"` shim per method (catches panic → `Z42Error`) |
| `#[z42::trait_impl]` | `Z42TraitImpl` entry registered to descriptor's `trait_impls` |
| `z42::module!` | `#[ctor]` registration entry; emits `<module>.z42abi` manifest at build |

### 4.4 Type Mapping (Rust ↔ ABI)

| Rust type | ABI form | Notes |
|---|---|---|
| `i8..i64`, `u8..u64`, `f32`, `f64`, `bool` | same | direct |
| `&T` where `T: Z42Type` | `*const T` | borrow |
| `&mut T` | `*mut T` | exclusive borrow |
| `Box<T>` | `*mut T` (owned) | ownership transfer |
| `&[T]` (T blittable) | pinned slice (`ptr+len`) | zero-copy; see §6.3 |
| `&str` | pinned UTF-8 slice | zero-copy |
| `Self` / `impl Z42Type` | `Z42TypeRef` | |
| `Result<T, E>` | tagged union split at ABI | error path |
| `()` | void | |

### 4.5 Ecosystem Leverage

Wrapping a Rust crate becomes a thin shell:

```rust
#[derive(Z42Type)]
#[z42(module = "z42.regex", name = "Regex")]
pub struct Regex(::regex::Regex);

#[z42::methods]
impl Regex {
    #[z42(ctor)]
    pub fn new(pattern: &str) -> Result<Self, RegexError> {
        ::regex::Regex::new(pattern).map(Regex).map_err(Into::into)
    }
    pub fn is_match(&self, s: &str) -> bool { self.0.is_match(s) }
}
```

Expected package families:

- `z42-std-*` (official) — regex, serde, tokio, reqwest, tracing, polars
- `z42-ext-*` (community)

---

## §5 Tier 3: Source Generator (Compile-Time Bindings)

### 5.1 z42-Side Declaration

Two forms:

```z42
// Form A: implicit import (recommended)
import Tensor from "numz42";

// Form B: explicit declaration (when manifest unavailable, or for a curated subset)
[Native(lib = "numz42", type = "Tensor")]
extern class Tensor {
    fn new(shape: *const i64, n: usize) -> Tensor;
    fn ndim(&self) -> usize;
    fn dot(&self, other: &Tensor) -> Tensor;
}
```

### 5.2 Compile-Time Flow

```
Compile time (z42c)
  1. Parse [Native(lib=…)] / `import T from "lib"`
  2. Locate <lib>.z42abi (search: $LD_LIBRARY_PATH, $Z42_PATH, ./target/)
  3. Parse manifest → resolve full type descriptor
  4. Validate user declaration against manifest (signature match)
  5. Emit IR:
     - TypeDef Tensor with vtable [new, ndim, dot]
     - Call sites become `CallNativeVtable <slot>`
  6. Record link dependency (libnumz42.dylib)

Run time
  1. VM startup → dlopen all link dependencies
  2. Native lib's #[ctor] calls z42_register_type
  3. VM resolves vtable slots to function pointers
  4. User code → indirect call → extern "C" shim → real impl
```

### 5.3 Why Compile-Time Resolution

Inspired by C# 11+ `[LibraryImport]` (which replaced the old reflection-based `[DllImport]`):

| Aspect | Runtime (`dlopen`+`dlsym`) | Compile-time (Source Gen) |
|---|---|---|
| AOT compatibility | Poor | Native |
| First-call cost | dlsym + signature lookup | Zero |
| Signature mismatch | Crash at runtime | Compile error |
| Missing type | Crash at runtime | Compile error |
| ABI version mismatch | Runtime panic | Compile error |
| Debugger symbols | Often missing | Proper |

z42 commits to compile-time resolution from day one — no historical baggage.

---

## §6 Type System at the Boundary

### 6.1 Blittable Types

Allowed at `[Extern]` and native-method signatures:

| z42 | C ABI | Notes |
|---|---|---|
| `i8/i16/i32/i64`, `u8/u16/u32/u64` | `int*_t` / `uint*_t` | direct |
| `f32`, `f64` | `float`, `double` | direct |
| `bool` | `uint8_t` | 1 byte |
| `*const T`, `*mut T` | `T*` | raw pointer |
| `CStr` | `const char*` | borrowed, NUL-terminated |
| enum (integer repr) | `int32_t` | |
| `Option<*T>` | `T*` (NULL = None) | |
| struct `[Layout(C)]` (all blittable fields) | C struct | |
| static fn pointer | fn pointer | |
| Native-class ref (`*Self`) | `*mut T` | reference to native type instance |
| `ref T` 参数 | `*mut T` | 双向引用（spec `impl-ref-out-in-runtime` 落地后启用 zero-copy 路径；编译期签名验证已生效）|
| `out T` 参数 | `*mut T` | 单向输出（同上 + DefiniteAssignment）|
| `in T` 参数 | `*const T` | 只读引用，零拷贝（同上）|

**Disallowed**: `String`, `Array<T>` (use `pinned`), `Tuple`, closures, plain z42 class. Compiler enforces at signature check.

### 6.2 Struct Layout Control

```z42
[Layout(kind = Sequential, pack = 4)]
struct sockaddr_in {
    family: u16, port: u16, addr: u32, zero: [u8; 8],
}

[Layout(kind = Explicit, size = 16)]
struct Variant {
    [FieldOffset(0)]  tag:    u8,
    [FieldOffset(8)]  as_int: i64,
    [FieldOffset(8)]  as_ptr: *mut void,   // union
}

[Layout(kind = C)]   // = Rust #[repr(C)]
struct Point { x: f64, y: f64 }
```

### 6.3 Pinned/Fixed: Zero-Copy String / Array

`String` and `Array<T>` (T blittable) cannot directly cross FFI. Instead, the `pinned` block borrows the internal buffer pointer:

```z42
fn write_native(s: String) -> i32 {
    pinned p = s {                  // borrow s.buf as raw view
        unsafe { c_write(p.ptr, p.len) }
    }
    // p lifetime ends; s usable again
}

fn process_array(data: Array<u8>) {
    pinned p = data {
        unsafe { c_process(p.ptr, p.len) }
    }
}
```

**Semantics**:

- `pinned p = X { ... }` is a scope. Within: `X` is immutable (no append/resize/move); `p.ptr` and `p.len` form a raw view.
- Compiler emits `PinPtr` / `UnpinPtr` IR opcodes around the block.
- Under RC backend: pin is a no-op (no relocation possible).
- Under future moving GC backend: pin prevents relocation during compaction.
- Only blittable-element `Array<T>` and `String` (UTF-8) may be pinned (compile-time check).

**Comparison with C# `fixed`**: same intent. Narrower (z42 only on `String` / `Array`; C# accepts any blittable lvalue). Strictly typed: `p.ptr` is `*const u8` for `String` / `*const T` for `Array<T>`, never an opaque pointer.

**Runtime representation (C4 ✅ 2026-04-29)**: a pinned view is an internal
`Value::PinnedView { ptr: u64, len: u64, kind: PinSourceKind }` variant. The
ABI tag `Z42_VALUE_TAG_PINNED_VIEW = 8` is reserved in `z42_abi`. `String`
sources are supported today; `Array<u8>` pinning waits on a dedicated
byte-buffer variant in a follow-up spec. `view.ptr` and `view.len` go
through the standard `FieldGet` IR opcode; out-of-range field names are
an internal VM error (IR invariant violated — TypeChecker rejects unknown
field access before codegen).

### 6.4 Native Types as First-Class

The most important departure from C#. Native libraries register **complete z42 classes** via Tier 1/2:

```z42
let t = Tensor::new([3i64, 4, 5]);
print(t.rank);                  // field access
let t2 = t.dot(&t);             // method call
print(t.fmt());                 // native impl of z42.core::Display
```

Capabilities of native-defined types:

- ctor / dtor / instance + static methods
- Fields (readonly or read-write)
- Trait implementations (full participation in z42 dispatch)
- RC participation (`retain` / `release` / cycle traversal)

**Comparison**:

| Capability | Python C API | C# P/Invoke | z42 |
|---|---|---|---|
| Native function callable from script | ✓ | ✓ | ✓ |
| Native-defined complete type | ✓ (`PyTypeObject`) | ✗ (only opaque `IntPtr` wrappers) | ✓ |
| Type participates in `is`/`isinstance` checks | ✓ | ✗ | ✓ |
| Native trait/protocol implementation | ✓ (duck-typed) | ✗ | ✓ (vtable) |
| ABI stability | ⚠ (occasional breaks) | N/A | ✓ versioned |

This unlocks "numpy / torch / lxml" style libraries in z42: high-performance native cores exposed as ergonomic z42 types.

---

## §7 Calling Conventions

### 7.1 z42 → Native Method

Convention (z42 private ABI; not platform C ABI):

```
fn(self: *mut Self, arg1, arg2, …, argN) -> ret
                              ↑
              all blittable; lowered to platform C calling convention
```

- No implicit context arg (no GIL handle, no thread context)
- Errors: `Result<T, E>` split into `(tag: u8, payload: union)` at ABI; methods marked `nothrow` return value directly
- Panics caught at the `extern "C"` shim boundary, converted to `Z42Error` (or `Abort` per `[UnmanagedCallback(on_panic = Abort)]`)

VM dispatch:

| Backend | Dispatch |
|---|---|
| Interp | libffi (cached `ffi_cif` per signature) |
| JIT | emit direct `call` after vtable slot resolution |
| AOT | linker resolves at link time |

Hot path: indirect call only. **No marshal stub** (zero-marshal principle).

### 7.2 Native → z42 (Reverse Calls)

Native code creates / invokes z42 objects via Tier 1 API:

```c
Z42TypeRef ListType = z42_resolve_type("z42.core", "List");
Z42Value list = z42_invoke(ListType, "::new", NULL, 0);

Z42Value args[1] = { z42_int(42) };
z42_invoke_method(list, "push", args, 1);
```

z42-side panics surface as `Z42Error` via `z42_last_error()`.

### 7.3 Reverse Callbacks (z42 fn passed to native lib)

```z42
[UnmanagedCallback(conv = C, on_panic = Abort)]
fn my_compare(a: *const void, b: *const void) -> i32 {
    let pa = a as *const i32; let pb = b as *const i32;
    *pa - *pb
}

unsafe { qsort(arr.ptr, arr.len, 4, my_compare); }
```

**Constraints**: no captures, blittable args/return. Compiler emits a stable `extern "C"` entry.

Closure callbacks (with captures) require explicit trampoline:

```z42
let captured = 42;
let cb = make_trampoline(|x| x + captured);   // returns (fn_ptr, user_data)
register_callback(cb.fn_ptr, cb.user_data);
// cb owns trampoline memory; drop releases it
```

---

## §8 Memory Management

### 8.1 RC Model + Native Dealloc

```
z42 holds Tensor ref:
  let t  = Tensor::new(...);   rc = 1
  let t2 = t;                  retain → rc = 2
  drop(t2);                    release → rc = 1
  drop(t);                     release → rc = 0 → dtor → dealloc
```

Native lib provides `retain` / `release`. Default impl: `AtomicUsize::fetch_add` / `fetch_sub` on the instance's first field. Custom impls allowed for shared backing or pooling.

### 8.2 Cycle Collection

Native types may opt in by implementing `Z42Traceable` and setting `flags |= TRACEABLE`. The `#[derive(Z42Type)]` macro auto-generates traversal for fields whose types implement `Z42Traceable` — strictly better than CPython's manual `tp_traverse` (which is the source of most numpy refcount bugs).

### 8.3 Pin Protocol

Encoded by IR opcodes:

- `PinPtr <local>` — set pin flag on container (or register in pin set)
- `UnpinPtr <local>` — clear flag

Backends:

- RC: no-op (no relocation possible)
- Future moving GC: pinned objects skipped during compaction

### 8.4 Cross-Boundary Lifetime Rules

- `*const T` / `*mut T` from z42 → native: **borrowed for call duration only**; native MUST NOT store
- Owned native handles (`[NativeHandle]`): managed by z42 RC; dtor invoked at refcount zero
- z42 closure passed to native: requires trampoline (§7.3); native MUST release explicitly when no longer needed

---

## §9 Manifest Format (`<module>.z42abi`)

Machine-readable native library metadata, **published alongside the `.so` / `.dylib`** (analogous to Rust `.rmeta` or C# XML doc + metadata).

**Format**: JSON during development, FlatBuffer for production distribution (decision §12). The canonical v1 schema lives at [`docs/design/compiler/manifest-schema.json`](../compiler/manifest-schema.json) (JSON Schema Draft 2020-12).

```json
{
  "abi_version": 1,
  "module": "numz42",
  "version": "0.1.0",
  "library_name": "numz42",
  "types": [
    {
      "name": "Tensor",
      "size": 56, "align": 8,
      "flags": ["sealed"],
      "fields": [
        { "name": "rank", "type": "u32", "offset": 0, "readonly": true }
      ],
      "methods": [
        { "name": "new",  "kind": "ctor",
          "symbol": "__shim_Tensor_new",
          "params": [{"name":"shape","type":"&[i64]"}], "ret": "Self" },
        { "name": "ndim", "kind": "method",
          "symbol": "__shim_Tensor_ndim",
          "params": [], "ret": "usize" },
        { "name": "dot",  "kind": "method",
          "symbol": "__shim_Tensor_dot",
          "params": [{"name":"other","type":"&Self"}], "ret": "Self" }
      ],
      "trait_impls": [
        { "trait": "z42.core::Display",
          "methods": [{"name":"fmt","symbol":"__shim_Tensor_fmt"}] }
      ]
    }
  ]
}
```

**Generation**:

- Rust: `z42::module!` macro emits `OUT_DIR/<module>.z42abi`; build script copies to target dir
- C: `z42-bindgen-c <header.h> -o lib.z42abi` tool (analog of `cbindgen`)

**Consumption**: z42 compiler integrates a manifest reader; lazy-loads type descriptors only for imported types (manifest summary read first, full type resolved on use).

---

## §10 Roadmap

| Stage | Content | Depends on | Status |
|---|---|---|---|
| **C1** (`design-interop-interfaces`) | All Tier 1/2/3 public surfaces locked: C header + 3 Rust crates + manifest schema + 4 new IR opcodes (declared, trap on execution). _(Z0905–Z0910 originally reserved; retired 2026-05-11 — see `retire-z-codes`)._ | — | ✅ 2026-04-29 |
| **L2.M8** (`impl-tier1-c-abi`) | Tier 1 C ABI v1 + `z42_register_type` + libffi (Interp); fills `CallNative` runtime behaviour | C1 | ✅ 2026-04-29 |
| **L2.M9** (`impl-tier1-c-abi`) | Tier 1 PoC: handwritten `numz42-c` demo (Counter type, register + `CallNative` end-to-end) | M8 | ✅ 2026-04-29 (合并入 L2.M8 spec) |
| **L2.M10** | `z42-abi` / `z42-rs` / `z42-macros` crate skeleton | C1 | ✅ scaffold landed in C1 |
| **L2.M11** (`impl-tier2-rust-macros`) | `#[z42::methods]` + `module!` proc macro 实现（C3 主入口）+ `numz42-rs` PoC（Rust 版 Counter 端到端）；`#[derive(Z42Type)]` 与 `#[z42::trait_impl]` 仍 stub，留给 C5 与 source generator 联动设计 | M10 | ✅ 2026-04-29 |
| **L2.M12** (`impl-pinned-block` runtime) | `Value::PinnedView { ptr, len, kind }` + `PinPtr`/`UnpinPtr` runtime + `FieldGet` on PinnedView (.ptr / .len) + marshal 接 PinnedView | type system | ✅ 2026-04-29 (runtime) |
| **L2.M12.5** (`impl-pinned-syntax`) | z42 用户代码 `pinned p = s { ... }` syntax：lexer (Pinned keyword) + AST (PinnedStmt) + Parser + TypeChecker (source 类型校验 / 控制流限制 / PinnedView 字段) + IR Codegen (PinPtr/Body/UnpinPtr emit)；E0908a/b TypeCheck 错误码；其他 user-facing FFI syntax (`[Native(lib=,entry=)]` / `extern class T` / `import T from "lib"`) 留给后续 spec。 | C4 runtime | ✅ 2026-04-29 (syntax) |
| **L2.M13a** (`extend-native-attribute`) | 扩展 `[Native]` attribute 接受 `[Native(lib=, type=, entry=)]` Tier 1 形式；解析为 `Tier1NativeBinding`；IR Codegen 在 stub 中 emit `CallNativeInstr` 而非 `BuiltinInstr`。**z42 用户代码现在能直接调用 C2 注册的 native 函数**（test harness 预注册 native lib 留作后续 spec）。E0907 NativeAttributeMalformed。 | C5 syntax | ✅ 2026-04-29 |
| **L2.M13b** (`marshal-str-to-cstr`) | `Value::Str` 直接 marshal 到 `*const c_char`（NUL-terminated）：`marshal::Arena` 承载 CallNative 期间的 `CString` 临时；`(Value::Str, SigType::CStr/Ptr)` 分支构造借出；`CallNative` dispatch 接 arena；interior NUL 抛 `Std.InvalidMarshalException`（2026-05-11 retire-z-codes；曾用 Z0908(d)）。**z42 字符串可直接进 libc 风格 native 函数无需 pinned 块**。 | C6 / C7 | ✅ 2026-04-29 |
| **L2.M13c** (`class-level-native-shorthand`) | 类级 `[Native(lib=, type=)]` 共享默认：`Tier1NativeBinding` 改 nullable，方法级 partial 形式 + 类级 defaults 拼接出完整 binding；TypeChecker 校验 stitched 完整性；IrGen 用 stitched 结果 emit `CallNativeInstr`。**非平凡 native 库声明不再每方法重复 lib + type**。 | C6 + C8 | ✅ 2026-04-29 |
| **L2.M13d** (`byte-buffer-pin`) | Array<u8> pin support：`VmContext.pinned_owned_buffers` 副表持有 `Box<[u8]>`；`PinPtr` Array 路径扫元素 0..=255 → 拷贝到 Box → leak ptr；`UnpinPtr` 释放 Box；snapshot 语义。元素越界抛 `Std.InvalidMarshalException`（2026-05-11 retire-z-codes；曾用 Z0908(e)）。**z42 二进制数据可直接进 native FFI**。 | C4 | ✅ 2026-04-29 |
| **L2.M13e** (`manifest-reader-import`, C11a) | Lexer `Import` Phase1 keyword + contextual `from`；`import IDENT from "<lib>";` 顶层语法 → AST `NativeTypeImport` 收集到 `CompilationUnit.NativeImports`；`Z42.Project.NativeManifest.Read` 读取 `.z42abi` JSON（System.Text.Json，`abi_version == 1` + 必需字段轻量校验）；`NativeManifestException` + E0909 启用。**编译器现在能消费 manifest 数据通路；尚未合成 ClassDecl（留给 C11b）**。 | C9 | ✅ 2026-04-30 |
| **L2.M13f** (`synthesize-native-class`, C11b — Path B1) | `NativeImportSynthesizer` 编译期 pass（位 Parser 与 TypeChecker 之间）：每个 `import T from "lib";` → 找 manifest → 合成 `ClassDecl`（`IsSealed=true`, `Visibility=Internal`, `Fields=[]`, `ClassNativeDefaults` 复用 C9 stitching）注入 `cu.Classes`；`ManifestSignatureParser` 白名单（primitives + `Self` + `*mut/const Self`）；`INativeManifestLocator` 注入式（默认 `<sourceDir>/<lib>.z42abi` + `Z42_NATIVE_LIBS_PATH`）；E0916 启用。**用户现在写 `import Counter from "numz42";` 即得脚本可见 Counter 类，TypeChecker / IrGen / VM 走 C2–C10 既有路径，零新 ABI**。 | C11a | ✅ 2026-04-30 |
| **L2.M13g** (`extend-signature-whitelist`, C11e) | `ManifestSignatureParser` 白名单从 demo 级扩到能包真实 opaque-handle C 库：(1) `*const/*mut c_char` 在 param 位置 → `string`（走 C8 已有 arena marshal，零新 IR）；(2) `*const/*mut <Other>` 中 Other 是当前 CompilationUnit 已 import 的 native type 名 → `NamedType("Other")`（让 native 类之间互相引用作为方法签名）。`NativeImportSynthesizer.Run` 收集 `knownNativeTypes`（含所有 import.Name）并下传给 sig parser；E0916 错误信息分两类——**unknown-type**（`*mut Foo` 但 Foo 未 import，附 ``import Foo from "...";`` 提示）/ **unsupported-shape**（`Box<T>` / `[T; N]` 等结构未支持，附当前已 import 的 type 列表）。c_char return 留 C11f（owner ship 协议未定）。**用户现在能 import 真实 C 库（printf/regex_t/sqlite3 等）：c_char param + 跨 native 类指针 unblocked**。 | C11b | ✅ 2026-05-06 |
| **L2.M13** (manifest reader) | `.z42abi` manifest reader (schema already locked in C1) | M11 | ✅ 2026-04-30（合并入 C11a）|
| **L2.M14** (C5 second half) | Source gen: `import` auto-syncs manifest + compile-time validation; fills `CallNativeVtable` runtime | M13 |  |
| **L3.M15** | `z42-std-*` series start (regex / serde wrappers) | M14 |  |
| **L3.M16** | JIT/AOT direct vtable call emission (bypass libffi) | JIT backend |  |
| **L3.M17** | Cycle collector ↔ native `Z42Traceable` integration | cycle GC |  |

---

## §11 Current L1 Implementation (Legacy)

> **Status**: This section describes the L1 stub mechanism currently in use. It will be **superseded by Tier 1/2/3** during L2.M8–M14. New L1 builtins should still follow the conventions below until migration begins.

The L1 mechanism is a simplified interim FFI: a fixed `dispatch_table` in the Rust VM exposes a curated set of stdlib-backing functions, called from z42 via `extern` + `[Native("__name")]` annotations.

### 11.1 Declaration

```z42
namespace Std.IO;

public static class Console {
    [Native("__println")]
    public static extern void WriteLine(string value);
}

namespace Std.Math;

public static class Math {
    [Native("__math_sqrt")]
    public static extern double Sqrt(double x);
}
```

Compile-time rules:

- `extern` requires `[Native("__name")]`, else error `Z0903`
- `[Native]` requires `extern`, else error `Z0904`
- Body forbidden (`;` only)
- Name must exist in `dispatch_table`, else `Z0901`
- Param count must match, else `Z0902`

### 11.2 Naming Convention (`__<area>_<verb>[_<modifier>]`)

| Area | Domain | Examples |
|---|---|---|
| `str`     | string ops on `Std.String` | `__str_length`, `__str_char_at` |
| `char`    | `Std.Char` ops | `__char_to_upper`, `__char_is_whitespace` |
| `int32` / `int64` / `double` | primitive ops (parse / hash / equals / to_string) | `__int32_parse`, `__double_to_string` |
| `math`    | `Std.Math.Math` static methods | `__math_sqrt`, `__math_atan2` |
| `obj`     | universal object protocol | `__obj_get_type`, `__obj_hash_code` |
| `file`    | `Std.IO.File` static methods | `__file_read_text`, `__file_exists` |
| `env`     | environment / process | `__env_get`, `__env_args` |
| `time`    | clock / measurement | `__time_now_ms` |
| `process` | host process control | `__process_exit` |

New domain → add to this table + pick a short single-word identifier (no inner underscores).

**Legacy bare names** (do **not** add new ones): `__println`, `__print`, `__readline`, `__concat`, `__contains`, `__len`, `__to_str`, `__time_now_ms`, `__process_exit`. These predate the convention; retained for backward compatibility. Migrate opportunistically when the implementing module reorganizes.

### 11.3 Dispatch

z42 compiler emits `Builtin(native_id, args)` IR; VM `dispatch_table` maps id → Rust fn pointer; called directly without marshaling.

```rust
pub fn println(s: GcHandle<String>) { /* ... */ }
pub fn sqrt(x: f64) -> f64 { x.sqrt() }

pub static NATIVE_TABLE: &[(&str, usize, NativeFn)] = &[
    ("__println",   1, native_impl::println as NativeFn),
    ("__math_sqrt", 1, native_impl::sqrt    as NativeFn),
];
```

### 11.4 Type Mapping (Current)

| z42 | Rust | Notes |
|---|---|---|
| `int` / `long` | `i32` / `i64` | |
| `float` / `double` | `f32` / `f64` | |
| `bool` | `bool` | byte-sized |
| `char` | `u32` | Unicode code point |
| `string` | `GcHandle<String>` | GC ref |
| `T[]` / `List<T>` | `GcHandle<Array<T>>` | GC ref |
| value struct | `<struct>` by value | C-compatible layout |
| class instance | `GcHandle<Class>` | GC ref |

### 11.5 Migration Path

The full L1 → L2+ migration plan (M8–M14 phasing, deprecation timeline) is tracked in [`docs/roadmap.md`](../../roadmap.md) under the L2 interop section. Both L1 `[Native("__name")]` and L2+ `[Native(lib=,type=,entry=)]` styles coexist during the transition; the compiler accepts both.

---

## §11.5 Native Class Synthesis: Path B1 / B2 / C (decided 2026-04-30)

When the compiler synthesizes a script-visible `ClassDecl` from an `import T from "lib";` statement (spec C11b+), there are three architectural shapes for **who owns the instance memory layout**:

| Path | Owner | What native side provides | What VM side does |
|---|---|---|---|
| **A** — native-owned blittable | Native struct with `#[repr(C)]` | size / align / per-field offsets in manifest; `tp_alloc` / `tp_dealloc` | Treats instance as opaque ptr; `Z42TypeDescriptor_v1` carries layout |
| **B1** — handle-flavored (✅ C11b) | Native (opaque) | function callbacks only; native ctor returns `*mut Self`; methods take `*mut/const Self` first | VM stores opaque handle (Value-as-IntPtr); no script-visible fields |
| **B2** — VM-owned fields (⏭️ C11c) | VM | function callbacks; field access via new `z42_obj_get/set_field` ABI | Allocates Value::Object with named fields; native methods see them via callback |
| **C** — hybrid script-side `[Repr(C)]` (⏭️ C11d) | Script | layout declared by user in z42 `class` with `[Repr(C)]` attribute | Compiler emits matching descriptor; opens A-style perf for user-defined types |

**C11b ships B1 only.** Rationale:
- 95% of FFI use cases are wrapping opaque-handle libraries (sqlite, curl, openssl, regex_t) — B1 fits naturally
- B1 reuses C2–C10 ABI unchanged, zero new VM-side surface
- B2's "fields visible in z42 debugger / reflection" is desirable but not blocking
- C lets users opt into A-style blittability without polluting the default path

**Out of scope for C11b** (each becomes its own future spec):
- B2: VM-owned fields requires `z42_obj_get_field(handle, name)` + `z42_obj_set_field(handle, name, value)` exports plus an allocator hook
- A & C: blittable layout requires the synthesizer to read manifest `size` / `align` / per-field `offset` and emit a `Z42TypeDescriptor_v1` with field metadata, plus codegen must lower direct field access to offset arithmetic

**Signature whitelist (C11e extension, 2026-05-06)** — `ManifestSignatureParser` accepts:

| Shape | z42 mapping | Position | Notes |
|---|---|---|---|
| `void` / `i8..i64` / `u8..u64` / `f32` / `f64` / `bool` | `NamedType("<primitive>")` / `VoidType` | param + return | C11b |
| `Self` | `NamedType(<enclosing>)` | return | C11b |
| `*mut Self` / `*const Self` | implicit `this` receiver | first param | C11b |
| `*const c_char` / `*mut c_char` | `NamedType("string")` | **param only** | C11e — return 位置 C11f 未定 owner ship 协议；marshal 走 C8 既有 `(Value::Str, SigType::CStr)` arena 路径 |
| `*const <Other>` / `*mut <Other>` | `NamedType("Other")` | param + return | C11e — Other 必须是当前 CompilationUnit 已 `import` 的 native type 名（含 `selfTypeName`）；未 import 报 unknown-type E0916 |

**Out of scope for C11e** (each留给 C11f / C11g):
- `c_char` 返回值（who frees the C string? — owner ship 协议未定）
- `Array<T>` / `&[T]` / `[T; N]` 数组形态（数组语义 + 长度对协议待定）
- `Option<T>` / nullable 指针

The handle in B1 is **not stored as a script-visible z42 field**. It is the runtime Value of the variable holding the imported instance — i.e. `var c = new Counter()` ⇒ `c` *is* the IntPtr returned by `numz42_Counter_alloc`. Subsequent calls `c.inc()` are dispatched as `CallNativeInstr` with `c` passed as the first libffi argument (the receiver), exactly the pattern C2–C10 already exercise for hand-written `[Native(lib=, type=, entry=)]` declarations.

---

## §12 Open Decisions

| # | Question | Default leaning |
|---|---|---|
| 1 | Manifest format: JSON (dev) → FlatBuffer (prod), or single-format z42 binary (`.zpkg`-style)? | JSON dev / FlatBuffer prod; decide before M13 |
| 2 | `.z42abi` distribution: same dir as `.so` / embedded as ELF section / package registry? | Same dir initially; registry post-1.0 |
| 3 | `import T from "lib"` resolution: library name (filesystem) or package name (registry)? | Library name in L2; registry in L3 |
| 4 | Tier 1 ABI stability commitment: post-1.0 break-change rules? | TBD; defer until Tier 1 has external users |
| 5 | Cycle collection trait: opt-in (`#[derive(Z42Traceable)]`) or default-on for native types? | Opt-in (perf-conscious; matches Rust convention) |

---

## §13 Related Documents

- [philosophy.md](../philosophy.md) — embedding-first design principle
- [language-overview.md](../../reference/src/language/README.md) — `extern` / `[Native]` syntax (L1)
- [ir.md](../../internals/src/formats/ir.md) — `Builtin` instruction (L1) and future `CallNative` / `CallNativeVtable` opcodes
- [vm-architecture.md](../../internals/src/runtime/vm-architecture.md) — VM dispatch path
- compiler-architecture.md — type checker for FFI signatures
