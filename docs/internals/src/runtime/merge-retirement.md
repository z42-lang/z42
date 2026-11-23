# merge 退役——per-Assembly 身份模型设计

> 代码：`src/runtime/src/loader/package/merge.rs`（退役对象）、`src/loader/lazy/`（目标机制）、
> `src/exec/ops/const_string.rs`（双路径先例）、`src/metadata/ids/`（全局身份表）
> 状态：设计稿（M0）。前置：会话定义共享 #1836 已合（ImageCache）；token 家族下移与缺口表收官在途（#1840/#1841）。

## 1. 问题

M0 时的 `host::load::prepare` 把入口产物与全部依赖（标准库居首）`merge_modules` 成**一个扁平 Module**：

- 字符串池拼接 + ConstStr 重映射（merge 自述的唯一复杂点）；
- 类/函数按名幂等合并（last wins）；
- 扁平模块 = VM 的世界：单一 type_registry / func_index，`MethodId` 恰为扁平索引。

代价：跨包边界消失（#1836 已补救定义内存）、版本冲突被 last-wins 静默吞掉、
eager 与 lazy（`loader/lazy`）两套装载语义并存。

## 2. 存在性证明：惰性路径已是完整 per-Assembly 运行机制

| 能力 | 惰性路径现状 | 证据 |
|---|---|---|
| 函数全局身份 | `FuncTable::register_lazy(Arc<Function>) -> FnId`，不在扁平模块内 | `metadata/ids/func_table.rs:144` |
| 类型全局身份 | TypeTable 注册、VCallIC 的 TypeId 本就是 VM 级 | `metadata/ids/type_table.rs` |
| 字符串 | **模块自己的池**：非入口模块 ConstStr 走 `load_foreign` | `exec/ops/const_string.rs:12` |
| 跨包解析 | `resolve_function_tokens` 的 by_fn_id 分支 + `bind_callee` 冷路径 | `metadata/resolve.rs` |
| 装载编排 | `register_loaded_artifact`：依赖候选、符号归属、模块注册 | `loader/lazy/registry.rs:267` |

**结论**：merge 退役 = 让 eager 路径走惰性注册流（每个 zpkg 各自注册，入口模块只是
"第一个注册的"），`merge_modules` 与 ConstStr 重映射删除。

## 3. 难点与对策

1. **入口索引不变量**（`method_tokens` 文档："entry functions are 0..n = Module.functions 索引"）。
   对策：保持注册顺序不变量——入口模块最先注册，其全局 FnId 仍为 0..n；interp 的
   "in range → module.functions[t]，else FuncTable" 双读逻辑保留且语义不变。
2. **类型表与继承补全**。现状 `build_type_registry(&mut Module)` 在扁平模块上建表并
   `try_fixup_inheritance` 跨包补 base 字段。per-Assembly 后各包类型进 TypeTable（全局
   TypeId 由注册分配），跨包继承改走注册期补全（lazy 路径的 `enqueue_type_load` /
   `has_loaded_type` 机制已有）。**这是风险最集中的一步**。
3. **同名冲突语义**。merge 的 last-wins 变为注册冲突——`register_lazy` 的幂等性需要
   显式语义：同名同签名幂等（#1836 内容寻址保证同字节同定义），同名异签名报错。
4. **resolve 双分支统一**。`by_fn_id` / `module.func_index` 两分支收敛为全局 id 单分支
   （简化，但触碰热路径取 token 逻辑，需 bench 对账）。
5. **自举与测试链**。xtask 两代自举、stdlib 逐包字节对账、e2e golden 全走 prepare——
   每个 M 阶段都要过全量门。

## 4. 分阶段实施

| 阶段 | 内容 | 风险 |
|---|---|---|
| M0 | 本设计文档 | — |
| M1 | prepare 双轨：默认仍 merge，`Z42_NO_EAGER_MERGE` 开关切逐包注册（复用 `register_loaded_artifact`），CI 全量门 + 可选 bench A/B | 中（**已落地**：入口模块单独走四道装配 pass，依赖工件经 `register_eager_artifact`→`register_decoded_artifact` 复用惰性漏斗；smoke=embedding hello 开关路径端到端，CI 执行） |
| M2 | ~~类型注册迁移~~ 实测缩窄：#1852 的归属触发修复后，stdlib 448 + 自举不动点全过——原判「风险最高」的类型/继承面**已被惰性注册机制覆盖** | 已大幅缩窄 |
| M3 | resolve 双分支统一（全局 id 单分支）+ interp/JIT 取 token 对齐；bench 对账 | 中 |
| M4 | 翻默认已合（#1855，M3 #1885 找回性能）＋三个 nightly 观察期满 → **终刀已落**：prepare 侧合并臂与 `Z42_EAGER_MERGE` 逃生口删除，装载只剩逐包轨道；`merge_modules` 收窄为 **zpkg 包内装配**（一个 zpkg 内部多编译单元合并 = 格式语义，`assemble_zpkg_artifact`），不再是装载策略 | 已完成 |

每阶段独立 PR、可回退（M1 的开关即回退通道）；M2 若发现继承补全语义不可收敛，
设计回到此文档修订而不是硬闯。

## 4b. 开关路径扩面验证战报（M2 进行中，11-19）

`Z42_NO_EAGER_MERGE=1` 下的全量测试矩阵：

| 套件 | 规模 | 结果 |
|---|---|---|
| e2e goldens | 906 | ✅ 0 失败 |
| e2e cross-zpkg | 106 | ✅ 0 失败（修复 4 个 module_init 后，见下） |
| e2e multi-exe | 3 | ✅ |
| **stdlib 全量** | **448 文件 / 26 库**（泛型/继承/反射/集合/IO 深水区） | **✅ 0 失败** |
| **compiler 含 z42c 自举不动点** | **13/13 包 gen1==gen2 字节一致** | **✅** |
| 微基准（启动+执行，debug） | fixture 双轨 | 无回退（39ms vs 34ms/次） |

module_init 4 红的根因与修复已合（#1852）：带 sym 的初始化屏障改为只触发
**拥有该符号的包**（`pending_module_owns` 归属判定）——合并轨同步收紧为文档
承诺的触达语义，非行为变化。

含义：per-Assembly 装载路径通过了仓库现有的**全部**正确性验证。剩余门槛是
M4 前的 CI 级 bench 对账（稳态派发成本）与 M3 的取 token 双分支统一（简化，
非前置条件）。

### 尾项裁决（收尾审计，11-22）

- **string_pool 跨 VM 共享**：**单 VM 进程（实际部署形态）下问题不存在**——字符串池
  只有一份。它只出现在 C 会话多次 `load_zbc`（每模块各建 VM）的 API 允许形态里；
  即便那时，实测机制也只是 `StrTable::append` 逐条重装箱（MB 级以下）且堆 `Str`
  本就惰性驻留。**裁决：非问题收档**；若未来出现多模块 C 会话的高密度驻留部署
  再议（`Arc<str>` 贯穿三层的方案在案）。
- **C 适配分域**：触发条件维持（第二个 C 面或显著超出五文件）；当前单 C 面
  集中归组可发现性最优，预做分散为负收益。

## 5. 与既有工作的关系

- #1836 ImageCache：per-Assembly 注册的消费方天然是它（按内容共享定义），M1 直接受益；
- load-context 模型（contexts.rs）：root context 的"扁平合并模块"描述随 M4 更新；
- ALC 共享语义不变：同会话已加载 zpkg 按内容共享定义，per-Assembly 让"共享"精确到
  注册条目而非合并产物。
