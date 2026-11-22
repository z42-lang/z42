# Runtime 渐进式 PR 计划

本计划基于 [审计](runtime-audit.md) 和 [目标架构](runtime-target-architecture.md)，当前仅为计划，不授权本轮修改 Runtime 实现。每个 PR 基于届时最新 main 再核对文件与契约；若证据不足，缩小或取消该 PR，而不是机械执行。所有行为不变 PR 均保持初始化、同步、错误、对象布局和对外 API；完整 `./xtask test` 是仓库最终 GREEN 门禁。各 PR 需独立编译、审查、测试及回滚。

| 顺序 | 优先级 | 单一目标 | 前置依赖 |
|---|---|---|---|
| 1 | P0 | 消除 VM 装配对 Host 程序类型的反向依赖 | 无 |
| 2 | P0 | 消除 VM 默认服务对 Host 实现的反向依赖 | 1（统一装配输入） |
| 3 | P0 | 固化失败/生命周期不变量并拆出私有接线 | 1、2 |
| 4 | P1 | 分离可证明的纯产物准备 | 1；按复核结果决定是否进行 |
| 5 | P1 | 补齐共享语义的差分契约 | 无；可与 2–4 独立 |
| 6 | P1 | 按已证实差异收敛非热路径共享语义 | 5；仅有具体差异时实施 |
| 7 | P1 | 组件能力和生命周期边界验证 | 无；如暴露缺陷单独修复 |
| 8 | P2 | 按真实消费者收口公开面和文件命名 | 1–4，逐项依赖兼容审计 |

## PR 1：解开程序装配与 Host 的类型环（推荐第一个）

- 文件：`src/runtime/src/vm/assembly.rs`、`src/runtime/src/host/load.rs`、`src/runtime/src/host/program.rs`，必要时同目录现有测试。
- 变更：以 crate-private 已解析输入替代 `LoadOptions` 依赖，以内部装配结果替代直接返回 `LoadedProgram`；宿主适配器转换选项并包装结果；在宿主侧获取同一时点的 JIT 配置后传入。保留 `ProgramInputs` 所有权、准备/attach/事件顺序及错误传播；不搬文件、不改公开 host 类型。
- 验证：Rust host 文件/bytes、C 缓存加载和 CLI 集成；`host::program_tests`、`components::component_tests`、`tests/cli_loading.rs`、`tests/engine_components.rs`；包含成功和组件准备失败的回归。运行无默认 feature 构建及完整 `./xtask test`。
- 验收：生产 `vm/assembly.rs` 不再引用 `crate::host`，宿主公开签名不变；现有测试均通过；`git diff` 可看出加载、初始化和事件顺序未变化。JIT 配置读取时点若无法原样保持，先拆为独立 PR，不随意改变进程配置冻结语义。

## PR 2：宿主服务显式注入

- 文件：`vm/services.rs`、`vm/defaults.rs`、`vm/assembly.rs`、`host/services.rs`、`host/app_state.rs`、相关调用方及测试，具体以 PR 复核为准。
- 变更：用已有 `VmExtensions` 注入宿主服务和应用状态；确保所有构造入口，包括 `VmContext::new/with_module` 的既有默认行为保持不变。若默认构造仍需组合根，移动具体接线到高层，而非让低层读取 host；不新增通用 service locator。
- 验证：`vm/defaults_tests.rs`、`host/app_state_tests.rs`、`host/app_tests.rs`、多 VM 和嵌入调用，检查自定义扩展覆盖与 drop 顺序；完整 GREEN。
- 验收：VM 机制和默认服务组装不再硬编码 `crate::host`，CLI、Rust、C 及默认构造行为等价；没有扩展持有强 Core 环。

## PR 3：生命周期不变量与接线分离

- 文件：`vm/mod.rs`、可新增 `vm/wiring.rs`、`vm/tests.rs`、`components/component_tests.rs`、`gc/heap_tests/multi_vm.rs`、`threads/roots_tests.rs`；测试具体增量由缺口决定。
- 变更：先为部分构造失败、组件 attach 失败与正常销毁加入可观测测试；随后只把 GC observer/roots/reclaimer 接线抽为私有函数。保持 Arc/Weak、pin 与注册位置、字段顺序、逆序释放和 rollback 时序。
- 验证：相关单测、多线程 roots/GC、`tests/gc_registration_race_loom.rs`、`tests/cross_thread_smoke.rs`、组件测试及完整 GREEN；loom 在项目配置支持时执行，不能以普通测试冒充 loom。
- 验收：构造/接线职责能单独阅读，故障和正常路径资源次数平衡；无 ABI、偏移、锁顺序或可观测行为变化。测试若无法安全构造故障注入，先交付测试调查而不声称已证明回滚。

## PR 4：收口产物准备（条件性）

- 文件：`host/load.rs`、`loader/` 中新增或现有准备模块及对应测试、`vm/assembly.rs`。
- 变更：只迁移不依赖宿主实现的依赖候选、包准备或索引机制；resolver 的宿主适配仍在 host，eager/merge 两轨和来源选择不变。先列出每项函数输入及其策略所有者；无法定义清晰边界则取消该迁移。
- 验证：`loader/package/loader_tests.rs`、`loader/lazy/*tests.rs`、`tests/cli_loading.rs`、host 解析器/缓存用例，覆盖 file/bytes、VFS、resolver、lazy、eager 和 `Z42_EAGER_MERGE` 两轨；完整 GREEN。
- 验收：准备机制不引用 host 类型，路径优先级、事件、失败和包去重语义不变。

## PR 5：解释器/JIT 语义差分测试

- 文件：`crates/z42-interp/src/interp/ops/` 和 `crates/z42-jit/src/jit/` 现有测试，或现有跨后端集成测试。
- 变更：选择真实风险点如除零、混合数值、数组写屏障、异常和初始化；同一输入分别运行解释器及 JIT，比较结果、异常和可观察堆状态，不以复制实现为测试预言。
- 验证：两模式运行、目标 feature 构建、完整 GREEN；无 JIT 平台明确跳过且另有支持平台执行结果。
- 验收：每个新增场景有明确语义与边界，覆盖 fast/slow 路径；不改变执行实现。

## PR 6：仅修复已证实语义分叉

- 文件：按 PR 5 暴露的具体问题确定，如 `exec/ops/scalar.rs` 和对应 interp/JIT helper；不预设大规模移动。
- 变更：纯规则共享，保留寄存器/frame 管理与 JIT 内联 lowering；涉及行为修复时单独描述契约和适用范围。
- 验证：PR 5 差分、对应单测、完整 GREEN；影响热路径时增加同机基准和基线/变更数据。
- 验收：差分问题消除、无不必要的动态分发、性能门禁通过；无已证实分叉则不创建此 PR。

## PR 7：组件边界验证

- 文件：`components/api.rs`、`components/instance.rs` 对应测试、`tests/engine_components.rs`；仅在测试需要时添加注入点。
- 变更：验证无副作用查询、角色缺失或歧义、实例创建/attach 失败、逆序销毁与 code lease；与真正发现的 bug 修复分开审查。
- 验证：静态链接、共享链接、运行时动态装载分别按可用平台运行，检查禁用策略、组件 core identity 和后端存活；完整 GREEN。
- 验收：无逐指令组件查找，失败不发布可调用的半成品；不能覆盖的平台列出而非推断通过。

## PR 8：公开面与文件组织（按领域分批）

- 文件：依据消费者清单选取 `lib.rs`、相关领域 `mod.rs`、内部实现、文档及编译测试；例如 `vm/mod.rs` 与已拆出的 wiring，而非批量改名。
- 变更：按所属领域收口实现路径、拆分职责明确的文件；保留外部宿主公共 API，组件 bridge 的同构建消费者单独处理。
- 验证：正向宿主/组件消费者、内部路径 compile-fail、C 头和 SDK 打包（适用时 `./xtask package sdk --verify`）、完整 GREEN。
- 验收：每个调整有列明的真实消费者和依赖边变化；不因名称或引用数量而拆分，未验证公开符号不删除。
