# Object Protocol — VM Implementation

> **Status**: L1 ✅ ｜ ToString / Equals / GetHashCode / GetType 三路派发（Object vtable / String hardcoded / primitive value_to_str）

This document describes how the VM implements the universal "object" protocol
methods (`ToString`, `Equals`, `GetHashCode`, `GetType`) — the methods every
type implicitly inherits from `Std.Object`.

The doc focuses on **runtime dispatch paths**, not language-level semantics.
For the user-facing semantics see [`language-overview.md`](../../reference/src/language/README.md).
For the C# emit side see `compiler-architecture.md`.

---

## ToString

User code writes:

```z42
string s = obj.ToString();
string greeting = $"Hello, {name}!";   // implicit ToStr per interpolation hole
string concat   = "n=" + n;            // implicit ToStr on numeric operand
```

The IR uses one instruction:

```
ToStr dst, src    // emit `obj_to_string(src)` and store result in dst
```

`ToStr` is emitted at every site that needs a string from an arbitrary value —
string interpolation, `+` with a string operand, explicit `.ToString()` call
where the receiver is unknown at compile time, etc.

### Three dispatch paths (by `Value` variant)

```
                    ┌──────────────────────────────────┐
                    │   ToStr dst, src                  │
                    │   = exec_instr.rs::Instruction    │
                    └──────────────┬───────────────────┘
                                   │
                                   ▼
                    ┌──────────────────────────────────┐
                    │   dispatch.rs::obj_to_string     │
                    │   match Value variant            │
                    └─┬────────────┬─────────────────┬─┘
                      │            │                 │
        Value::Object │     Value::Str (special)     │  others (I64/F64/Bool/Char/...)
                      │            │                 │
                      ▼            ▼                 ▼
         ┌────────────────┐  ┌───────────────┐  ┌───────────────────┐
         │ vtable lookup  │  │ exec_instr.rs │  │ value_to_str       │
         │ "ToString"     │  │  VCall        │  │ (corelib::convert) │
         │ ↓ found?       │  │  hardcoded    │  │ Display-style fmt  │
         │ ├ yes: call    │  │  __str_to_   │  └───────────────────┘
         │ │   user fn    │  │   string     │
         │ └ no: fallback │  └───────────────┘
         │   __obj_to_str │
         │   builtin      │
         └────────────────┘
```

### Path 1 — `Value::Object`: vtable then builtin fallback

`dispatch.rs::obj_to_string` (entry point):

1. Take `type_desc` from the object's `Rc<RefCell<ScriptObject>>`.
2. Look up `"ToString"` in `type_desc.vtable_index` (O(1) HashMap).
3. **Hit** → resolve `vtable[slot].1` to a function name → look up in
   `module.func_index` → `exec_function(...)` → unwrap `ExecOutcome`:
   - `Returned(Some(Value::Str(s)))` → `s`
   - `Returned(Some(other))` → fall back to `value_to_str(&other)` (defensive)
   - `Returned(None)` → empty string (defensive; should never happen for
     a typed `string` return)
   - `Thrown(v)` → render as `<exception: {value_to_str(v)}>` (don't
     re-throw — `ToStr` is supposed to be infallible from the IR's view)
4. **Miss** → call `__obj_to_str` builtin (returns the simple class name like
   `Foo{...}`); used when a class doesn't override `ToString` (it inherits
   `Std.Object.ToString` which the stdlib script implements via this builtin).

### Path 2 — `Value::Str`: same unified path (with overload-suffix retry)

`Value::Str` follows the **same unified path** as Path 3 (other primitives) —
it's routed through `primitive_class_name` → `"Std.String"` → looked up as
`Std.String.<method>` IR function (extern stub or script body).

**Overload-suffix retry**: when `<method>` doesn't resolve directly (e.g.
`Equals` is overloaded as `Equals(object?)` + `Equals(string)` and IrGen
emits `Std.String.Equals$1`), the dispatcher retries with `Std.String.<method>$<arity>`.
This is required because the IR carries the unmangled method name when the
receiver type is statically `object` (`Std.Assert.Equal(object, object)` ⇒
`expected.Equals(actual)` ⇒ VCall with method = `"Equals"`), but IrGen always
mangles overloads at decl site.

> **Resolved asymmetry** (review2 §2.2 / C2 in review backlog, 2026-04-28):
> the legacy hardcoded `match method { "ToString" => "__str_to_string", ... }`
> block is gone from both `interp::exec_instr.rs::VCall` and
> `jit::helpers_object.rs::jit_vcall`. `Value::Str` now uses the same
> machinery as `Value::I64` / `Value::F64` / etc. The `__str_*` builtins are
> reached via the IR `extern` stubs that IrGen emits for each `[Native]`-
> attributed declaration in `Std.String`.

### Path 3 — Other primitives: `value_to_str` (basic Display)

For `Value::I64` / `Value::F64` / `Value::Bool` / `Value::Char` / `Value::Null`
/ `Value::Array(...)` etc., `obj_to_string` falls through to
`corelib::convert::value_to_str`, which is a simple `Display`-style formatter:

| Value | Output |
|-------|--------|
| `I64(n)` | `n.to_string()` |
| `F64(f)` | `f.to_string()` |
| `Bool(b)` | `"true"` / `"false"` |
| `Char(c)` | `c.to_string()` |
| `Null` | `"null"` |
| `Array(rc)` | `[elem0, elem1, ...]` (recursive) |
| `Object(rc)` (defensive — shouldn't reach here) | `ClassName{...}` |

Note: this path is the **fallback** for `ToStr` IR instruction. When user code
writes `n.ToString()` on a typed `int`, the call goes through the **interp
VCall** with receiver type → stdlib script `Std.Int32.ToString` (not directly
through `value_to_str`). `value_to_str` is only the implicit format for
`ToStr` instructions (string interpolation, `+`).

---

## Equals / GetHashCode / GetType

These follow analogous paths but currently lack a centralised dispatch helper
(unlike `obj_to_string`):

- **`Equals` / `GetHashCode`**: routed through `VCall`. For `Value::Object` →
  vtable lookup. For primitives → hardcoded builtins (`__int32_equals`,
  `__double_hash_code`, `__char_equals`, ...). For `Value::Str` → same
  hardcoded shortcut as `ToString` above.
- **`GetType`**: always routes through `__obj_get_type` builtin (returns a
  `Std.Type` object holding name + qualified-name).

**TODO**: consolidating these into a single `dispatch.rs::obj_method` helper
(parameterised by method name) is a candidate refactor — same pattern as
`obj_to_string`. Tracked for future cleanup.

---

## Call sites

| IR instruction | Where emitted | Calls |
|----------------|---------------|-------|
| `ToStr dst, src` | string interpolation `$"..."` per hole; `+` with string operand on the other side | `obj_to_string` |
| `VCall dst, recv, method, args` | every `obj.method(...)` user call | path-by-receiver dispatch (above) |

---

## Why three paths, not one

History:

1. **`Value::Object` vtable** (Path 1) is the canonical OOP dispatch — once
   `Std.Object.ToString` was rewritten as a script that delegates to
   `__obj_to_str`, the path naturally emerged from "look up vtable; if missing,
   invoke fallback builtin".

2. **`Value::Str` unified** (Path 2, post-C2) — strings used to have a
   hardcoded VCall switch (predating the script-first migration where
   strings had *all* methods as VM intrinsics). The C2 refactor (2026-04-28)
   removed the hardcoded block in favour of the same unified path other
   primitives use, gated only by overload-suffix retry. The intrinsic
   `__str_*` builtins are reached via the IR `extern` stub IrGen emits for
   each `[Native]` declaration in `Std.String` — same pattern as `Std.Int32`
   / `Std.Double`.

3. **`value_to_str` fallback** (Path 3) covers `ToStr` IR instructions on
   primitives where the receiver type is statically `Value::I64` etc. and the
   compiler emits implicit conversion. Once Path 2's asymmetry is fixed, this
   path remains valid as the "infallible Display" used by interpolation.

---

## Related

- [`stdlib.md`](../stdlib/overview.md) — script-first stdlib structure
- `compiler-architecture.md` — IR `ToStr` / `VCall` emission
- [`vm-architecture.md`](../../internals/src/runtime/vm-architecture.md) — `Value` enum + interpreter loop
- review2 §5.4 (this doc's origin) + §2.2 (Path 2 asymmetry tracking)
