# Spec: interp 寄存器文件预分配

## MODIFIED Requirements

### Requirement: 解释器 frame 寄存器文件按函数寄存器总数预分配

**Before:** `func.max_reg` 恒为 0（zbc reader 硬编码 + loader 从不回填），`Frame::new*` 走
`else { args.len() }` 分支，寄存器文件仅按实参数起步；后续对更高寄存器的写入逐个触发
`Frame::set_grow`（`resize(idx+1, Null)`，realloc + memmove + 清零）。

**After:** loader 在 `build_block_indices` post-load 阶段用 `Function::reg_file_len()` 计算
函数寄存器文件长度（COUNT）并回填 `func.max_reg`；`Frame::new*` 走 `max_reg > 0` 分支一次性
预分配到正确长度，热路径不再触发 `set_grow`。

#### Scenario: 普通函数一次性预分配到正确长度
- **WHEN** 一个函数用到寄存器 `%0..%N`（N ≥ 实参数），被解释执行
- **THEN** 其 `Frame` 的寄存器文件在构造时即为 `max(N+1, 实参数)` 长、全 Null 初始化
- **AND** 函数体内对 `%0..%N` 的写入全部落在 in-bounds 快路（`set` 而非 `set_grow`）

#### Scenario: try/catch 的 catch 寄存器被计入
- **WHEN** 一个函数含 `try/catch`，其 catch 寄存器 `%c` 只由运行时在 `install_catch` 写入、
  没有任何指令以它作 dst（IR 优化可能删掉最后引用它的指令）
- **THEN** `reg_file_len()` 仍把 `%c` 计入（经 `exception_table` 折叠），寄存器文件覆盖到 `%c`，
  运行时写 catch 寄存器不越界

#### Scenario: 读取范围内从未写过的寄存器 → Null（interp/JIT 一致）
- **WHEN** 合法字节码之外的边界：读取一个「在预分配范围内、但从未被写入」的寄存器
- **THEN** 解释器返回 `Value::Null`（与 JIT 一致——JIT 早已预分配并读到 Null）
- **AND** 此前 interp 会 `bail!("undefined register")`；本变更消除该 interp/JIT 行为分歧
- **NOTE** z42c codegen 保证 define-before-use，合法字节码不会到达此边界；此场景只固化
  interp 与 JIT 在该边界上的行为一致性

#### Scenario: 所有构建配置均预分配（不依赖 jit feature）
- **WHEN** 以 `--no-default-features`（interp-only / wasm，无 jit）构建并解释执行
- **THEN** `func.max_reg` 仍被 `reg_file_len()` 回填（计算逻辑在始终编译的 `bytecode.rs`），
  预分配同样生效

#### Scenario: 输出/自举字节不变
- **WHEN** 对同一批源码，用本变更前后的 z42vm 分别解释执行（dump-bound / 全套 golden / z42c 自举）
- **THEN** 所有可观测输出逐字节一致；z42c 自举 gen1 == gen2 逐字节（预分配是纯运行时优化，
  不改任何 emit）

## IR Mapping

无新 IR 指令、无 zbc/zpkg 格式变更（`func.max_reg` 在 load 时由 runtime 计算，不读写 wire）。

## Pipeline Steps

受影响的 pipeline 阶段：
- [ ] Lexer — 不涉及
- [ ] Parser / AST — 不涉及
- [ ] TypeChecker — 不涉及
- [ ] IR Codegen — 不涉及
- [x] VM interp — frame 寄存器文件预分配（本变更核心）
- [x] VM 元数据 load — `build_block_indices` 回填 `func.max_reg`
- [x] JIT — `translate::max_reg` 复用上提逻辑（去重，行为不变）
