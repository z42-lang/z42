# Tasks: 静态初始化统一到类型初始化器

> 状态：🔴 DRAFT 待 User 过 gate | 创建：2026-09-22 | 类型：`lang` + `vm`（完整流程）
> 前置：阶段 6.5 须 User 明确说"没问题 / 可以开始"后方可动代码。

## 阶段 7.0：实施前置调查（**必须先做完，结论可能改变方案**）

- [x] 0.1 **用当前 main 自建的编译器 + VM** 复现 spec 场景 4 的"改前行为"（2026-09-22 完成）
      —— 确认为**静默错值**（`A.X = 1`，引用类型为 `null`），三组对照见 spec.md 场景 4。
      注：已安装 SDK（build-date 2026-09-06）早于 add-static-constructors (2026-09-12)，
      其上的结论一律作废，见 [[local-xtask-runs-old-vm-blindspot]]
- [x] 0.2 扫描全仓静态初始化器的跨类型依赖（2026-09-22 完成，扫描器带正/负对照验证）：
      **1011 文件 / 369 条初始化器 → A 类（同文件依赖声明在后）0 处，B 类（跨文件同包
      依赖 stem 字母序在后）0 处**。分母很小：**只有 3 条**初始化器在表达式里引用了别的
      类型，全部是 `Double.NaN/±Infinity = BitConverter.DoubleFromBits(...)`，而
      `BitConverter` 无静态字段 ⇒ 无序依赖。
      **结论：本变更的顺序语义改动在仓内受害点为 0，行为回归风险 ≈ 0。**
- [x] 0.3 统计受影响类型数（2026-09-22 完成）：含静态初始化器的**文件 70 个 → 类 80 个**，
      函数条目净增 **+10**。体积影响可忽略（此前担心的"一文件 3–5 类"不成立）
- [ ] 0.4 确认上一版 VM 遇到"更多类挂 $Cctor 哨兵"不炸（自举前提，零格式 bump 但语义面扩大）
      - [ ] 0.4.0 **前置：核对种子与源码的格式版本一致**（`.z42/libs/*.zpkg` 头偏移 6..8
            的 minor vs `ZpkgWriter.Minor`）。不一致时先 `./scripts/install-z42.sh --force`。
            ⚠️ **无 `--force` 的 `install-z42.sh` 会在种子落后时谎报 "already up to date"**
            （2026-09-22 实证：种子停在 09-06/0.48，实际 nightly 已是 09-21/0.49）。
            种子过期时一切自举结论无效，见 [[verify-conclusion-after-reseeding]]
- [x] 0.5 基线量测（2026-09-22，基准 `b82282d93`，种子 0.6.0/zpkg 0.49，**串行测量**）：
      - hello 启动：默认 **10.2 ms ± 0.5**，interp **10.3 ms ± 5.2**（60 runs）
      - z42c 编译 z42.core（冷 cache）：**1.906 s ± 0.044**（8 runs，σ 2.3%）
      ⚠️ 首次测量因 bootstrap 并发抢 CPU 得到 `3.681 s ± 1.237`（range 2.24–5.55），已作废。
      **量测必须串行**，见 [[measure-before-optimizing-and-nohup-trap]]

> **实施位置**：worktree `z42-staticinit`，分支 `unify-static-init-into-cctor`。
> ⚠️ 绝不在主树 `z42-test` 上做（§0 铁律；2026-09-22 违反过一次，代价见 design.md §5.1）。

## 阶段 7.1：编译器 —— 绑定与发射统一

- [ ] 1.1 `DeclBinder.z42`：删二分支，静态字段初始化器一律注入宿主类型初始化器体首
- [ ] 1.2 `DeclBinder.z42`：静态 auto 属性初始化器同上（`:232-236`）
- [x] 1.3 `SemanticModel.z42`：加 `HasStaticInitFor`，消费端按 `SiCls` 分组
- [ ] 1.4 `FunctionEmitter.z42`：无显式 cctor 但有初始化器的类 → 合成类型初始化器
- [ ] 1.5 `FunctionEmitter.z42`：删 `EmitStaticInit`（含"不带 DBUG 行表"约定）
- [x] 1.6 `IrGen.z42`：删 "static_init 首位" 特判，改为按类声明序发射 per-class 类型初始化器
- [ ] 1.7 `IrDump.z42`：删 SA-3 `SourceStem` 设置
- [ ] 1.8 `AccessEmitter.z42`：静态 struct 字段装箱统一走 `_emitStaticStore`
- [x] 1.9 `ClassDescBuilder.z42`：`$Cctor` 哨兵挂载扩到合成 cctor，判据取自 IrGen 的实际发射记录

## 阶段 7.2：编译器 —— 屏障消除位（对标 `CtorKnownFixup`）

- [ ] 2.1 IR 指令加 `owner_init_free` 位（StaticGet / StaticSet / Call / ObjNew 站点）
- [ ] 2.2 新 pass（或并进 `CtorKnownFixup` 的同一趟遍历）：**整包装配后**重算全部站点
- [ ] 2.3 **跨包站点一律不置位**（D4）；只对本包类型置位
- [ ] 2.4 **每次装配重算**，不是只置位 / OR（增量安全，spec 场景 11）
- [ ] 2.5 zbc 往返：置位态必须 round-trip（镜像 `zbcreader_tests.z42:59` 的 `CtorKnown` 做法）

## 阶段 7.3：运行期 —— 状态搬到 TypeDesc

- [ ] 3.1 `type_desc.rs`：`TypeDesc` 增 `init_state: AtomicU8`
- [ ] 3.2 `cctor.rs`：状态机改读写 `TypeDesc`；删全局 `pending` 计数门与 `Mutex<HashMap>`
- [ ] 3.3 `cctor.rs`：保留 `Failed` 文案旁挂表（少见路径）
- [ ] 3.4 `statics.rs`：静态读写屏障改用预解析缓存的 owner `TypeDesc`
- [ ] 3.5 `exec_call` / `jit_call`：静态调用屏障同上
- [ ] 3.6 屏障尊重 `owner_init_free` 位：置位站点整条不执行
- [ ] 3.7 `loader/type_registry.rs`：登记点统一到 `build_type_registry` 单一漏斗

## 阶段 7.4：运行期 —— 删除 `__static_init__` 平行管道

- [ ] 4.1 `lazy_loader/registry.rs`：删后缀扫描 `ends_with(".__static_init__")`
- [ ] 4.2 `lazy_loader.rs`：删 `pending_static_inits` / `static_init_state`
- [ ] 4.3 `lazy_loader/resolve.rs`：删 `InitState::Claimed` 窗口逻辑
- [ ] 4.4 `vm_context/types.rs`：删 `running_static_inits` 等计数字段
- [ ] 4.5 `vm_context/lookup.rs`：`run_pending_static_inits` 塌缩为单队列
- [ ] 4.6 `interp/entry.rs`：`init_static_fields` 塌缩
- [ ] 4.7 `jit/mod.rs`：镜像塌缩；删 `collect_lazy_static_init_names`
- [ ] 4.8 `well_known_names.rs`：删 `METHOD_STATIC_INIT`
- [ ] 4.9 `metadata/resolver.rs`：排空点调整
- [ ] 4.10 `host/ops.rs` / `host/state.rs` / `boot.rs`：宿主加载路径与启动步骤表同步

## 阶段 8：验证

- [ ] 6.1 spec 场景 1–11 全部落成 fixture 并通过
- [ ] 6.2 **场景 8 的会变红的门**：断言产出 zpkg 的 SIGS 段无 `.__static_init__` 后缀函数
- [ ] 6.3 `xtask test` 全绿（interp）
- [ ] 6.4 `xtask test stdlib --mode jit`（屏障改在派发面，**必跑**）
- [ ] 6.5 `cargo test` **全量**（非只 `--lib`）
- [ ] 6.6 cross-zpkg：`static_init_cross_pkg` / `static_init_concurrent` 不回归
- [ ] 6.7 `xtask test bootstrap` + 冷启动自建（多层跨成员符号收敛，
      [[bootstrap-test-misses-multilevel-symbols]]）
- [ ] 6.8 性能 A/B 对 0.5 的基线；被 bench 门判红时按 [[investigate-micro-ab-false-regressions]]
      做字节对账证伪
- [ ] 6.9 `xtask test lines`（超限文件只能缩不能涨）

## 阶段 9：文档与归档

- [ ] 7.1 `docs/reference/src/language/static-constructors.md`：统一叙述；**删**「加 cctor
      会把初始化从启动时变成首次使用时」的行为悬崖警告（接缝消失）
- [ ] 7.2 `docs/reference/src/language/static-members.md`：静态字段初始化时机
- [ ] 7.3 `docs/internals/src/runtime/static-ctor-init.md`：屏障从全局门改为 per-TypeDesc；
      删双管道叙述；补编译期屏障消除位一节
- [ ] 7.4 doc-check 三道门逐项核对（[doc-system.md](../../../agent/rules/doc-system.md)）
- [ ] 7.5 归档到 `docs/spec/archive/YYYY-MM-DD-unify-static-init-into-cctor/`

## 后继（不在本变更）

- `add-module-init-hook`：`[ModuleInit]` → 合成 `<pkg>.$Module` 伪类型的类型初始化器
  （C# 的 module initializer 本就是 `<Module>` 伪类型的 `.cctor`）。建在本变更之上。
