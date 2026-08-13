# Proposal: unify-value-types Phase 2 —— R3 装箱统一（基元装箱对齐 C# 堆对象 + 引用身份）

> 伞程序 `unify-value-types` 的 **Phase 2**（R3）。Phase 1（编译器轨：消灭 Z42PrimType，PR #182/#185 已合）
> 落地后，runtime 装箱是**两套不对称**的实现——本 change 收敛成单一「堆 ScriptObject + 引用身份」模型，
> 对齐 C#。**纯 runtime（`src/runtime/`），编译器发射不变，大概率零格式 bump。**

## 背景：当前两套装箱（不对称）

| | 基元装箱 | struct 装箱（P4b 落地） |
|---|---|---|
| Value 变体 | `Value::Boxed(Box<BoxedPrim{class, inner}>)` | `Value::BoxedStruct(GcRef<ScriptObject>)` |
| 分配 | 轻量堆 `Box`（**非 GC 管理**） | GC 管理 `ScriptObject` |
| 引用身份 | **无**（每次 `__box_prim` 新 Box） | **有**（C# 语义：`object b=a` 别名同盒、反射 SetValue 写穿） |
| 装箱 builtin | `__box_prim(裸值, 类名)` → `Value::Boxed` | `__box_struct(structHandle)` → `Value::BoxedStruct` |

不对称的代价：反射（GetValue/SetValue/GetType）、`value_to_str`、GC visit、equality、convert 等
**~20 处 helper 双写**（`match { Value::Boxed(b)=>…, Value::BoxedStruct(gc)=>… }`），且 `object o = 5;
object p = 5; ReferenceEquals(o,p)` 在 C# 是 `false`（两个不同盒），当前基元装箱无引用身份 → 语义偏差。

## 目标（User 裁决 2026-08-13：全对齐 C#）

**基元装箱也产堆 `ScriptObject` + 引用身份**，与 struct 装箱统一：
- `__box_prim` 改成 alloc `ScriptObject`（`type_desc` = 基元 wrapper 类型 `Std.Int32`/`Std.Boolean`…），
  裸标量存进对象 → 返 `Value::BoxedStruct(GcRef)`（复用现有 GC-ref 引用身份机制）。
- 拆箱（AsCast/`__unbox`）从 boxed ScriptObject 读回裸标量。
- **收敛所有双写 helper** 到单一 BoxedStruct 路径。
- 最终**删除 `Value::Boxed` 变体 + `BoxedPrim` 结构** —— Value 少一个变体，密度/一致性双赢。

## Scope（实际改动文件，全在 `src/runtime/`，零 `src/compiler/`）

| 文件 | 变更 | 说明 |
|------|------|------|
| `src/runtime/src/gc/heap.rs` | MODIFY | `MagrGC::alloc_boxed_prim` trait 方法（默认回落 alloc_object） |
| `src/runtime/src/gc/arc_heap.rs` | MODIFY | `alloc_boxed_prim` 覆盖（struct_bytes 按标量宽度定尺）+ 提取 `finish_alloc` 共享尾部；删 Boxed sizing 臂 |
| `src/runtime/src/metadata/well_known_names.rs` | MODIFY | `int_wrapper_scalar_spec(name) -> Option<(width, signed)>` |
| `src/runtime/src/metadata/types.rs` | MODIFY | 删 `Value::Boxed=13` + `BoxedPrim`；`ScriptObject::boxed_prim_i64()`；GC visit / equality 收敛 |
| `src/runtime/src/metadata/types_tests.rs` | MODIFY | `boxed_prim_i64` 宽度+符号 round-trip 单测 |
| `src/runtime/src/corelib/convert.rs` | MODIFY | `box_prim_to_heap` + `builtin_box_prim` 产 BoxedStruct；`value_to_str` / `arg_i64` 收敛 |
| `src/runtime/src/corelib/object.rs` | MODIFY | GetType：删 Boxed 臂（BoxedStruct 覆盖） |
| `src/runtime/src/corelib/reflection.rs` | MODIFY | SetValue 拆箱 BoxedStruct-prim |
| `src/runtime/src/corelib/repl.rs` | MODIFY | 会话变量类名走 type_desc.name |
| `src/runtime/src/interp/exec_object.rs` | MODIFY | is_instance / as_cast 基元盒 vs struct 盒分流 |
| `src/runtime/src/interp/exec_value.rs` | MODIFY | convert_value 拆箱 BoxedStruct-prim |
| `src/runtime/src/interp/exec_vcall.rs` | MODIFY | 基元盒方法派发（this=裸标量） |
| `src/runtime/src/jit/helpers/object.rs` | MODIFY | JIT is/as 镜像 interp |
| `src/runtime/src/jit/helpers/vcall.rs` | MODIFY | JIT 基元盒方法派发镜像 interp |
| `docs/book/src/runtime/struct-value-semantics.md` | MODIFY | 「基元装箱统一到 BoxedStruct」机制节 |
| `src/tests/types/boxed_primitive_is_as.z42`、`box_unbox/` | 只读引用 | 现有 golden，验端到端（未改） |

**Out（后续 Phase）**：Phase 3 FFI 值类型 marshaling（R5）、Phase 4 单标量塌缩 + runtime 谓词收敛（R7）。
编译器侧（`src/compiler/`）**不动**（`__box_prim` 发射点、装箱路由不变）。

## 与其它 in-flight 的关系

- **[[rebuild-class-access-on-unify]]（「重建类访问」track）**：compiler + 格式 bump（1.33/0.38），会 rebase 到
  origin/main + force-push #186。本 change 是 **runtime + 零格式 bump**（预期）→ 不同子系统、**低碰撞**；
  谁先落地另一个 rebase 即可。
- **[[unify-object-byte-layout-program]]**：也是 runtime 内存模型（引用压 8B），与本 change **深度耦合** →
  **串行**（本 Phase 2-4 先，object-layout 后，User 已裁决）。

## 验证

`xtask test` 全绿（含 `cargo test --lib` —— runtime 改动 [[xtask-test-excludes-cargo-test]]）+ 反射/装箱
golden e2e（boxed int 引用身份、GetType、GetValue/SetValue、ToString）+ self-host 不动点（编译器不动应逐字节）。

🤖 Generated with [Claude Code](https://claude.com/claude-code)
