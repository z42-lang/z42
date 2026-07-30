# Tasks: 惰性化跨包类型世界

> 状态：🟢 已完成 | 创建：2026-07-30 | 完成：2026-07-30
> 类型：perf（compiler 跨包解析机制重构，正确性风险 → 走完整流程 + 自举字节不动点门禁）
> 子系统：`compiler`（z42c.pipeline）+ 借道 `z42.ir`（TsigReconcile）。
> Spike：路由 230/230 一致、闭包 max=3 avg=1（见 design）——决策关已过。
>
> **GREEN（全绿）**：自举字节不动点 **5/5 gen1==gen2**（gen1=旧 eager、gen2=新惰性 → 字节相同即证
> eager==lazy）+ z42c 20/20 units + cross-zpkg 8/0 + stdlib 279file/23lib。**实测**：`1+2` 首轮解析
> 14/34 包（prelude+默认 using 闭包，不随库总量增长）而非全部 34。**踩坑**：① 改 `Rebuild` 签名踩
> bootstrap 轴④（种子运行期自依赖）→ 保留旧 4-arg + BuildWorld 作 eager 包装重载；② `AppendPackage`
> 扩容按 World.Length 判 → Wp 越界（`test_extend_with_package` 抓出）→ 改按 Wp.Length 判。

## 进度概览
- [ ] 阶段 0: 占用登记 + 分支就位
- [ ] 阶段 1: `LazyReconWorld`（懒填 Wp + ns 路由 + EnsureFq）
- [ ] 阶段 2: `TsigReconcile.Rebuild`/`_rebuildClass`/`_locate` 改惰性
- [ ] 阶段 3: `DepScan` 接入（去 eager BuildWorld + 建 nsIdxMap + DepScanResult 携 LazyReconWorld）
- [ ] 阶段 4: 差分断言（eager vs lazy 逐字段）绿
- [ ] 阶段 5: 全绿门禁（自举不动点 + cross-zpkg + stdlib + e2e）+ REPL 延迟实测
- [ ] 阶段 6: 文档同步 + 归档

## 阶段 0
- [ ] 0.1 ACTIVE.md 查 `compiler` 是否空闲；登记 `lazy-type-world` 占用
- [ ] 0.2 分支/worktree 就位（现 `spike-lazy-typeworld` off origin/main）

## 阶段 1: LazyReconWorld（z42.ir/TsigReconcile.z42）
- [ ] 1.1 `LazyReconWorld` 类：`World[]`/`WorldDirs[]`/`Wc` + 懒 `Wp[]`（null=未填）+ ns→world索引多重映射
- [ ] 1.2 `EnsureIdx(i)`：幂等填 `Wp[i] = ReconWorldPkg(ReadModuleTypes, ReadModuleSigs)`
- [ ] 1.3 `EnsureFq(fq)`：`_nsOf(fq)`（最后一个 `.` 切分）→ 查多重映射得**所有**声明 ns 的包 → 各 `EnsureIdx`
- [ ] 1.4 `BuildWorld` 保留为「填单包」内部 helper（不再 eager 全量）

## 阶段 2: TsigReconcile 惰性化
- [ ] 2.1 `Rebuild(z, zDir, world)` 签名（`wp[]`→`LazyReconWorld`）；先 EnsureFq(z 自身)，复用 Wp[z] 消除重复解析
- [ ] 2.2 `_rebuildClass` 基类链 walk：定位 `bn` 前 `world.EnsureFq(bn)`；扫描跳过 `Wp[p]==null`
- [ ] 2.3 `_locate` 同款：EnsureFq + 跳 null；祖先 SIGS 取 `Wp[chainPkg].Sigs[chainMod]`（已填充）
- [ ] 2.4 接口/impl 路径确认不受影响（不走基类链路由）

## 阶段 3: DepScan 接入（z42c.pipeline/DepScan.z42）
- [ ] 3.1 `ScanDirsLazy` 删 eager `BuildWorld`；world 收集循环从 NSPC 建 `nsIdxMap`（ns→world索引多重）
- [ ] 3.2 `DepScanResult` 携 `LazyReconWorld`（取代裸 `Wp[]`/`WpCount`）
- [ ] 3.3 prelude Rebuild、`_loadOpenedPackage`、`EnsurePackageLoaded`、`ExtendWithPackage` 改传/用 `LazyReconWorld`
- [ ] 3.4 `ExtendWithPackage`（REPL 增量 Repl.R{N}）：追加 world 条目 + 更新 nsIdxMap

## 阶段 4: 差分断言（开发期临时）
- [ ] 4.1 临时 harness：对每个 stdlib 包，`Rebuild` eager-Wp vs LazyReconWorld 产出 `ExportedModuleZ[]` 逐字段比对 → 0 diff
- [ ] 4.2 差分绿后撤除 harness

## 阶段 5: 全绿门禁
- [ ] 5.1 `cargo build`（VM 不改，仅确认无牵连）
- [ ] 5.2 **自举字节不动点 gen1==gen2**（`xtask test compiler`）——最终铁证
- [ ] 5.3 `xtask test e2e` + `--dir cross-zpkg`（基类链跨包全套）
- [ ] 5.4 `xtask test stdlib`（23 lib）+ `vscode-syntax`
- [ ] 5.5 REPL：eval 正确性 + 首次-eval 延迟实测（`1+2` / `Console.WriteLine`）+ 闭包不随包数增长外推

## 阶段 6: 文档 + 归档
- [ ] 6.1 `docs/design/toolchain/repl.md` 补惰性类型世界机制 + Deferred（defer-open-strs / symbol-name-index）
- [ ] 6.2 `z42.ir` / `z42c.pipeline` README 功能索引
- [ ] 6.3 spec scenarios 逐条覆盖确认
- [ ] 6.4 归档（archive/2026-MM-DD-lazy-type-world；释放 compiler 锁）

## 备注
- 最高风险：基类链懒解析漏祖先 / 顺序错 → 差分断言(4.1) + 自举不动点(5.2) 双保险，未绿不提交。
- 零格式 bump、零 VM 改动。
- **CI 冷启动踩 bootstrap 轴④ + 修（2026-07-30，PR #79 首推后 CI 红暴露）**：本 change 让 z42c
  自身源（z42c.pipeline/DepScan）用到当前源 z42.ir 新增的 `LazyReconWorld`——而 `_ensureBootstrapZ42Ir`
  （`scripts/build/xtask_compiler.z42`）早先「z42.ir 已在 build-libs 就 warm-skip」的幂等假设 = 「z42c 不
  消费 z42.ir 新 API」；CI 冷启动 stage 的是**上一 nightly 的旧 z42.ir**（无 `LazyReconWorld`）→ z42c
  `--workspace` self-build 编 z42c.pipeline `unknown type LazyReconWorld` 全红。**修**：`_ensureBootstrapZ42Ir`
  改为**总是**用种子 driver 把当前源 z42.ir 建进 build-libs（覆盖种子旧版），z42c 永远对当前源 z42.ir
  编译——破了「z42c→z42.ir 新 API 需晚一 nightly」约束。warm dev 靠增量缓存近零成本。种子运行期靠保留的
  旧 4-arg `Rebuild`+`BuildWorld` 仍工作（种子 z42c 对 fresh z42.ir 运行 OK，fresh z42.ir 含旧 API）。
  **验证**：本地用**上一 nightly 的 driver + 种子 z42.ir**（一致态）复现 CI 冷路径——nightly driver 建
  fresh z42.ir + 编 z42c.pipeline against it 均通过。教训：改 z42c 运行期自依赖库的 API 面，本地必须用
  **一致的种子态**验冷路径，别用「fresh driver + stale ir」的错配态（会假失败误导）。
