# Proposal: interp 寄存器文件正确预分配（消除 set_grow 逐个增长）

## Why

解释器每次函数调用构造 `Frame` 时，本应按「函数用到的寄存器总数」一次性分配寄存器文件
（`Frame::new*` 有 `if max_reg > 0 { max_reg } else { args.len() }` 的预分配分支）。但
`func.max_reg`（"Total number of registers used, 0 = unknown → 动态增长"）在 zbc reader
里**两处硬编码为 0**（`zbc_reader.rs:1712` / `:2204`），且全仓 `.max_reg =` **零处赋值**
——loader 从不回填。

后果：interp 每个 frame 只按**实参数**起步，所有超出实参的寄存器写逐个命中 `#[cold]` 的
`Frame::set_grow`，每次 `resize(idx+1, Null)` 做一次 realloc + memmove + 清零。前端
typecheck profile 里这就是 **`set_grow` 257 samples（#1 leaf）+ `extend_with` 214 + `bzero` 73**
那一坨（frame ~22% 桶里最大的可攻击块）。

JIT 侧**不受影响**——它用 `translate::max_reg(func)` 扫描函数体（param_count + catch regs +
所有写入寄存器）算出真实寄存器数并预分配。interp 只是没用这套已验证的逻辑。

**实测收益（本 change 的 spike A/B，各 6 runs，前端 typecheck big.z42）**：
baseline 7.464s ±0.159 → 预分配 7.189s ±0.033 = **1.04× / ~3.7% faster**，输出**逐行 identical**，
profile 确认 `set_grow` 从热点榜**消失**、`extend_with` 214→96 腰斩。

## What Changes

- 把「函数寄存器文件长度」的权威计算从 jit-gated 的 `translate::max_reg` **上提**到始终编译的
  `metadata/bytecode.rs`：新增 `Instruction::written_reg()` + `Function::reg_file_len()`（返回
  COUNT = maxIndex + 1）。
- loader 的 `build_block_indices`（已 post-load 遍历 `&mut functions`）里一次性回填
  `func.max_reg = func.reg_file_len()`，**所有构建配置**（含 interp-only / wasm，无 jit feature）
  均生效。
- JIT `translate::max_reg` 改为复用上提逻辑（`instr.written_reg()`），去除重复实现。
- `Frame::new*` **无需改动**——其 `max_reg > 0` 预分配分支本就正确，此前从未被喂过非零值。

## Scope（允许改动的文件）

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `src/runtime/src/metadata/bytecode.rs` | MODIFY | 新增 `Instruction::written_reg()` + `Function::reg_file_len()`（权威寄存器计数，含 param_count / catch regs / 写入 dst 扫描）|
| `src/runtime/src/metadata/loader.rs` | MODIFY | `build_block_indices` 内回填 `func.max_reg = func.reg_file_len()` |
| `src/runtime/src/jit/translate.rs` | MODIFY | `max_reg()` 复用 `Function::reg_file_len()` / `Instruction::written_reg()`，删除重复的本地 `written_reg` |
| `src/runtime/src/metadata/bytecode_tests.rs` | MODIFY | `reg_file_len` 单元测试：param-only / 写入越过 param / catch reg 覆盖 |
| `src/runtime/src/interp/README.md` | MODIFY | 功能索引/核心文件补注 frame 寄存器文件预分配来源 |
| `docs/book/src/runtime/superinstr-fusion.md` | MODIFY | 追加一节：寄存器文件预分配（同 `build_block_indices` post-load 预计算的姊妹机制）+ interp/JIT 计数一致性 |

**只读引用**（理解上下文，不修改）：

- `src/runtime/src/interp/mod.rs` — `Frame::new*` / `set_grow` 现有预分配分支（确认无需改）
- `src/runtime/src/jit/lazy.rs` — JIT 如何用 `translate::max_reg` 预分配（对照逻辑）
- `src/runtime/src/metadata/zbc_reader.rs` — `max_reg: 0` 硬编码来源（根因确认）

## Out of Scope

- **collect_args 池化/栈分配**（builtin/native/ctor/closure 的 `&[Value]` ABI 分配）——单独 spike + change。
- **drop\<Frame\> / push_frame / pop_frame 瘦身**——探索表明基本是不可约减的基础成本（drop 必须逐个
  释放存活 Value）或高风险（push/pop 记账、历史 name 缓存回归），归 Deferred。
- **z42c 侧发射 REGT/max_reg 到 zbc**——runtime 端 load 时计算已足够，不动 wire 格式（无格式 bump）。

## Open Questions

- [x] book 机制页落位 → **并入现有 runtime 页 `superinstr-fusion.md`**（User 2026-08-17 裁：不新建页；
  该页是 interp 提速机制页，代码清单已含同一 `build_block_indices` + `bytecode.rs` 基础设施）。
