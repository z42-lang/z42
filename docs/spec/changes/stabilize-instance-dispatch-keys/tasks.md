# Tasks: 实例派发键稳定化

> 高风险、冷 CI 收敛型（上次全 mangle 挂 19 e2e golden、全在运行期实例派发、本地 warm 不可见）。
> 分阶段、support-先行；每阶段尽量本地可验，运行期最终以 CI 两代自举 + e2e golden 为权威门。
> 文件路径为**当前布局**（`converge-z42c-ir-metadata` 后 DependencyIndex/ZpkgWriter 已在 stdlib `z42.ir`）。

> **实际落地（2026-09-04，精炼版 primary/非-primary）**：下方阶段 1–2 的详细清单是**早先「全键 +
> 裸规范槽双登记」设计**的分解，已被 design.md「精炼版」取代——最终**未**做每方法双登记，改为
> **primary（声明序首个同名）保裸键 / 非-primary 取全签名 MangleKey**（additive、唯一方法零字节漂移）。
> 故那部分清单**不逐条执行**，实际落地收敛为下面 5 个修复点。机制文档见
> [source-compile.md「实例派发键稳定化」](../../../book/src/compiler/source-compile.md)。

## 🟢 实际落地（5 修复 + 格式双 bump）
- [x] `MemberCollector._fillClass`：primary 裸 / 非-primary 全键（`emittedInst` 跟踪首个同名）
- [x] **修 1**（段错误）：`MemberResolver` prim-wrapper 去 `_sameArity<2` 快路径 → 统一 `_resolveOverload` 取 `RegKey`
- [x] **修 2**（H4 vtable 塌槽）：`type_desc.rs derive_simple_method_name` 返回完整键、不剥 `$`
- [x] **修 3**（ctor 重载）：`OverloadBinder._ctorKey` 按 argCount 找非-primary ctor 的 `RegKey`
- [x] **修 4**（协议豁免重载）：`ToString`/`Equals`/… 也走 primary/非-primary（重载不再全裸 last-wins）
- [x] **修 5**（碰撞守卫）：`MemberCollector` `sigSeen` 按全签名自查 nullable 碰撞 → E0408（补 `DeclBinder` 新键下漏判）
- [x] 格式双 bump：zbc 1.37→1.38 / zpkg 0.42→0.43 + `versions.rs` 两常量 + `zbc_tests.z42` golden hex minor
- [x] 泛型 H3（generic-arity 进键）/ H5（composite FQN）：grep 证实无碰撞 → **降为前向守卫 / Deferred**，不为不存在的碰撞制造字节漂移
- [x] 本地验：disp 派发（Substring/用户重载/虚方法/foreach/Sort/ctor 重载/struct ToString 重载）**interp + JIT 双模式**全过；碰撞 3 例（E0408 / 2×0-error）符合预期；两代自举 gen2 建 xtask.zpkg debug-assertions VM 无段错误
- [ ] 权威门：CI `ci-bootstrap` 两代自举 + `bootstrap-no-csharp` + 全 golden regen 一致 + e2e golden 全绿（冷 CI 收敛，push 后盯）

## 阶段 0：环境 + 基线（已完成）
- [x] `z42-dispatch` worktree 供种（post-#379 nightly SDK → `.z42`）+ cargo 建 z42vm + 建 xtask.zpkg
- [x] 基线 `xtask test` 全绿存档（改前对照）；记录 5 子系统现有测试清单（见阶段 4 门）

<details><summary>早先「全键 + 裸规范槽双登记」设计的详细清单（已被精炼版取代，留档追溯）</summary>

## 阶段 1：编译器 —— 全键 + 裸规范槽（support，不改 z42c 源调用点）
键生成与登记：
- [ ] `src/compiler/z42c.semantics/src/OverloadResolver.z42` `MangleKey`/`TypeKey`：本变更**不改键格式**
  （H3 泛型 arity 编码 + H5 composite FQN 均 grep 证实当前无碰撞 → **降为守卫/Deferred**，避免为不存在的
  碰撞制造字节漂移）。MangleKey 保持现状（`name$arity$types`，composite 短名 + keyword 非对称）。
  - [ ] **守卫**：`MemberCollector` 加碰撞诊断——「同类同名同值-arity 的泛型+非泛型」→ 报 E-，不静默覆盖（H3 guard）
- [ ] `src/compiler/z42c.semantics/src/MemberCollector.z42` `_fillClass`：
  - [ ] 实例方法**恒登记全键**（去掉兄弟集三档决策对「注册键」的影响）
  - [ ] 计算并标记每 (owner,name) 的**规范槽方法**（virtual origin / 接口·协议 / 协议豁免 / 唯一非虚）
  - [ ] 规范槽方法**额外登记裸别名**（`ct.Methods.Put(bareName, sym)`，稳定序 first-wins）
  - [ ] `staticVirtual` carve-out（`:193-196`）改为「全键 + 裸规范槽别名」（op_* 可派发且稳定，H2）
- [ ] `src/compiler/z42c.semantics/src/InheritanceResolver.z42` `_passFixupOverrides`/`_findVirtualOrigin`：
  override 采纳 origin 的**规范槽 + 全键**；对齐比较用声明层签名（替换不变，H2）
- [ ] `src/compiler/z42c.semantics/src/SymbolCollector.z42` `IsProtocolExempt` + `src/libraries/z42.ir/src/DependencyIndex.z42`
  `_isProtocol`：合并为**单一 SoT**（6 名）（D5）
调用点 emit + 登记：
- [ ] `src/compiler/z42c.semantics/src/MemberResolver.z42` / `CallEmitter.z42` / `ExprEmitter.z42`：
  resolved 具体重载→emit **全键**；多态（虚/接口/协议/泛型参数接收者）→emit **裸规范槽**
- [ ] `src/compiler/z42c.semantics/src/ClassExtractor.z42`(TSIG 导出) / `IrGen.z42`(impl) / `TestIndexBuilder.z42` /
  `src/libraries/z42.ir/src/DependencyIndex.z42`(实例注册)：导出/注册**双标识**（全键 + 规范槽别名）
- [ ] `src/compiler/z42c.semantics/src/ImportedSymbolLoader.z42`：import 侧读全键 + 规范槽别名（替换今天的 bare first-wins hack）
本地验（阶段1）：`xtask test stdlib` + 定向 e2e（能 warm 跑的）；`xtask test bootstrap` 无越界

## 阶段 2：运行期 —— 无-vtable 路径探全键 + vtable 保 `$`（interp/JIT 镜像）
- [ ] `src/runtime/src/interp/exec_vcall.rs`：原始（`:159-198`,`:321-349`）/ 装箱-struct（`:204-252`）候选列表
  **增全键形态**（优先 VCall 携带的已决议全键，再回落裸规范槽 + `$arity`）（H1）
- [ ] `src/runtime/src/metadata/types.rs` `derive_simple_method_name`（`:942`）：非规范虚重载**保 `$`**，规范槽保裸
- [ ] `src/runtime/src/metadata/loader/type_registry.rs` `merge_with_base`（`:213-254`）：vtable override 按全键匹配非规范虚重载（H4）
- [ ] `src/runtime/src/jit/helpers/vcall.rs`（`:126-132`,`:205-211`）：候选列表**与 interp 逐一镜像**
- [ ] `src/runtime/src/corelib/reflection/methods.rs`：`MethodInfo.Name` demangle 全键→源名（规范槽 no-op）
- [ ] method table 别名登记（`loader/indices.rs` / `lazy_loader.rs`）：全键 + 裸规范槽别名，稳定序 first-wins
本地验（阶段2）：`cargo test`（含全量非 --lib）；warm e2e 定向跑 5 子系统

## 阶段 3：格式双 bump + 迁移
- [ ] `src/libraries/z42.ir/src/BinaryFormat/ZbcFormat.z42` `ZbcVersion.Minor` +1；`src/libraries/z42.ir/src/ZpkgWriter.z42` `ZpkgWriterZ.Minor` +1
- [ ] `src/runtime/src/metadata/zbc_reader/versions.rs` 两常量 + version-pin cargo 测试同步
- [ ] `docs/design/runtime/{zbc,zpkg}.md` changelog + 当前版本
- [ ] fixtures：`src/tests/zbc-format/*` / `zpkg-format/*` header minor 版本-patch（CI 真 z42c 重键覆写）；
  `src/compiler/z42c.semantics/tests/zbc/*` golden hex minor
- [ ] 确认非 format-bump 周期（避开与其它格式 churn 撞同一 nightly，bootstrap-seed.md 残留窗口）

## 阶段 4：硬回归门 + CI 收敛（权威）
- [ ] 新增/加强针对性 e2e：接口派发、泛型虚 CompareTo、泛型-over-原始 op_*/CompareTo、委托/事件、foreach
  （现有：`examples/generics.z42`、`z42.core/tests/list_sort.z42`/`list_enumerator.z42`/`op_edge_cases.z42`、
  `z42.collections/tests/{generic_list,generic_stack,foreach_user_class,dict_iter,stdlib_*}`、`z42.numerics/tests/bigint_*`）
- [ ] 本地完整 `xtask test` 全绿 + gen1==gen2 3/3 + `xtask test bootstrap`
- [ ] push → 盯 CI：`ci-bootstrap` 两代自举、`bootstrap-no-csharp`、全 golden regen 一致、e2e golden 全绿；红则逐轮定位裸名 emit 漏点收敛
- [ ] 归档 + PR（body 三段 + Claude Code 页脚）

</details>

## 风险登记
- **R1 运行期裸名 emit 漏点**（最高）：5 子系统在多处裸名 emit，阶段1 未全覆盖→阶段4 冷 CI 才炸。缓解：D7 对照表逐项核；委托/foreach 的 emit 点在 tasks 中先精确定位再改。
- **R2 interp/JIT 不同步**：候选列表两处必须逐一镜像，漏一处 JIT 路径静默走错。
- **R3 vtable 保 `$` 的辐射**：非规范虚重载全键槽若辐射过大→退为「诊断报错不支持同名多虚重载」（design Deferred）。
- **R4 跨包 composite FQN 一致性**：export/import 两侧键必须逐字节同，否则 `undefined function`。
