# z42 Intermediate Representation (IR)

> **本文档边界**：IR 指令集（**权威**：opcode / 操作数语义 / 类型映射 / 异常表）。**Wire format**（二进制 section / 文件头 / 编码细节）见 [`zbc.md`](zbc.md)；**执行模式切换**（Interp / JIT / AOT 调度）见 [`execution-model.md`](../runtime/execution-model.md)。

## Design Goals

- Typed SSA form — every value has an explicit type
- Register-based — maps efficiently to JIT and native codegen
- Ownership annotations preserved — enables Rust VM to enforce safety
- Compact binary serialisation — `.zbc` bytecode files (see [zbc.md](zbc.md))
- Human-readable text form — `.zasm` for debugging

---

## Modules & Functions

```
module "hello"

fn @greet(%name: str) -> str {
entry:
    %lit = const.str "Hello, "
    %msg = call @str.concat(%lit, %name)
    ret %msg
}

fn @main() -> void {
entry:
    %s   = const.str "world"
    %msg = call @greet(%s)
    call @io.println(%msg)
    ret
}
```

---

## Type System in IR

| IR Type    | Description                        |
|------------|------------------------------------|
| `i8..i64`  | Signed integers                    |
| `u8..u64`  | Unsigned integers                  |
| `f32 f64`  | Floating point                     |
| `bool`     | Boolean                            |
| `char`     | Unicode scalar                     |
| `str`      | Immutable UTF-8 string             |
| `ptr<T>`   | Raw pointer (unsafe)               |
| `ref<T>`   | Immutable borrow                   |
| `refmut<T>`| Mutable borrow                     |
| `own<T>`   | Owned heap value                   |
| `void`     | No value                           |

---

## Instruction Set (draft)

### Constants
```
%r = const.i32  <value>
%r = const.f64  <value>
%r = const.bool true|false
%r = const.char '<char>'
%r = const.str  "<utf8>"
%r = const.null <type>
```

### Arithmetic & Logic
```
%r = add  <type> %a, %b
%r = sub  <type> %a, %b
%r = mul  <type> %a, %b
%r = div  <type> %a, %b
%r = rem  <type> %a, %b
%r = neg  <type> %a
%r = and  bool  %a, %b       ; logical AND
%r = or   bool  %a, %b       ; logical OR
%r = not  bool  %a           ; logical NOT
%r = bit_and  i32|i64  %a, %b
%r = bit_or   i32|i64  %a, %b
%r = bit_xor  i32|i64  %a, %b
%r = bit_not  i32|i64  %a
%r = shl      i32|i64  %a, %b
%r = shr      i32|i64  %a, %b
```

### Comparison
```
%r = eq  <type> %a, %b    -> bool
%r = ne  <type> %a, %b    -> bool
%r = lt  <type> %a, %b    -> bool
%r = le  <type> %a, %b    -> bool
%r = gt  <type> %a, %b    -> bool
%r = ge  <type> %a, %b    -> bool
```

### Numeric Cast (spec fix-numeric-cast-lowering, 2026-05-13)

`Convert` lowers an explicit user-level cast (e.g. `(long)d`) to a runtime
type conversion. Target type rides on the destination register's static
tag; source type is read at runtime from the source `Value` variant.

```
%r = convert  %src       ; %r has the target tag (e.g. i64); VM reads %src's
                         ; Value variant (F64 / I64 / Char) to pick conversion
```

**Opcode**: `0xB1` (slot in the 0xB0–0xBF generic-runtime range)
**Encoding**: `op(1) + dst_tag(1) + dst_reg(u16) + src_reg(u16)`
**zbc version**: 1.5+

**Conversion matrix (legal pairs, VM-supported)**:

| from → to | semantics |
|---|---|
| `f64 → i*/u*` | Rust `as iN` saturating + truncate toward zero; NaN → 0 |
| `f64 → char` | scalar value validation (surrogate / >U+10FFFF → InvalidCastException) |
| `i64 → f32/f64` | widening (precision may be lost beyond 2^53) |
| `i64 → i8/i16/i32` | low-bits + sign extend (matches C# `(int)long_val`) |
| `i64 → u8/u16/u32` | low-bits + zero extend |
| `char → i*/u*` | Unicode scalar value (0..U+10FFFF, skipping surrogates) |
| `char → f64` | scalar value as double |
| `i64 → char` | scalar value validation; surrogate / >U+10FFFF → exception |

**Rejected at TypeCheck (E0424 IllegalCast)**: `bool ↔ numeric/char/string`, `string ↔ numeric/char`. Users go through conditional expressions or `Parse` / `ToString` instead.

**Identity casts** (e.g. `(int)int_val`) are elided by Codegen and don't emit `Convert` — register flows through unchanged.

**Object / Unknown source (unbox)**: Codegen still emits `Convert`; VM resolves at runtime based on dynamic `Value` variant. Preserves the existing stdlib `(long)object` pattern. This is the **unboxing** path for `object → primitive` (装箱模型见 [struct-value-semantics.md](../runtime/struct-value-semantics.md)): numeric/char targets convert from the boxed scalar; **`bool` unboxes via an identity match** (`(Value::Bool, T_BOOL)`) since `bool` has no numeric arm; a tag mismatch or `Null` source throws `InvalidCastException`. Boxing (`primitive → object`) 走 `__box_prim` **BuiltinInstr**（复用既有 builtin opcode，非新 IR 指令），且只覆盖**整数与 enum**——`bool`/`char`/`float`/`double`/`string` 各有自己的 `Value` 变体，不进盒。

### Control Flow
```
br <label>
br.cond %cond, <true_label>, <false_label>
ret
ret %value
throw %val          # terminate block by throwing %val; handled by exception table
```

### Exception Handling (Phase 1)

Functions may carry an exception table (list of `ExceptionEntry`). Each entry:

| Field | Description |
|-------|-------------|
| `try_start` | Label of first block inside the try region |
| `try_end` | Label of first block **after** the try region (exclusive) |
| `catch_label` | Label of catch handler block |
| `catch_type` | Optional exception type string (Phase 1: ignored, catches all) |
| `catch_reg` | Register that receives the thrown value on handler entry |

`throw %val` terminates the block. The VM searches the exception table for an entry whose
`[try_start, try_end)` block range covers the current block. If found, it jumps to
`catch_label` and pre-loads `catch_reg` with the thrown value. If not found, the exception
propagates to the caller.

JSON wire format:
```json
{"op": "throw", "reg": 3}
```

Exception table row:
```json
{"try_start": "try_start_0", "try_end": "try_end_1", "catch_label": "catch_start_2", "catch_type": "Exception", "catch_reg": 4}
```

### Calls
```
%r = call @<fn>(%arg0, %arg1, ...)
%r = call.virt %vtable_slot(%receiver, %arg0, ...)
%r = call.async @<fn>(%args...)   -> task<T>
     await %task                  -> T
%r = builtin "<name>"(%arg0, ...)
```

`builtin` invokes a VM-native built-in by name (e.g. `"__println"`, `"__list_add"`).
Built-ins are registered in the VM's dispatch table and bypass the normal function call mechanism.
They are used inside stdlib stub functions and for pseudo-class operations on `Array`/`Map` values
(List/Dictionary) that cannot be dispatched via `v_call`.

**Stdlib call chain** — user code emits `call` to a fully-qualified stdlib function
(e.g. `z42.io.Console.WriteLine`); that stub function contains a `builtin` instruction
which invokes the VM-native implementation (e.g. `__println`).  The compiler resolves
stdlib call sites at build time via `StdlibCallIndex` and emits `call` instead of `builtin`
so that the VM can correctly load and link the stdlib module.

**Pseudo-class instance methods** — `List<T>` and `Dictionary<K,V>` are backed by `Array`
and `Map` VM values, not by real class instances. The compiler emits `builtin` directly for
their instance methods (e.g. `list.Add(x)` → `builtin "__list_add"`) because `v_call` cannot
dispatch on non-object values.

**`params` variadic arguments** (`add-params-varargs`) — no new IR instruction and no new
zbc opcode; the VM is entirely unaware of `params`. It is a pure frontend lowering in the
compiler's Codegen stage: an *expanded-form* call site (`Sum(1, 2, 3)` against
`int Sum(params int[] values)`) is emitted as an array literal built from the trailing
arguments (`new int[3]` + per-element stores) followed by an ordinary `call`/`call.virt`
with that array as the last argument — identical to a hand-written
`Sum(new int[] { 1, 2, 3 })` *normal-form* call. Cross-package calls resolve
normal/expanded form using a `paramsFrom` marker carried in the callee's TSIG record
(see [zpkg.md](zpkg.md)); this only affects which parameter the compiler treats as the
trailing array when deciding how to lower a call, not IR/zbc semantics.

JSON wire format:
```json
{"op": "call",    "dst": 2, "func": "z42.io.Console.WriteLine", "args": [1]}
{"op": "builtin", "dst": 3, "name": "__list_add",               "args": [0, 1]}
```

### Native Interop (C1 scaffold)

```
%r = call.native     "<module>::<type>::<symbol>"(%arg0, ...)
%r = call.native.vt  %recv.<vtable_slot>(%arg0, ...)
%p = pin    %src                  # borrow String/Array buffer for FFI
     unpin  %pinned               # release pin
```

Four opcodes lock down the binary format for the L2+ three-tier ABI (see
[native-abi.md](../runtime/native-abi.md)). Each is **declared** in C1 with no runtime
behaviour; subsequent specs (C2, C4, C5) wire up dispatch.

| Opcode | Byte | Operands | Filled by spec |
|--------|------|----------|----------------|
| `call.native` | `0x53` | `dst`, module:str, type:str, symbol:str, args | C2 (`impl-tier1-c-abi`) |
| `call.native.vt` | `0x54` | `dst`, recv:reg, vtable_slot:u16, args | C5 (`impl-source-generator`) |
| `pin` | `0x90` | `dst`, src:reg | C4 ✅ |
| `unpin` | `0x91` | pinned:reg (no dst) | C4 ✅ |

`call.native` is the direct-symbol path used to call functions registered
through `z42_register_type` (Tier 1 C ABI). `call.native.vt` is the
vtable-indexed path: the source generator picks `vtable_slot` at compile
time so no name lookup happens at runtime, matching C# 11+
`[LibraryImport]` semantics.

> **z42c codegen — two `[Native]` forms (port-z42c-typed-native-call):** an `extern`
> method's stub body is emitted from its `[Native]` attribute. The **typed** named
> form `[Native(lib="L", type="T", entry="E")]` lowers to `call.native L::T::E(...)`
> (this opcode). The **positional / `entry=`-only** form `[Native("__name")]` lowers
> to `builtin __name(...)` (`0x51`), resolved via the VM's `dispatch_table` /
> `ext_builtins` registry. (The self-hosted z42c originally only carried the builtin
> path; the typed path was restored after the C# compiler's removal exposed the gap.)

`pin` borrows the raw buffer of a `String` (and, in a follow-up spec,
blittable-element byte arrays) for FFI use; `unpin` returns it to normal
use. Pinned regions cannot be mutated or relocated until unpinned. The
`pinned` block syntax in user code (introduced in spec C5) lowers to a
`pin` … `unpin` pair around the FFI call.

**Runtime semantics (C4)**: `pin` constructs a `Value::PinnedView { ptr, len, kind }`
from a `Value::Str` source — `ptr` is the raw `String` buffer address,
`len` is the byte count, `kind = PinSourceKind::Str`. Field access
`view.ptr` / `view.len` projects via `FieldGet` to `Value::I64`.
`unpin` is a no-op on the RC backend (no relocation possible) but
validates that the operand is a PinnedView (internal VM error
otherwise — IR invariant violated) so a moving GC backend can later
use the same opcode for pin-set deregistration. `Array<u8>` pinning is
reserved for a follow-up spec that introduces a dedicated byte-buffer
Value variant.

> User-facing marshal failures (`pin` on an unsupported source, NUL in a C
> string, etc.) became typed z42 exceptions (`Std.InvalidMarshalException`)
> in 2026-05-11 retire-z-codes; only IR-shape invariants remain as Rust
> `anyhow!` traps.

VM behaviour for the still-trapping opcodes (`call.native.vt`): the
interpreter raises a clean error ("`<opcode>` not yet implemented
(see spec C5)") if executed. The JIT translator refuses to
compile a function that contains any of the four FFI opcodes (lands in
L3.M16).

JSON wire format:
```json
{"op": "call_native",        "dst": 2, "module": "numz42", "type_name": "Tensor",
                             "symbol": "__shim_Tensor_dot", "args": [0, 1]}
{"op": "call_native_vtable", "dst": 3, "recv": 0, "vtable_slot": 7, "args": [1]}
{"op": "pin_ptr",            "dst": 4, "src": 1}
{"op": "unpin_ptr",          "pinned": 4}
```

### String Operations
```
%r = str.concat %a, %b          # concatenate two strings
%r = to_str %src                 # convert any value to its string representation
```

`to_str` converts a value to string. For `Value::Object`, it dispatches `ToString()` via the
vtable (same as `v_call %obj.ToString`). If no `ToString` override exists, it falls back to
the `__obj_to_str` builtin (unqualified type name). All other value types use the built-in
formatting directly (e.g. `I64` → decimal string, `Bool` → `"true"/"false"`).

JSON wire format:
```json
{"op": "str_concat", "dst": 3, "a": 1, "b": 2}
{"op": "to_str",     "dst": 4, "src": 1}
```

### Memory / Ownership
```
%r = alloc  <type>              # heap allocation, returns own<T>
     drop   %owned              # explicit drop (usually inserted by compiler)
%r = borrow %owned              # ref<T>
%r = borrow.mut %owned          # refmut<T>
%r = deref  %ref                # load through borrow
     store  %refmut, %value     # store through mutable borrow
```

### Arrays

Five instructions cover the whole surface of `T[]` (`Instruction::ArrayNew` / `ArrayNewLit` /
`ArrayGet` / `ArraySet` / `ArrayLen`, `metadata/bytecode/instruction.rs`):

| IR 指令 | 操作 |
|---------|------|
| `array_new { dst, size, element_type, … }` | 分配 `size` 个元素的零值数组 |
| `array_new_lit { dst, elems: [reg...], element_type, … }` | 字面量数组 |
| `array_get { dst, arr, idx }` | 读元素，越界 abort |
| `array_set { arr, idx, val }` | 写元素，越界 abort |
| `array_len { dst, arr }` | 长度（i32） |

两条 new 指令携带**元素类型 FQ 名**（`element_type`，add-reflection-array-element-type）——
数组在运行期不擦除元素类型，`arr.GetType().GetElementType()` / `FullName` 才能给出
`"Std.Int32"` / `"Std.Int32[]"`。泛型值 struct 上这里有个刻意的不对称：TypeDesc 只按**擦除**
基名（`Kv`）注册一份，而 `element_type` 刻意带**非擦除**名（`Kv<string,int>`），
故 `try_struct_backed` 查布局时先 `split('<')` 剥回基名，同时把完整名存进 `ArrayObj`
（`interp/exec_array.rs`）。`array_new` 另带 `stack_alloc`（逃逸分析，zbc 1.29）与
`type_param_kind` / `type_param_index`（元素是型参时运行期解析出真实零值，zbc 1.37）。

```
# new int[] { 1, 2, 3 }
%r0  = const.i32 1
%r1  = const.i32 2
%r2  = const.i32 3
%arr = array_new_lit [%r0, %r1, %r2]

# arr[i] += 1（复合赋值）
%v   = array_get %arr, %i
%one = const.i32 1
%v2  = add %v, %one
       array_set %arr, %i, %v2
```

**运行期表示**：`Value::Array(GcRef<ArrayObj>)`（tag 6），载荷 `ArrayObj` = 元素类型名
（`Arc<str>`）+ 元素 `Vec<Value>`，`Deref` 到元素向量故元素访问零开销。栈分配逃逸分析命中时
receiver 变成 `Value::StackArray { idx, frame_id }`，同四条指令走 `ctx.stack_arena` 解析。
布局细节见 [对象与值表示 ABI](../runtime/object-abi.md)。

**越界不是异常**：`array_get` / `array_set` 越界直接
`bail!("array index {} out of bounds (len={})")`（`interp/exec_array.rs:196`）—— VM abort，
用户侧 `catch` 接不住。面向用户的规则见
[数组](../../../reference/src/language/arrays.md)。

**交错数组**：`T[][]` 无专门指令——外层就是元素类型为 `T[]` 的普通数组，逐层 `array_get` 即可。
多维 `T[,]` 没有 IR 支持，也没有对应的类型语法。

### Objects (Phase 1 — class instances)
```
%r = obj_new  <ClassName> ctor=<CtorName>(%arg0, %arg1, ...)
                                                # allocate + call overload-resolved ctor
%r = typeof   <TypeName><TypeArg0, ...>         # reflection Std.Type (generic args 可选)
%r = field_get %obj, <field>                    # load a field
     field_set %obj, <field>, %value            # store a field
```

JSON wire format (tag = `"op"`):
```json
{"op": "obj_new",     "dst": 5, "class_name": "Demo.Point", "ctor_name": "Demo.Point.Point$2", "args": [1, 2]}
{"op": "typeof",      "dst": 5, "type_name": "Demo.Box", "type_args": ["int"]}
{"op": "field_get",   "dst": 6, "obj": 5, "field_name": "X"}
{"op": "field_set",             "obj": 5, "field_name": "X", "val": 3}
{"op": "v_call",      "dst": 7, "obj": 5, "method": "Area", "args": []}
{"op": "is_instance", "dst": 8, "obj": 5, "class_name": "Demo.Shape"}
{"op": "as_cast",     "dst": 9, "obj": 5, "class_name": "Demo.Shape"}
```

`typeof`（opcode `0x73`，zbc 1.18，add-reflection-generic-type-definition）求值为一个
`Std.Type` 反射对象。`type_name` 是反射类型的 FQ 名（泛型取**定义**名）；`type_args`
是实例化 arg 的 FQ 名列表（`typeof(Box<int>)` → `["int"]`，非泛型为空），结构化编码
（镜像 `obj_new` 的 type_args）。非空 args 标记为**构造型**泛型——运行期把解析后的
`Std.Type[]` 挂到结果对象的 `__typeArgs` 槽，背书 `GetGenericArguments()` /
`IsGenericTypeDefinition` / `GetGenericTypeDefinition()`。统一所有 `typeof(...)`，取代旧
`__typeof` builtin（type args 是编译期类型元数据，不 materialize 成运行期 `const_str` 值）。

`obj_new` 调用 **TypeChecker 编译期 overload-resolve 选定的具体 ctor 函数**
（`ctor_name` 字段直查），与 `call` 指令对齐 — VM 不做名字推断。命名约定：

- 单 ctor：`{ClassName}.{SimpleName}`（无 suffix），如 `Demo.Point.Point`
- 重载：`{ClassName}.{SimpleName}${N}`（1-based 声明序），如 `Demo.Point.Point$2`
- 类无显式 ctor：`ctor_name` 取单 ctor 形式占位，VM lookup 失败时跳过 ctor
  调用（默认无参 ctor 语义）

`obj_new` 分配 `ScriptObject`（slot-indexed 字段）后用 `[this, ...args]`
调用。0.7 起 `ctor_name` 字段必备，0.6 及更早 zbc 不再被支持
（按 [`../../agent/rules/philosophy.md "不为旧版本提供兼容"`](../../../agent/rules/philosophy.md#不为旧版本提供兼容2026-04-26-强化)）。

0.9（2026-05-07，add-default-generic-typeparam）起，`obj_new` 携带 **resolved
type-args 列表**（如 `new Foo<int>()` → `["int"]`），VM 在分配实例后写入
`ScriptObject.type_args` 字段，供后续 `default_of` 等运行时类型查询使用。
非泛型类传空列表，零开销。interp 与 JIT 路径都 propagate（JIT 路径 by
`expand-jit-type-args` 2026-05-07 同日补齐）。

#### `default_of` (D-8b-3 Phase 2)

```
%r = default_of $<param_index>      # default value of this.type_args[idx]
```

JSON: `{"op": "default_of", "dst": 5, "param_index": 0}`

仅在 generic class 的 instance method / ctor body 内由 IrGen 为
`default(T)` 表达式 emit。运行时读 `frame.regs[0]` (this) →
`ScriptObject.type_args[param_index]` → `default_value_for(tag)` 写 dst。
非 Object reg 0 / OOB index / 空 type_args → graceful 退化为
`Value::Null`（method-level type-param / free generic / static method on
generic class 的当前路径都走该退化）。

`field_get` / `field_set` use pre-computed slot indices from the `TypeDesc` registry (O(1) per
access). Virtual fields are also dispatched by `field_get` for built-in primitive types:
- `Value::Str` — `"Length"` returns `I64` (Unicode scalar count, not byte length)
- `Value::Array` — `"Length"` / `"Count"` returns `I64`
- `Value::Map` — `"Length"` / `"Count"` returns `I64`

`v_call` dispatches via the pre-computed vtable in `TypeDesc` (O(1)). The vtable is flattened
at load time: base class methods appear first, derived overrides replace the corresponding slot.

For primitive types that lack a `TypeDesc`, `v_call` dispatches to the built-in name table:
- `Value::Str` — `ToString` → `__str_to_string`, `Equals` → `__str_equals`, `GetHashCode` → `__str_hash_code`

This allows object-protocol methods (`ToString`/`Equals`/`GetHashCode`) to work uniformly on
both class instances and primitive string values.

`is_instance` returns `bool` — true if the object's runtime class is `class_name` or a subclass.
The check walks `TypeDesc.base_name` links in the pre-built registry (O(depth)).

`as_cast` returns the object unchanged if it matches `class_name` (or a subclass), else `null`.

#### ScriptObject / TypeDesc (VM internals)

At load time `build_type_registry` topologically sorts all `ClassDesc` entries and for each
class pre-computes a `TypeDesc`:

| Field | Type | Description |
|-------|------|-------------|
| `name` | `String` | Fully-qualified class name (e.g. `"Demo.Circle"`) |
| `base_name` | `Option<String>` | Direct base class name, or `None` |
| `fields` | `Vec<FieldSlot>` | Flat field list (base fields first) |
| `field_index` | `HashMap<String, usize>` | Name → slot index (O(1) lookup) |
| `vtable` | `Vec<(String, String)>` | `(method_name, qualified_func_name)` |
| `vtable_index` | `HashMap<String, usize>` | Method name → vtable slot (O(1) lookup) |

Each `ScriptObject` instance holds an `Arc<TypeDesc>` (shared across all instances of the same
class), a `Vec<Value>` of field slots, and a `NativeData` discriminant for built-in backing
stores (`NativeData::StringBuilder(String)` for `Std.Text.StringBuilder`).

### Static Fields
```
%r = static_get <QualifiedField>      # load module-level static field by "ClassName.fieldName"
     static_set <QualifiedField>, %v  # store module-level static field
```

JSON wire format:
```json
{"op": "static_get", "dst": 3, "field": "Demo.Counter.count"}
{"op": "static_set", "field": "Demo.Counter.count", "val": 3}
```

A module with static fields may contain a zero-argument function named
`<namespace>.__static_init__` (e.g. `"Demo.__static_init__"`). The VM calls
it once, before the entry point, to initialize all static fields.

### Structs & Tuples
```
%r = struct.new <TypeName> { field0: %v0, field1: %v1 }
%r = struct.get %s, <field>
     struct.set %s, <field>, %value
%r = tuple.new (%v0, %v1, ...)
%r = tuple.get %t, <index>
```

### Enums / Variants
```
%r = variant.new <EnumName>::<Variant>(%payload)
%r = variant.tag %v             -> u32   # discriminant
%r = variant.cast %v, <Variant> -> <PayloadType>?
```

### Execution Mode Hint
```
exec.mode interp | jit | aot    # module-level directive
```

---

### Closures (草案，L3 落地)

闭包 / lambda / 函数引用相关的 IR 指令。用户面捕获语义见[闭包与捕获语义](../../../reference/src/language/closures.md)，运行期表示与栈分配见[逃逸分析](../runtime/escape-analysis.md)。
opcode 编号在 `impl-closure-l3` 变更落地时分配。

```
# 创建闭包：栈分配（档 A，env 在调用方栈帧）
%r = mkclos.stack <env_layout>, <fn_ref>

# 创建闭包：堆分配（档 C，env 进 RC/GC 堆，闭包对象是胖指针）
%r = mkclos.heap <env_layout>, <fn_ref>

# 调用闭包：档 A/B 直接 call；档 C 走 vtable 间接调用
%r = callclos %closure (%arg1, %arg2, ...) -> <RetType>

# Ref<T> 包装类型（R14：闭包内修改外部值类型的逃生口）
%r = mkref %value         -> Ref<T>
%v = loadref %ref         -> T
       storeref %ref, %value
```

档 B（单态化）不产生新 IR——闭包字面量在 IR 阶段直接 inline 到泛型函数体内。

无捕获 lambda 在 IR 层降级为 `loadfn <fn_ref>`（与函数引用同源），不产生 closure 对象。

> **待回填**：档 C 闭包 env 在 RC vs GC 内存模型下的具体编码（refcount 字段位置 / 扫描根注册）；弱引用支持 IR 指令——这些依赖 z42 内存模型决议。

---

## Binary Format

`.zbc` 和 `.zpkg` 二进制格式的完整规范见 [zbc.md](zbc.md)。

## Rust 内存表示：热/冷装箱（slim-instruction-enum, 2026-06-11）

> 这是 **VM 内部内存布局** 决策，与 zbc/zpkg wire format **完全解耦**——
> 二进制字节序列不变，无版本 bump。

VM 的 `metadata::Instruction` 是一个枚举，`Function.body` 是其 `Box<[Instruction]>`
热数组，interp / JIT 顺序迭代。枚举 size = 最大变体 size，所以一个臃肿变体会拖胖
**每一条** 指令槽位（含 `Add`/`Copy` 这类纯寄存器热指令），劣化 cache 局部性。

**策略**：凡 **携带 `String`**（name-bearing，冷路径）的变体，把 payload 移入一个
`<Variant>Insn` struct，变体改为 newtype `Variant(Box<XxxInsn>)`；纯寄存器/小标量的
**热变体保持 inline**，dispatch 热路径不增一次指针解引用。

- **装箱的 16 个冷变体**：`Call` `Builtin` `LoadFn` `LoadFnCached` `MkClos` `ObjNew`
  `Typeof` `FieldGet` `FieldSet` `VCall` `IsInstance` `AsCast` `StaticGet` `StaticSet`
  `CallNative` `LoadFieldAddr`（payload struct 见 `bytecode.rs`）。
- **保持 inline**：所有算术/比较/位运算/常量/数组存取/地址加载（`LoadLocalAddr` 等）
  /`Convert`/`DefaultOf`/`PinPtr`/`UnpinPtr`，以及无 `String` 的 call 类
  `CallIndirect` / `CallNativeVtable`（它们的 inline payload 仍 ≤ 枚举上限）。
- **效果**：`size_of::<Instruction>()` 从 **96 B**（旧最大变体 `CallNative`，三个
  inline `String`）降到 **32 B**（现由 `CallIndirect` / `CallNativeVtable` 这两个无
  String 但带 `Box<[Reg]>` 的变体决定）。`metadata::bytecode_tests::instruction_size_is_slim`
  静态断言 ≤ 32 B 守门。

**JSON wire format 不变**：枚举是 internally-tagged（`#[serde(tag = "op")]`），
newtype 变体的内层 struct 字段会被 serde **摊平进 tag 对象**，故
`Call(Box<CallInsn>)` 仍序列化为 `{"op":"call", dst, func, args}`——与装箱前逐字符
相同（`bytecode_tests` 的 round-trip 单测守门）。

### Deferred / Future Work

#### slim-terminator-future: 装箱 `Terminator` 的 String label

- **来源**：slim-instruction-enum（2026-06-11）
- **触发原因**：`Terminator`（`Br { label }` / `BrCond { …labels }`）带 `String`，
  但它是 **per-block**（每个 basic block 末尾一个），不是 per-instruction 热数组，
  装箱收益远低于 `Instruction`。
- **前置依赖**：无。
- **触发条件**：若后续 profiling 显示 block 元数据内存成为瓶颈，或 `Terminator`
  新增更多 String-carrying 变体时回来评估。
- **当前 workaround**：无（保持 inline struct 变体）。

#### slim-instruction-stringid: `String → StringId` 收敛（E2.P3，正交后续）

- **来源**：review.md E2.P3。
- **触发原因**：本变更只调整装箱布局，未改 `String` 表示本身；把 name 字段换成
  intern 过的 `StringId` 可进一步缩小 payload struct 并去重。
- **前置依赖**：StringId intern 表贯通 zbc reader → metadata。
- **触发条件**：name 重复内存占用或字符串比较成为热点时。
- **当前 workaround**：payload struct 持有 owned `String`。
