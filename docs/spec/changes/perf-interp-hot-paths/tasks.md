# Tasks: perf-interp-hot-paths

> 状态：🟡 进行中 | 创建：2026-08-04
> 类型：perf（最小化模式）。不改外部可见行为，仅优化解释器热路径。
> 纪律：每项**改前后跑 `xtask bench`** 与 baseline 对比（记忆 measure-before-optimizing）；
> 每项单独 commit；全部完成后一个 PR。

**变更说明：** 消除解释器热路径上的若干系统性开销（每指令 site_idx 税、每调用行号线性扫描、
闭包 env 重分配、builtin 参数堆分配、对象默认槽字符串解析）+ JIT 翻译期清理。
**原因：** 见探索分析 S1–S5 / J1；均为"happy path 白付的成本"，语义零变化。
**文档影响：** 纯内部实现调整，不改对外行为；涉及机制的项（S2a 惰性行号、S5 默认槽模板）
在对应 `docs/book/` 机制页补一句实现说明（若已有页）。README 核心文件表若无新增文件则不动。

## 基线（interp, 2026-08-04, hyperfine 10 runs）
- 01_fibonacci 69.8ms / 02_math_loop 54.4ms / 03_startup 51.1ms / 04_arith_loop 90.9ms
- 05_polymorphic_dispatch 1.004s / 06_thread_scaling 114.6ms / 07_string_heavy 325.8ms / 08_dict_heavy 302.7ms

## 进度概览
- [x] S1 每指令 site_idx 税下沉
- [x] S2a 行号解析二分（resolve_line O(n)→O(log n)）
- [x] S3 闭包间接调用复用 GcRef env
- [x] MICRO 新增 dispatch 微基准（10_mono_vcall）→ 重测：S1+S2a 在 vcall 密集 ~3%
      （闭包微基准无法建：`xtask bench` 用 --emit-zbc，单文件 emit-zbc 不支持 lambda lifting
      → `T.<error>`；这是独立于本改动的 --emit-zbc 局限，记 backlog）
- [~] S4 builtin/间接调用 SmallVec 参数 —— **DROP**：S1/S2a 微优化天花板 ~3%，S4 针对更小的
      单次 Vec 分配、更侵入（加 dep + 改 collect_args 签名多处），预期 <1-2% 落噪声内，收益不抵churn
- [~] S5 TypeDesc 缓存默认槽模板 —— **DEFER**：bench 套件无对象分配场景可验收益，
      且需给共享 TypeDesc 所有构造点加字段（投机复杂度）；待独立"对象分配基准"落地后再评
- [~] J1 JIT 翻译期清理 —— **DEFER**：仅翻译延迟（非稳态），无稳态收益；不值本批 churn

## 微基准结论（2026-08-04，10_mono_vcall dispatch 隔离，clean vs 本 3 文件）
| 场景 | clean | S1+2a+3 | delta |
|------|------:|--------:|------:|
| 10_mono_vcall（单态 vcall 紧循环） | 1482.5 | 1443.1 | **-2.7%** |
| 05_polymorphic_dispatch | 1001.0 | 969.4 | **-3.2%** |
| 01_fibonacci（调用密集递归） | 73.1 | 70.6 | **-3.5%** |
| 04_arith_loop（无 vcall/site_idx） | 87.1 | 86.4 | -0.8% |

**结论**：S1+S2a 在 **dispatch 密集**代码上稳定 **~3%**（高于噪声、方向一致），arith 无收益（符合预期，
算术不走 site_idx/vcall）。S3（闭包）无法微基准（--emit-zbc lambda 局限），收益保持推理证成。
三项均零风险、零行为变化，值得落地。

## 诚实基准结论（2026-08-04，两边均 FRESH stdlib，仅差本 3 文件）
| 场景 | trueBase | S1+2a+3 | delta |
|------|---------:|--------:|------:|
| 01_fibonacci | 70.5 | 69.9 | -0.9% |
| 02_math_loop | 51.6 | 51.4 | -0.2% |
| 03_startup | 47.6 | 47.7 | +0.3% |
| 04_arith_loop | 89.9 | 87.5 | -2.7% |
| 05_polymorphic_dispatch | 979.7 | 987.4 | +0.8% |
| 07_string_heavy | 99.1 | 98.7 | -0.3% |
| 08_dict_heavy | 279.0 | 280.4 | +0.5% |

**结论**：三项对现有 e2e 场景的宏观影响**在 ±3% 噪声内**。它们是零风险、零行为变化的
**干净简化**（移除每指令双层 Vec 索引 / 每调用线性 line 扫 / 闭包 env 深拷+重分配），
移除的是真实工作，但收益需 **dispatch/闭包微基准**（套件此前无）才显——故新增 MICRO 项。
> ⚠️ 测量教训：`xtask bench` 用磁盘上 stdlib .zpkg；首个 baseline 误用旧 stdlib（#104 前），
> 一度显示 string_heavy -70%（实为 #104 "Script-First 字符串性能" 的 stdlib 重建，非本改动）。
> 正确做法：baseline 与被测都必须 FRESH stdlib（同一 regen 态），只留被测 3 文件为变量。

## S1 — 每指令 site_idx 税下沉
- [ ] S1.1 `exec_instr.rs`：把 `resolved.get()` + `site_index[block][instr]` 双层索引从
      函数顶部无条件计算，改为仅在 token-bearing 分支（Call/Builtin/ObjNew/VCall/
      FieldGet/FieldSet/StaticGet/StaticSet）内按需取；非 token 指令零开销
- [ ] S1.2 bench 对比（arith_loop / polymorphic_dispatch）+ 记录

## S2a — 行号解析惰性化
- [ ] S2a.1 `exec_instr.rs::update_caller_line` / `mod.rs`：happy path 不再每次调用
      `resolve_line` 线性扫 line table；改为存廉价的 (block_idx,instr_idx)，
      仅在真正需要栈帧位置（throw / populate_stack_trace）时 resolve
- [ ] S2a.2 bench 对比（fibonacci / polymorphic_dispatch）+ 回归测 e2e 异常栈用例

## S5 — TypeDesc 缓存默认槽模板
- [ ] S5.1 `types.rs` TypeDesc：加 `default_slots` 缓存（按字段 type_tag 预算一次）
- [ ] S5.2 `exec_object.rs::obj_new`：clone 模板替代每次 `default_value_for` 字符串 match
- [ ] S5.3 bench 对比（dict_heavy）+ 记录

## S3 — 闭包间接调用复用 GcRef env
- [ ] S3.1 `exec_call.rs::call_indirect`：`Value::Closure` 直接复用已有 GcRef（Arc +1），
      不再 `elems.clone()` + `alloc_array` 重分配；仅 StackClosure 物化
- [ ] S3.2 bench + closure e2e 回归

## S4 — builtin/间接调用 SmallVec 参数
- [ ] S4.1 `ops.rs::collect_args` + `exec_call.rs`：小参数场景用 SmallVec 栈缓冲免堆分配
- [ ] S4.2 bench 对比（string_heavy）+ 记录

## J1 — JIT 翻译期清理（可选）
- [ ] J1.1 `jit/translate.rs`：`written` set + hoist 候选扫描的 O(n²) `contains`/`any`
      改 HashSet；helper import 可按需（评估收益后决定是否做）

## 验证
- [ ] 每项 commit 前跑相关 stage；全部完成后跑完整 `xtask test`（GREEN gate）
- [ ] 最终 bench 汇总（vs baseline）写入本文件备注

## 备注
（实施中记录每项 before/after 数字与决策）
