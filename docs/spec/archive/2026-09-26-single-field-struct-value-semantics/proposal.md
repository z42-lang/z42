# Proposal: 单字段 struct 的值语义（坑点 ⑤）

> 状态：✅ 已实施（2026-09-26）。类型：**lang**（值类型表示模型的闸门变更）。
> 路线由 User 裁决（D3，两轮）：**⑤-blob** —— 单字段 struct 与多字段走**同一个**值模型，
> 而非归档原先设想的「单标量叶子塌缩（Phase B）」。
> 依赖：`fix-crosspkg-static-sret`（#851）—— 那是本刀暴露出来的**既存** bug，已独立成刀。

## Why

```z42
struct S { public int X; }
S a = new S(); a.X = 10;
S b = a;       b.X = 50;
// 修前：a.X == 50（！）—— 单字段 struct 走引用模型
```

闸门在 `StructLayout.IsBlobStruct`：`FieldCount >= 2`。低于它的 struct 落在引用模型上 ⇒
赋值、传参、返回、数组元素、类字段一律**共享**而非复制。这是学习手册与 reference 都
白纸黑字记着的「实现缺口，不是设计意图」（`memory-model.md`「已知偏差」、
`structs-records.md`「🔴 两个坑」之一），并配了活示例把坏输出钉在 transcript 里。

## 路线裁决（D3，两轮，全部实测支撑）

| | ⑤-blob（**选中**） | ⑤-scalar（归档原先的 Phase B 塌缩）|
|---|---|---|
| 语义收益 | 值语义 ✅ | 值语义 ✅（相同）|
| 机制 | **复用**已被 golden 铺满的 blob 模型（arena 字节 + 逐叶子复制 + sret + `__box_struct`）| **新建第三种表示**：ctor 无处写 `this.X` / 裸标量携不了身份（`primitive_class_name` 把 `Value::I64` 一律映射成 `Std.Int32`）/ `ReprOf` 是**死代码零消费点**，没有咽喉点可扩 |
| `GCHandle`（唯一带 native 成员的单字段 struct）| 需 native 桩支持 blob 返回（本刀顺带做了，~30 行，复用 `as_cast` + `struct_copy`）| 保持裸 `long`、native 侧更简单 |
| 与文档 | 推翻 `StructLayout.z42:305` 的「保持现有模型（塌缩=Phase B）」——**就地写明为什么** | 契合原意 |

⭐ **第二轮更正**（我先前给 User 的成本框架有误，实测后撤回）：`GCHandle` 自己的抬头文档写的
就是**值语义**（「拷贝 GCHandle 共享同一 slot：`h2 = h1` ⇒ `h1.Free()` 后 `h2.IsAllocated` 也变
false」）⇒ blob 化**兑现**这条文档而非破坏它；而 ⑤-scalar 的代价被我低估了整整一个量级。

## What Changes

1. `StructLayout.IsBlobStruct`：`FieldCount < 2` → `< 1`（并改写那段解释为什么推翻 Phase B 的注释）。
2. 🔴 **VM 侧同一判据的镜像必须同时翻**：`interp/exec_array.rs::try_struct_backed` 有一份逐字镜像
   （注释自称 “matches IsBlobStruct”）。只翻编译器 ⇒ `S[]`（单字段）退化成引用数组、元素全 `Null`，
   首次 `arr[0].X = v` 抛 `StructFieldSetPrim base: expected a struct value (StructRef), got Null`。
   这是全 VM **唯一**一份镜像（其余地方问「`struct_layout()` 交付了没有」）。
3. **native 桩支持 blob 返回**（`StubEmitter._emitNativeStubSret`）：`extern` 声明返回 blob 值 struct 时，
   桩发 `builtin nat(args…)` → `as_cast` 拆箱 → `struct_copy` 写进调用方的 sret 槽 → `ret`，
   并置 `MethodFlagSret`。全仓**唯一**这种形状是 `Std.GCHandle.Alloc`（GCHandle 单字段 ⇒ 翻门后才成为 blob）。
   🔴 踩过的坑：`ParamCount` **不含 sret**（只由 flag 承载），第一版把含 sret 的总数当 ParamCount ⇒
   VM 算出 `takes 4 / passes 3`，同一道门反向报。
4. **Rust 侧 `GCHandle` 的读写**（`corelib/gc.rs`，5 个 builtin 全经这两个函数）：
   `make_gc_handle` 改产**装箱 struct**（`box_struct_blob`）；`extract_gc_handle_slot` 改吃三种承载
   （`StructRef` / `BoxedStruct` / 旧的单槽 `Object`）——**读要宽、写要窄**：保留旧 `Object` 臂是为了
   让旧编译器产出的 zbc 仍能跑。

## 不做什么

- **不动 `Guid`**（另一个单字段 struct，字段是 `byte[]` **引用**叶子）：它全是纯 z42 成员，
  翻门后自然走 blob（1 个 ref 叶子），golden 已覆盖（`gu == gu2`、`ToString().Length == 36`）。
- **不做单标量叶子塌缩**（⑤-scalar）：降级为**性能优化**另立（真正收益是 `GCHandle`/FFI 的零 arena 分配）。
