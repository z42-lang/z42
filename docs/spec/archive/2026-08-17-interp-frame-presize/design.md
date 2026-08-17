# Design: interp 寄存器文件预分配

## Architecture

```
zbc load
  └─ read_module (zbc_reader.rs)      func.max_reg = 0   （wire 不带；保持）
  └─ build_block_indices (loader.rs)  ← 回填点（已 post-load 遍历 &mut functions）
        func.max_reg = func.reg_file_len()          ← 新增，一次性
                          │
                          ├─ 使用 Instruction::written_reg()   （上提自 translate.rs）
                          └─ 折叠 param_count / exception_table catch regs / 写入 dst

interp 执行
  └─ exec_function_from_regs → Frame::new_from_regs(caller_regs, args, func.max_reg)
        size = if max_reg > 0 { max_reg } else { args.len() }   ← 现在恒走 max_reg 分支
        regs.resize(size, Null)  一次性                          ← 不再逐个 set_grow

JIT 执行（不变）
  └─ translate::max_reg(func) → 复用 Function::reg_file_len() / Instruction::written_reg()
```

**单一权威**：「一个函数的寄存器文件需要多长」由 `Function::reg_file_len()` 唯一计算，interp
（load 时回填）与 JIT（compile 时调用）共用，消除两处逻辑漂移风险。

## Decisions

### Decision 1: 计数逻辑上提到始终编译的 bytecode.rs（而非在 loader 复制或依赖 jit）
**问题：** 寄存器计数逻辑现在 `jit::translate::max_reg`（`#[cfg(feature="jit")]`）。interp-only /
wasm 构建无 jit → loader 无法调用。
**选项：**
- A（上提）：把 `written_reg`（~69 行 match）+ 计数扫描移到 `metadata/bytecode.rs`，作
  `Instruction::written_reg()` + `Function::reg_file_len()`。JIT 复用。
- B（loader 复制扫描）：在 loader 内重写一遍扫描逻辑。
- C（cfg-gate 回填）：仅 jit 构建回填（spike 的做法）。
**决定：** 选 A。B 违反 DRY，且 translate.rs 注释明确警告「catch reg 必须计入否则 OOB panic」——
复制极易漏；C 让 interp-only/wasm 拿不到预分配（这些是真实目标）。上提是唯一同时满足「所有构建生效
+ 单一权威」的方案。

### Decision 2: 计算时机 = post-load 一次，缓存进 func.max_reg
**问题：** 扫描函数体是 O(指令数)。每次调用都算 = 灾难。
**决定：** 在 `build_block_indices`（已 post-load 遍历 `&mut functions`，做 block_index /
branch_targets / frame_meta 预计算）里算一次，存入既有字段 `func.max_reg`。该字段此前恒 0、
现被赋真值，无需新字段。回填时 `func.max_reg` 仍是 0，故扫描纯净、不自引用。

### Decision 3: func.max_reg 存 COUNT（寄存器总数），不是 INDEX
**问题：** `translate::max_reg` 返回**最大索引**；`Frame::new*` 把 `max_reg` 当**长度**用。
**决定：** `Function::reg_file_len()` 返回 COUNT = maxIndex + 1（与字段文档「Total number of
registers used」一致）。JIT 的 `translate::max_reg`（需 INDEX）改为 `reg_file_len() - 1` 再折叠
它自己的 param/catch 逻辑，保持返回 INDEX 语义不变。

### Decision 4: 接受「读未写寄存器 bail→Null」的 interp/JIT 一致性变化
**问题：** 预分配后，读一个「范围内但从未写」的寄存器，interp 从 `bail!("undefined register")`
变为返回 `Null`。
**分析：** JIT 早已预分配到 max_reg、读到 Null；此前是 **interp 比 JIT 更严**。z42c codegen 保证
define-before-use，合法字节码不会读未写寄存器，故此边界在真实程序中不可达。本变更让 interp 与 JIT
在该边界一致（parity 修复，非回归）。
**决定：** 接受并固化为 spec 场景 + 由全套 GREEN（尤其 vm-jit 一致性 gate + 自举字节不动点）证明
无真实行为漂移。

## Implementation Notes

- `Instruction::written_reg(&self) -> Option<u32>`：纯函数，match 所有写 dst 的 variant。从
  `translate.rs` 原样搬迁（逻辑不变），translate.rs 内改调 `instr.written_reg()`。
- `Function::reg_file_len(&self) -> u32`：
  ```
  let mut max_idx = param_count.saturating_sub(1);          // param 占低位寄存器
  for e in exception_table() { max_idx = max_idx.max(e.catch_reg); }
  for block in &blocks { for instr in &block.instructions {
      if let Some(d) = instr.written_reg() { max_idx = max_idx.max(d); } } }
  max_idx + 1                                                // COUNT
  ```
  边界：零寄存器零参函数 → param_count.saturating_sub(1)=0 → 无写入 → 返回 1（1 槽，无害微
  过分配；与旧 fallback `args.len()` 在 args=0 时的 0 槽相比多 1 槽，可忽略）。
- loader `build_block_indices`：在既有 per-func 预计算块尾追加 `func.max_reg = func.reg_file_len();`。
  此时 `blocks` / `cold`(exception_table) 已从 reader 装好，可扫。
- **撤销 spike 边改**：`jit/mod.rs` 的 `pub(crate) mod translate` 改回 `mod translate`（方案 A 下
  loader 不再穿透 jit，无需放开可见性）；loader 里 spike 的 `#[cfg(feature="jit")]` 回填块替换为
  无条件 `func.max_reg = func.reg_file_len();`。

## Testing Strategy

- **单元测试**（`bytecode_tests.rs`）：`reg_file_len` 三例——① 仅 param（写入不越 param）②
  写入 dst 越过 param ③ 含 catch reg 且无指令引用它（验证 exception_table 折叠，正是 translate.rs
  注释警告的 OOB 场景）。
- **Golden / e2e**：现有 e2e + cross-zpkg + stdlib + 自举已海量覆盖含 try/catch 的函数执行；
  预分配正确性由「输出逐行 identical + 自举 gen1==gen2 逐字节」端到端证明。
- **VM 验证**：完整 `xtask test`（GREEN gate 全 stage；vm-jit 一致性由 CI test-vm-jit(linux-x64)
  专腿覆盖 interp↔JIT 结果一致，恰好把 Decision 4 的 parity 兜住）。
- **性能回归**：spike 已实测 ~3.7%；正式实现后复跑同配方 A/B 确认不劣于 spike。
