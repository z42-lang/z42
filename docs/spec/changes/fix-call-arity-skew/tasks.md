# Tasks: fix-call-arity-skew（SIGS sret 位 + 首次绑定点签名判定，zbc 1.40 / zpkg 0.45）

## 调研（实测，全部完成）
- [x] 4 条 cross-zpkg fixture 复现：实例（VCall）/ sealed（去虚化 Call）两后端 `label null7`；静态已被 mangle 键挡住
- [x] 读 `MemberCollector._fillClass` 坐实洞的边界（实例/静态虚 = primary 裸键；常规静态 = 全签名 mangle）
- [x] 全量测试探针普查：合法「实参 < 形参」0 次；「实参 = 形参 + 1」10 站点全为 sret
- [x] 缓存结构：resolver 预填（含急切合并的 z42.core）/ cross_cell / jit tier3 IC / PIC —— 首次绑定点清单

## 实现
- [x] 编译器：`IrGenFacts.MethodFlagSret = 8`；`FunctionEmitter` 在 `RetIsStruct` 时置位；`IrGen*Emitter` 三处改按位或
- [x] 运行时：`METHOD_FLAG_SRET`；`symres::call_arity` / `wrong_arity_exception`（替换 `ctor_arity` / `wrong_ctor_arity_exception`）
- [x] 构造器 5 个调用点改传物理实参数（旧函数内部 +1 的口径迁到调用方）
- [x] resolver Pass 2：签名对不上不预填 token
- [x] interp `Call`：token 未命中写回前 / back-compat 分支 / cross_cell 填充前 / 惰性回落
- [x] `resolve_vcall` 出口判定 → `VCallTarget::Thrown`；`install_ic` 对不上不装 PIC；两后端消费 `Thrown`
- [x] JIT `jit_call` tier 3 写 IC 前（按名取 Function，不依赖 FnEntry）；`cross_zpkg_via_interp` 两种情形
- [x] 误删的 `ambiguous_function_exception` / `ambiguous_type_exception` 从 HEAD 逐字恢复（替换区间穿插所致）
- [x] 顺带：z42b `builder_commands.z42:69` 少传 `noBuild`（判定上线即抓出的真 bug）→ 显式 `false`

## 格式 bump（version-bumping.md 步骤 1–9）
- [x] 1/6 z42 writer 常量 1.40 / 0.45；2/7 Rust reader 常量 + changelog 注释；**钉值单测**（上次补进清单的那一步）
- [x] 3/8 `zbc.md` / `zpkg.md` changelog；版本常量表
- [x] 5 golden hex 单测 header minor 0x27→0x28（语料无 sret 函数）
- [ ] 4 regen zbc-format fixture ×6 —— 需 CI 新格式工具链
- [ ] 9 regen zpkg-format fixture ×4 —— 需 CI 新格式工具链

## 门
- [x] `symres_tests.rs`：7 条 `call_arity` 单测（含 sret 位、下界不读 min_arg、params 无上界）
- [x] 0.44 本地：cross-zpkg interp/jit 各 49/49（含两条 skew 修复后通过）；全量 `cargo test` 1370/0
- [x] 已知 flake `concurrency-null-thread-flake`：`z42.net` threaded 用例在本分支单跑 11/12，失败栈与 main 上历史复现逐帧一致；arity 判定命中 0
- [ ] 0.45：CI 工具链 overlay → fixture 重生 → 完整 `xtask test` + 自举不动点 + 全量 `cargo test` + cross-zpkg 两后端

## 收尾
- [x] book `runtime/missing-symbol-resolution.md`「签名对不上」一节重写；cross-zpkg README 登记
- [ ] 归档随 PR；登记编译器缺诊断（普通调用实参不足不报错）
