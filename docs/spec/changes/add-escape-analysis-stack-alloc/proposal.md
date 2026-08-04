# Proposal: 逃逸分析驱动的栈上分配（对象 + 数组，interp-first）

## Why

Profile 显示 interp 热路径 ~7% 花在对象/数组的堆分配（`ArcMagrGC::alloc_object`：region 分配锁
+ GC 追踪/清扫，实测 ~395ns/alloc）。这些分配里有相当一部分是**不逃逸的临时对象/数组**——只在
创建它的函数帧内被读写、从不流出。它们本可以在**帧局部 arena** 上分配、随帧退出即释放、完全绕过
GC，省掉分配锁 + 标记 + 清扫。

z42 已经为**闭包**落地过这条完整范式（`impl-closure-l3-escape-stack` 引入 `Value::StackClosure`
+ `Frame::env_arena` + GC 遍历跳过）。本变更把同一范式推广到 `new Foo(...)` 对象与 `new T[n]` /
数组字面量，由一个**可扩展规则**的编译期逃逸分析 pass 驱动，并以现有 `OptSet` 位独立开关。

不做的代价：对象密集代码（尤以编译器自举本身）持续吃满 GC 分配开销；已验证的 StackClosure 范式
无法惠及占比更大的对象/数组分配。

## What Changes

- **编译器（z42c.semantics）**：新增 `IrEscapeAnalysis` pass——**流不敏感、CFG-free 的 may-escape
  过近似 + 角色感知的"逃逸汇点规则表"**（可扩展点）+ ctor `this`-escape 单函数摘要。证明不逃逸的
  `ObjNew` / `ArrayNew` / `ArrayNewLit` → 置 `StackAlloc=true`。
- **开关（OptSet）**：加 `Opt.StackAlloc=64`（`All` 63→127），`ByName("stack-alloc")`。debug(-O0) 关 /
  release 开；CLI `--opt/--no-opt stack-alloc`；toml `[optimize] stack-alloc=true`。
- **IR + zbc/zpkg 格式**：`ObjNewInstr` / `ArrayNewInstr` / `ArrayNewLitInstr` 加 `bool StackAlloc`
  字段 + zbc 尾字节编码（照 `MkClosInstr.StackAlloc` 先例）。**bump zbc 1.28→1.29 / zpkg 0.33→0.34**。
- **运行时（interp-only 消费，準则 1）**：新增 `Value::StackObject` / `Value::StackArray` 变体 +
  帧 arena；`ObjNew`/`ArrayNew`/`ArrayNewLit` 按 flag 分叉到 arena；`FieldGet/FieldSet` /
  `ArrayGet/ArraySet/ArrayLen` 识别栈变体；GC 外部根扫描器扫 arena slots 作根、但不 sweep 栈条目。
  **JIT 读取但忽略该 flag（照常堆分配）**——interp-first：优化只服务无 Cranelift 兜底的 interp，
  JIT 语义不变、gauntlet interp==jit 靠"输出相同、表示不同"成立。

## Scope（允许改动的文件）

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `src/compiler/z42c.semantics/src/OptSet.z42` | MODIFY | 加 `StackAlloc=64` 位、`All`→127、`ByName("stack-alloc")` |
| `src/compiler/z42c.semantics/src/IrEscapeAnalysis.z42` | NEW | 逃逸分析 pass + 规则表 + ctor 摘要 |
| `src/compiler/z42c.semantics/src/IrOptPipeline.z42` | MODIFY | `_optFunc`/`Run` 接入门控分支 |
| `src/compiler/z42c.semantics/README.md` | MODIFY | 功能索引 + 核心文件加新 pass |
| `src/compiler/z42c.semantics/tests/escape_analysis/` | NEW | pass 单测（合格/不合格/ctor 泄漏/规则各汇点） |
| `src/libraries/z42.ir/src/IrInstr.z42` | MODIFY | 三个分配指令加 `bool StackAlloc` 字段 + ctor 参数 |
| `src/libraries/z42.ir/src/BinaryFormat/ZbcInstr.z42` | MODIFY | 三个分配指令 zbc 尾字节编码 |
| `src/libraries/z42.ir/src/BinaryFormat/ZbcFormat.z42` | MODIFY | `Minor` 28→29 |
| `src/runtime/src/metadata/bytecode.rs` | MODIFY | `ObjNewInsn`/`ArrayNewInsn`/`ArrayNewLitInsn` 加 `stack_alloc: bool` |
| `src/runtime/src/metadata/zbc_reader.rs` | MODIFY | 读 flag + `ZBC_VERSION`→1.29 + `ZPKG_VERSION`→0.34 |
| `src/runtime/src/metadata/types.rs` | MODIFY | `Value::StackObject`/`StackArray` 变体 + payload + GC 子引用遍历 |
| `src/runtime/src/interp/mod.rs` | MODIFY | `Frame` 加对象/数组 arena 字段 + 初始化 |
| `src/runtime/src/interp/exec_object.rs` | MODIFY | `obj_new` 按 flag 分叉 arena；`FieldGet/FieldSet` 识别 `StackObject` |
| `src/runtime/src/interp/exec_array.rs` | MODIFY | `array_new`/`array_new_lit` 按 flag 分叉 arena |
| `src/runtime/src/interp/exec_instr.rs` | MODIFY | 分发 flag 透传；数组 get/set/len 识别 `StackArray` |
| `src/runtime/src/vm_context.rs` | MODIFY | 外部根扫描器加扫对象/数组 arena slots |
| `src/runtime/src/gc/arc_heap.rs` | MODIFY | `size_of`/子引用遍历分支加栈变体（照 `StackClosure`） |
| `src/runtime/src/jit/translate.rs` | MODIFY | 构造新 Insn 字段兼容（读取即忽略，不改 emit） |
| `docs/book/src/runtime/optimization-pipeline.md` | MODIFY | 新增「逃逸分析 / 栈上分配」pass 机制节 + 对齐日期 |
| `docs/book/src/runtime/escape-analysis-stack-alloc.md` | NEW | 分析算法 / 规则表 / 运行时 arena / 安全边界机制页 |
| `docs/book/src/SUMMARY.md` | MODIFY | 挂入新 book 页 |
| `docs/roadmap.md` | MODIFY | Deferred Backlog Index 登记 future 条目（JIT arena / 跨过程 / scope-reset） |
| `src/tests/` 下 e2e（栈对象/数组端到端） | NEW | golden：非逃逸对象/数组行为等价、逃逸对象仍堆分配 |

**只读引用**：

- `src/libraries/z42.ir/src/IrInstr.z42`（`MkClosInstr.StackAlloc` 先例）
- `src/compiler/z42c.semantics/src/IrOptInfo.z42`（def-use / `IsPure` / `DstId` 复用）
- `src/compiler/z42c.semantics/src/IrLicm.z42`（只读参考：确认 v1 不需 CFG/支配域）
- `src/runtime/src/metadata/types.rs` `StackClosureData`（arena 范式）
- `src/runtime/src/interp/exec_call.rs`（`env_arena` 物化范式）
- `docs/agent/rules/*` version-bumping / bootstrap-seed（格式 bump + 两阶段纪律）

## Out of Scope

- **JIT 侧 arena 落地**（JIT v1 仅读取忽略 flag、照常堆分配）→ future。
- **跨过程逃逸摘要 / 字段敏感 / 部分逃逸**（v1 规则保守：未知 call/未知指令一律判逃逸）→ future rule。
- **标量替换**（把对象炸成寄存器彻底消除分配）→ future 第二种 lowering，纳入同一规则框架。
- **scope/loop 级 arena 复位**（v1 arena 随帧退出释放，循环内每次创建累积，与 `StackClosure` 同）→ future。
- **struct 真值语义**（[[z42-structs-not-value-types]]，独立大改）——本变更不改 struct 语义，只优化
  不逃逸的堆对象分配落点。

## Open Questions

- [ ] （已由 AskUserQuestion 定）落地=帧 arena 栈对象；v1 目标=对象+数组同批。
- [ ] ctor `this`-escape 摘要的保守边界：ctor body 内"把 this 传给另一函数"是否一律判逃逸（v1 拟：是，
      不递归）——见 design D3，实施前 User 复核。
