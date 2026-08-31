# Tasks: 优化 is_subclass_or_eq_td（去分配 + memo）

> 状态：🟡 进行中 | 创建：2026-08-31 | 类型：perf（VM 行为不变——is/as/catch/vcall 判定逐一相同）

**变更说明：** `is_subclass_or_eq_td`（`interp/dispatch.rs`）是 z42 `is`/`as`/`catch`/vcall 分派的核心。原实现每次检查 `derived.to_string()` + 沿基类链每级 `base.clone()` 分配 String，且 caller 的 `module.type_registry` 常不含跨-zpkg 类型（如 z42c 序列化 z42.ir 的 `IrInstr` 子类）→ 每级落 `try_lookup_type`（lazy_loader 锁）。z42c 的 zpkg 序列化把每条指令过 ~60 路 `is`-链 → 本函数是解释执行头号热点（profile 实证 17.6s 写段全在此）。

改：① **memo** `(derived,target)→bool`（per-VmContext 嵌套 map，hit 按 &str 零分配）——关系是全局单调事实（已加载类型的基/接口链不变、lazy-load 只增），可缓存；② **alloc-free walk**（持 `Arc<TypeDesc>` 跨迭代、按 &str 跟 base_name，去 per-level String 分配）。

**原因：** 相位插桩：zpkg 序列化 17.6s（占单包编译 51%）全是本函数的 is-链开销。见 [[compiler-parallel-heavy-phases-investigation]]。

**文档影响：** 内部 VM 优化，行为不变。目录 README 无需动；机制可注记 book runtime 页（若有 dispatch 页）。

## Scope
| 文件 | 变更 | 说明 |
|------|------|------|
| `src/runtime/src/interp/dispatch.rs` | MODIFY | `is_subclass_or_eq_td` memo 包装 + `is_subclass_or_eq_td_walk` alloc-free |
| `src/runtime/src/vm_context/types.rs` | MODIFY | 加 `subclass_memo: Mutex<FxHashMap<String,FxHashMap<String,bool>>>` 字段 |
| `src/runtime/src/vm_context/construct.rs` | MODIFY | 两处构造初始化 `subclass_memo`（镜像 interned_cache） |
| `src/runtime/src/vm_context/lookup.rs` | MODIFY | `load_module_into_vm`/`load_module_bytes_into_vm` 清 `subclass_memo`（REPL 重定义安全） |
| `src/runtime/src/interp/dispatch_tests.rs`（或就近 *_tests.rs）| NEW/MODIFY | 单测：memo hit==uncached、跨-zpkg 基类、接口传递、清 memo；正确性等价 |

## 阶段 1: 实现
- [x] 1.1 `types.rs` 加 `subclass_memo` 字段
- [x] 1.2 `construct.rs` 两处初始化
- [x] 1.3 `lookup.rs` load_module_* 清 memo
- [x] 1.4 `dispatch.rs` memo 包装 + alloc-free walk
- [ ] 1.5 `cargo build --release` 无错误/警告回归

## 阶段 2: 验证
- [ ] 2.1 单测：is/as/catch 跨-zpkg（TestFailure）+ 接口传递 + memo 一致性（memo 结果 == 直接 walk）
- [ ] 2.2 `cargo test`（全 targets）全绿——尤其 reflection（IsAssignableFrom）/ cross-zpkg catch
- [ ] 2.3 性能：nightly z42c 跑本 vm 编 z42c.semantics，序列化写段墙钟大幅下降（目标 17.6s→?）
- [ ] 2.4 **byte-identical**：产物与 baseline 逐字节一致（is-check 只影响速度不影响 codegen 输出）
- [ ] 2.5 完整 GREEN：`xtask test`（e2e/cross-zpkg/stdlib/compiler/vscode-syntax）+ 自举 gen1==gen2
- [ ] 2.6 文档同步（若触发矩阵命中）

## 备注
- **正确性核心**：memo 只在类型关系稳定时有效。lazy-load ADD 单调不清；显式 load_module_*（REPL 重定义）清 memo。cross-zpkg catch（TestFailure）+ reflection IsAssignableFrom 是必过回归。
- 与 ByteWriter 优化（[[compiler-parallel-heavy-phases-investigation]]，另一 change）叠加；本 change 是大头。
- 测量配方见 [[compiler-parallel-heavy-phases-investigation]]。
