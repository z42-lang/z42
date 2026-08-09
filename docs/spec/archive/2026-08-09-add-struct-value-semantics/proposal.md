# Proposal: struct 值语义 —— 内联/栈布局全重构（C#-style value type）

> 状态：🟡 DRAFT — User 已裁决**选项 B（真·内联/栈布局）+ Decision 3a（完整可变）**。
> 本 DRAFT 按 B 重写。因 B 是月级、跨多子系统重构，组织为**分阶段程序**（多 PR），非单个 change。

## Why

z42 有 `struct` 关键字，但**运行时 struct 与 class 完全同构**：都是 `Value::Object(GcRef<ScriptObject>)`，
堆分配 + 引用语义。`struct` 目前只影响继承（不隐式继承 `Std.Object`）与反射标志
（`CLASS_FLAG_STRUCT`），**不碰内存布局与赋值语义**。后果：`var b = a; b.field = 99;` 对 struct **会改到
`a`**（共享同一堆对象）。

目标：把 struct 做成**和 C# 一样的值类型**——数据**内联存储在容器里**（栈帧寄存器区 / 父对象槽区 /
数组扁平 backing），**无独立堆身份、无 GC 托管**，赋值/传参/返回/存容器 = **逐字段复制**（memcpy 语义），
可寻址位置（局部/字段/数组元素）上的 struct 支持**原地可变**。这同时是 [packed-primitive-arrays] 里
"② inline `struct[]`" 一直等待的前置（真值语义大改）。

## What Changes（选项 B）

- **布局模型（编译器）**：为每个 struct 类型计算**扁平布局**——递归展开嵌套 struct 字段成叶子槽序列，
  得到每字段的 `offset` 与类型的 `width`（占用槽数）。struct 值不再是"单个 `Value` 操作数"，而是
  容器里一段**连续槽区间** `[base, base+width)`。
- **IR / 操作数模型**：struct-typed 局部/临时占用**连续寄存器区间**；新增/扩展 struct-aware 指令：
  区间复制（copy width 槽）、字段区间 get/set（按 offset+width）、结构体传参/返回的区间搬运。
- **对象布局（运行时）**：class 的 struct 字段**内联**进父 `ScriptObject.slots`（预留 width 连续槽），
  `field_index` 变为 name→offset；`struct[]` 为 `len*width` 扁平 backing。
- **复制语义**：区间复制点（赋值/传参/返回/存字段/存数组元素）= clone width 槽（叶子引用字段共享句柄、
  叶子值字段复制、嵌套 struct 递归 = C# 字段级复制）。
- **lvalue 原地可变（3a）**：`obj.pt.x = 5` / `arr[i].x = 5` / `line.a.x = 3` = 算出叶子地址
  `(容器 base + Σoffset)` 直接写，**不经临时拷贝**。
- **装箱边界**：`struct → object/接口` = 在装箱点把 struct 槽区**拷进一个堆 `ScriptObject`**（CLR 式
  boxing，产生 `Value::Object`）；拆箱 = 拷回寄存器区间。值类型因此有"内联未装箱"+"堆装箱"两态。
- **默认值 / 相等 / 反射**：struct 默认值 = 全字段默认（内联零初始化，非 null）；`==` 逐字段值相等；
  `IsValueType` 与新布局一致。
- **GC 根扫描**：内联 struct 的叶子引用槽落在被扫描的寄存器/对象槽数组内，随现有扫描被追踪（需确认
  区间边界正确纳入）。
- **字节打包（User 裁决：v1 必做）**：叶子**字节精确布局**（`int`=4B/`bool`=1B/ref=1 槽…，含对齐），
  逼近 C# 内存密度——**非可选**，与 [packed-primitive-arrays] 的字节布局基础设施**收敛**。struct 存储
  从"N 个 `Value` 槽"降为**字节 blob**（帧局部区 / 父对象 blob 区 / 数组扁平 backing）。
- **格式 bump**：struct 字节布局元数据（byte offset/size/align）需入 zbc/zpkg 供跨包消费 + 新 struct
  指令 → **zbc/zpkg minor bump + 两阶段 nightly 纪律**（version-bumping.md + bootstrap-seed.md）。
- **逃逸分析**：struct 不再走 `ObjNew`→堆/arena（恒内联）；引用类型的 `StackObject` 逃逸优化不变。

## 分阶段程序（多 PR；每阶段独立 GREEN + 归档 —— User 裁决：分阶段）

> 月级、跨 5 子系统重构；**两阶段 nightly 纪律使单一原子 change 物理上不可能**（support 先行、晚一
> nightly 再 use）。字节打包（γ）贯穿各存储阶段，非独立末阶段。

- **P1 — 字节布局地基 + 局部 struct 值语义（同模块）**：字节精确 `StructLayout`（byte offset/size/align，
  递归嵌套 + 自含值字段报错）；帧局部 struct 存为**字节 blob 区间**；整体赋值/传参/返回的 blob 复制；
  局部嵌套 lvalue 原地可变（3a）。**收敛 packed-array 字节布局地基的 struct 部分**。先不动对象内联/
  数组/跨包。
- **P2 — 局部原地可变全场景补全**（若 P1 收敛面未含全部局部可变）。
- **P3 — 对象内联 + `struct[]` 字节 backing**：class 的 struct 字段字节内联进父对象 blob；数组字节扁平
  backing + 元素原地可变。**与 [packed-primitive-arrays] 全面收敛**。
- **P4 — 跨 zpkg 字节布局元数据（zpkg 格式 bump）+ 装箱/拆箱 + 反射一致**：布局入 zpkg，跨包消费；
  boxing 拷进堆对象；`is`/`as`/`GetType`。
- **P5 — JIT 值路径 + 密度/性能收尾**：interp 全绿后，struct 区间/字节访问的 JIT 支持 + bench。

## Scope（P1 首阶段；后续阶段各自补 Scope）

> ⚠️ 全程序 Scope 巨大，此处仅列 **P1（布局 + 局部/传参/返回内联复制，同模块）** 的 Scope。
> P2–P5 各在其 change 容器补齐。

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `src/compiler/z42c.semantics/src/StructLayout.z42` | NEW | struct 扁平布局计算（offset/width，递归嵌套） |
| `src/compiler/z42c.semantics/src/ClassDescBuilder.z42` | MODIFY | struct 类型产出布局元数据（供 emit + 运行时） |
| `src/compiler/z42c.semantics/src/ExprEmitter.z42` | MODIFY | struct-typed 表达式 → 寄存器区间；整体赋值区间复制；传参/返回区间搬运 |
| `src/compiler/z42c.semantics/src/FunctionEmitter.z42` | MODIFY | 局部/参数/返回值的 struct 区间分配（寄存器分配感知 width） |
| `src/compiler/z42c.semantics/src/IrGen.z42` | MODIFY | struct 区间复制 IR 生成 |
| `src/compiler/z42c.semantics/src/IrEscapeAnalysis.z42` | MODIFY | struct 的 new 不再走 stack_alloc/堆（恒内联） |
| `src/libraries/z42.ir/src/IrInstr.z42` | MODIFY | 新增 struct-aware 指令（区间 copy / 字段区间 get-set）——**触发格式评估** |
| `src/runtime/src/metadata/types.rs` | MODIFY | TypeDesc 承载 struct 布局；Frame 寄存器区间语义（叶子仍 Value） |
| `src/runtime/src/interp/exec_object.rs` | MODIFY | struct 局部的构造/字段区间访问（P1 限局部，对象内联留 P3） |
| `src/runtime/src/interp/exec_instr.rs` | MODIFY | 新 struct 指令 dispatch |
| `src/runtime/src/interp/mod.rs` | MODIFY | Frame 区间复制/搬运；collect_args/return 对 struct 区间 |
| `src/tests/struct-value-semantics/` | NEW | P1 golden：局部赋值/传参/返回复制语义 |
| `src/compiler/z42c.semantics/tests/codegen/codegen_tests.z42` | MODIFY | struct 布局 + 区间复制 IrDump 对比 |
| `docs/book/src/runtime/struct-value-semantics.md` | NEW | 机制页（布局/区间操作数/复制/lvalue/装箱/GC，随阶段增量补） |
| `docs/book/src/language/structs.md` | NEW/MODIFY | 语言页：值语义规则 |
| `docs/book/src/SUMMARY.md` | MODIFY | 挂新页 |
| `docs/features.md` | MODIFY | 登记 struct 值语义程序 |
| `docs/roadmap.md` | MODIFY | 新增 struct-value-semantics 程序（P1–P5）+ Deferred 索引 |

**只读引用**：`bytecode.rs`（`CLASS_FLAG_STRUCT`）、`interp/stack_alloc.rs` + `escape-analysis-stack-alloc.md`
（逃逸机制，区分二者）、`docs/design/runtime/zbc.md` / `zpkg.md` + `version-bumping.md`（P4 格式 bump）、
memory `packed-primitive-arrays`（P3/P5 收敛点）。

## Out of Scope（Deferred，登记 roadmap）

- **`readonly struct` / `ref struct` / `in` 参数零拷贝**：各自独立特性；v1 传参一律复制。
- **struct 自定义 `==`/`Equals`/`GetHashCode` 重载**：默认值相等之外。
- **泛型 `where T:struct` 的 monomorphization 密度收益**：本程序保证语义正确，不做特化。

## Open Questions（大方向已裁决，剩 P1 gate 内子决策）

- [x] **选项 A vs B** → B（内联/栈布局）。
- [x] **可变范围** → 3a（完整原地可变）。
- [x] **叶子存储 γ** → **字节打包（v1 必做）**，与 packed-array 字节地基收敛。
- [x] **落地方式** → 分阶段 P1–P5。
- [ ] **Decision α（P1 gate）**：struct 值操作数模型——"base + 字节布局元数据"引用字节 blob 区间（推荐）。
- [ ] **格式 bump 时机（P1 gate）**：新 struct 指令 zbc bump 落 P1；跨包布局元数据 zpkg bump 落 P4——
  两次 bump 分别踩两阶段纪律窗口，还是设法合并，P1 gate 定。
- [ ] **P1 收敛面（P1 gate）**：P1 是否含 class 的 struct 字段（否则纯局部 struct），P2 是否并入 P1。
