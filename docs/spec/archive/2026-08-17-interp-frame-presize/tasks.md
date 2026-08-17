# Tasks: interp 寄存器文件预分配

> 状态：🟢 已完成 | 创建：2026-08-17 | 完成：2026-08-17

## 进度概览
- [x] 阶段 1: 上提计数逻辑（bytecode.rs）
- [x] 阶段 2: loader 回填 + JIT 去重
- [x] 阶段 3: 测试与验证

## 阶段 1: 上提计数逻辑
- [x] 1.1 `bytecode.rs`：新增 `Instruction::written_reg(&self) -> Option<u32>`（从 `translate.rs` 原样搬迁 match）
- [x] 1.2 `bytecode.rs`：新增 `Function::reg_file_len(&self) -> u32`（param_count / exception_table catch regs / 写入 dst 折叠，返回 COUNT）

## 阶段 2: loader 回填 + JIT 去重
- [x] 2.1 撤销 spike 边改：`jit/mod.rs` 恢复 `mod translate;`
- [x] 2.2 `loader.rs`：`build_block_indices` 尾部无条件 `func.max_reg = func.reg_file_len();`（替换 spike 的 cfg-gate 版）
- [x] 2.3 `jit/translate.rs`：`max_reg()` 改用 `func.reg_file_len() - 1` + `instr.written_reg()`，删除本地 `written_reg`

## 阶段 3: 验证
- [x] 3.1 `bytecode_tests.rs`：`reg_file_len` 单元测试 ×3（param-only / 越 param / catch reg）
- [x] 3.2 `cargo build --release`（z42vm）+ `cargo test --lib` + `cargo test --release --tests --no-run`（集成测试编译，避免 [[xtask-test-excludes-cargo-test]] 的 CI 晚炸）
- [x] 3.3 `xtask test`（完整 GREEN gate 全绿：e2e + cross-zpkg + stdlib + compiler 自举 5/5 gen1==gen2 + vscode-syntax）
- [x] 3.4 A/B 复测：实测 7.45→7.22s ≈ 1.03×/~3.1%（同配方 hyperfine，与 spike 3.7% 一致，噪声内）
- [x] 3.5 spec scenarios 逐条覆盖确认（预分配/catch reg/读未写→Null/interp-only 生效/输出·自举字节不变 均验）
- [x] 3.6 文档同步：`interp/README.md` + `docs/book/src/runtime/superinstr-fusion.md` 追加寄存器文件预分配一节（Open Question 已裁：并入现有页，不新建）

## 备注
- 无格式 bump、无 wire 变更、无 z42c 侧改动 → 不触 version-bumping / bootstrap-seed 纪律。
- Decision 4（读未写寄存器 bail→Null）的行为一致性由 vm-jit gate + 自举字节不动点兜底。
- Deferred（本 change 不做，探索已确认低头子/高风险）：collect_args 池化（单独 spike）；
  drop\<Frame\> / push_frame / pop_frame 瘦身。
