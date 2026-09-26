# Tasks：依赖部署模型

> 设计 SoT：[design.md](design.md)。**批次待 User 裁决 A–D 后定稿**，此处是初步切分。

| 批 | 内容 | bump | 卡 nightly | 状态 |
|----|------|:---:|:---:|------|
| 1 | 统一复制判据 + 统一传递闭包（裁决 B/D）| 否 | 否 | ⬜ |
| 2 | `probing-paths` 旋钮 + 运行期搜索序（裁决 C）| 否 | 否 | ⬜ |
| 3 | `deploy` 字段 support（`DepEntry.Deploy` + ManifestLoader）| 否 | **是** | ⬜ |
| 4 | `deploy` 字段 use（构建期消费 + `shared` 存在性校验）| 否 | 否 | ⬜ |
| X | `Z42_PATH` 死旋钮处置 | 否 | 否 | ✅ **退役**（#821）|
| 5 | `ModuleSearch.Dirs()` —— 把解析后的搜索序暴露给 z42 | 否 | 否 | ✅ support（#832）|
| 6 | `[dependencies]` 的 path 支持直接引用 `.zpkg` 产物 | 否 | 否 | ✅ 完成（#836）|

## 批 1 —— 统一复制判据 ✅

- [x] 1.1 `_bundleExeDeps` 的判据换成 **在不在 shipped `libs/`**（= `Z42_LIBS`，publisher 一直
      在用的那条），删掉 `_srcRoot`（唯一调用方就是它）。**没抽共用 helper**：两个函数分别住在
      `z42c.driver` 与 `z42.builder`，共用 helper 要落在两边都依赖的包里 = 新跨成员符号、卡一个
      nightly；判据本身只有一行，两边各写一遍 + 注释互指更稳（同仓库既有做法）。
- [x] 1.2 顺带删掉不再使用的 `projectDir` 参数（三个调用点同步）。
- [x] 1.3 修正互相矛盾的注释，并把「只复制直接依赖」这条缺口**写在代码里**（连同为什么现在
      修不了：z42 侧读不出 DEPS 段）。
- [x] 1.4 门 `_e2eDeployPredicateChecks`（`xtask_compiler_e2e_deploy.z42`）。
      🔴 **fixture 必须落 repo 外（`/tmp`）**：放 repo 内 `_srcRoot` 找得到根、走目录判据，
      旧代码在那儿是对的 —— 门就看不见差别。**这个缺陷只在用户机器上发生。**
      判别力实证：把判据退回 `dep.StartsWith("z42.")` → 门红在「仍被复制进 exe dist」、rc=1。

> **可观测差异是什么**（选门的场景时想清楚的）：新旧判据对 path 依赖结论相同（旧代码里
> `!isPathDep &&` 已经把 path 依赖排除在 stdlib 之外）。真正的差异是**一个确实在框架目录里、
> 但名字不以 `z42.` 开头的包** —— 旧判据把它当私有依赖复制进每个用到它的 exe。

## 批 2 —— zpkg DEPS 解码 + 按名依赖的闭包

> ⚠️ **订正**：初稿把「统一闭包」整块判成做不到。实际上 **path 依赖那半已由 #811 修掉**
> （闭包本来就有，`PathDepPlan.Resolve`，只是没透传给 `_bundleExeDeps`）。剩下的是按名那半。

- [x] 2.1 **support**：`ZpkgReader.ReadDependencies`（Rust 侧 `zbc_reader/zpkg.rs:46` 早有，
      z42 侧没有）。无消费者 ⇒ 自举 byte-identical、已合并；use 待它随 nightly 进种子。
      门 = `zpkg.z42` 两条往返测试。⭐ **第一版 fixture 没有判别力**：把「按 nsCount 跳过」
      改成「恒跳 1 个」注入进去，测试**照样绿** —— 因为多命名空间的那一项被我放在了**最后**，
      跳错多少都没有后续项会错位。多 ns 项前移后才真红。
      **「构造了一个复杂输入」不等于那个复杂度真的被验到了。**
- [x] 2.2 **use**：`_bundleExeDeps` 对按名/产物引用的依赖也建闭包，来源 = zpkg 的 DEPS 段
      （`ReadDependencies`，**不是**源码树 toml —— 那条路在 repo 外整个失效）。
      只对**已复制**的包递归：框架包不复制也不递归（它的依赖同在 libs/）、`deploy = "shared"`
      在循环开头就跳过（闭包由运行期解析）。间接依赖不在 `[dependencies]` 里 ⇒ deploy 取 `""`、
      走默认判据。
      门 `_e2eNamedClosureChecks`（`app → mid → leaf` 两层链）；判别力：撤掉 DEPS 读取 →
      门红在「间接依赖没随产物走」。
      ⭐ **缺口的症状离原因很远**：改前 `dist/` 只有 mid，拷出去运行**死在 mid 的方法里**
      （`MissingSymbolException: NcLeaf.Deep`）——报的像是「mid 的代码有问题」，实际是打包漏了 leaf。
- [x] 2.3 **publisher 的闭包** —— 实测把本条原先记的修法**整个翻掉了**（记的是「改按 DEPS 段或
      `{ path }` 解析，镜像 native 那条的 Decision 5」）。实际情况分两半：

      **① publish 这条路上它是死重，删。** `z42 publish` 走 `_pubEnsureBuilt` → **z42c driver**，
      dist/ 早已由 `_bundleExeDeps` 填满（复制判据 #813 + 闭包 #811/2.2），`_pubCopyDistDeps`
      再把 dist 搬进 payload。撤除整条 `_pubBundleProjectDeps` 后，四个组件的 payload 文件清单
      **逐字不变**、`z42i` 行为一致 ⇒ 三个 publish 调用点删除。
      ⭐ **对账前必须清空 publish 目录**：`File.Copy` 只覆盖不删除，残留会让「撤除后仍一致」
      变成假结论 —— 同一轮里刚在门自己身上踩过这个（批 4.1）。
      顺带删掉 `_pubProjectDepNames` + `_pubMemberTomlPath`（零调用方，是被本函数取代时留下的）。

      **② 它还有第四个调用方，在 `z42b build` 编排路上（`builder_commands.z42`）—— 但那条路的
      问题比「闭包搬不全」更靠前：它根本不解析 path 依赖**（闭包解析住在 z42c driver 的 `_build`
      里）。repo 外对着一个有 path 依赖的工程连编译都过不了（实测 `E0494: 命名空间 `OoMid`
      不存在`）；repo 内看着能用，只因依赖早已预建进 libs。
      ⇒ **User 裁决（2026-09-26）：把闭包循环下沉，两边共用**。但裁决前我给的选项描述有一处
      事实错误，实施时查出来并纠正了：**`z42b build` 不是「比 `z42c build` 弱的重复路」** ——
      它带 `--rid`、`_selectWorkload(target.Family)`、`[build] hooks`，而 **z42c 根本没有 rid 概念**
      （源码里零出现）。转发过去会丢掉 rid + workload + hooks 三样。
      真正的事实是：`PathDepPlan` **只 `using Std / Std.IO / Z42.Build.Project`，零编译器依赖**，
      住在 `z42c.pipeline` 纯属历史落点 ⇒ 这不是「两条能力不同的路」，是**一个解析器放错了包**。
      落地见 2.4。

- [x] 2.4 **闭包解析下沉 + z42b 接上**（2026-09-26）：
      · `PathDepPlan` / `PathDepClosure` 搬进 **`z42.project`**（它唯一依赖的那个包）。z42b 刻意
        stdlib-only、只经注入的 `ICompiler` 碰编译器，搬下去之后它才够得着 ⇒ 解析器**一份实现**。
      · `z42c.pipeline` 留一层**转发**（种子 ABI）：上一 nightly 的 driver 二进制在运行期调
        `Z42.Pipeline.PathDepPlan.Resolve` 并按 `Z42.Pipeline.PathDepClosure` 的字段读结果，而
        `_ensureBootstrapSelfDepLibs` 逐个单独编那 6 个自依赖库时正是 top-level build ⇒ 真会走到。
        直接删 = 种子在 bootstrap 中途 `MissingSymbolException`（实测过）。
        ⏳ **阶段 2（下一 nightly 后）删掉 `z42c.pipeline/src/PathDepPlan.z42`**。
      · z42b 的 `_buildPathDepClosure`：按拓扑序逐成员 `_orchestrate` + 落 dist，累积 dist 目录。
        **共用的是解析器、不是循环** —— per-member 构建语义两边本就不同（driver 调 `_build` 走增量
        缓存与侧车，z42b 调 `_orchestrate` 走 rid/workload/hooks），这层差异不该消掉。
        `_initialInputs` / `_orchestrate` 加了收多个额外依赖目录的重载（原先只收一个，那是
        「父包 dist」一个用例留下的形状）。
      · ⭐ **闭包只买到「能编」是不够的**：补完解析后两层链编过了，但拷出去**跑不起来**
        （`MissingSymbolException: OoMid.Mid.Call`）—— dist 里没有闭包成员。补 colocate，名单直接
        取自刚建的闭包，**不是**去源码树按名搜（那份走 srcRoot 的实现在 repo 外恒空转，见 2.3）。
        `_pubBundleProjectDeps` 因此只剩**按名依赖**那半，保留不动（避免 in-repo 回归）。
      · 门 `_e2eZ42bClosureChecks` 两格（fixture **必须落 repo 外**，否则依赖早已预建进 libs、
        门看不见差别）。判别力：让 `_buildPathDepClosure` 直接返回空 → 门红在①格、rc=1。

      **③ 新门 `_assertPayloadComplete`**（`xtask_toolchain.z42`，每次 publish 后）：dist 里每个
      非-app 产物（zpkg + 同族 native）都必须在 payload 里出现。**在此之前没有任何东西盯着
      payload 完整性** —— payload 少一个包，`build toolchain` 照样打印 ✅。
      判据刻意是**结构对账**而非写死名单（名单会随依赖变化腐坏）。
      判别力实证：把 `_pubCopyDistDeps` 的 `take` 钉死成 false → 门红并逐个点名
      （`dist 有 z42.workload.android.zpkg 而 payload 没有`），rc=1。
      ⚠️ 实证时先踩了一次**空门**：`./xtask` 是二进制，不从 `scripts/` 重建 ⇒ 门代码没进
      `xtask.zpkg`，第一次注入「绿」了。重建命令 = `z42c build scripts/xtask.z42.toml --release`。

      **④ 顺带（User 指出「xtask 里尽量按 toml 配置取，不要写死」）：补上 `[build] output_dir` 的
      workspace 继承档。** 门初稿把 `distDir` 交给调用方传、z42c 那格传的是硬编码 helper ——
      而 `xtask_toolchain.z42` 开篇的 SoT 原则写着「every toolchain output/publish path is READ
      from the component's own z42.toml」。`_wsOutputDir` 实现 `[workspace.build] output_dir`
      （`${project_name}` / `${profile}` 模板、**相对 workspace 根**解析），`_toolchainDistDir` /
      `_desktopPublishDir` 一律经它读配置。
      🔴 **这一档缺失本身就是一个活 bug**：z42c 的 publish_dir 被猜成 `<projDir>/publish` ⇒
      **发布产物一直漏在源码树** `src/compiler/z42c.driver/publish/`，而 z42c.driver 清单的注释
      白纸黑字写着它该落 `${output_dir}/publish`。补完之后落点变
      `artifacts/build/compiler/z42c.driver/release/publish`，与清单一致。
      ⭐ 门是这么被发现的：它在 z42c 那格红在「dist 目录不存在」—— **门报的失败先怀疑门自己**，
      查下去发现门没错、是路径来源写死了。

## 批 3 —— probing-paths

- [x] 3.1 新增旋钮 `Z42_PROBING_PATHS` / `probing-paths`（`ValueKind::PathList`）。
      ⭐ **z42 侧零改动**：profile knobs 是通用 `"key=value"` 透传，且旋钮名**直接问 VM**
      （`RuntimeConfig.Names()`）⇒ 新旋钮进了 VM 登记表，z42c 自动认识、侧车自动烤进去。
- [x] 3.2 `app.rs` 的 `search_dirs` 插入展开结果（entry 之后、libs 之前）。
- [x] 3.3 展开器（`runtime/src/probing.rs`）：相对 **entry 目录**／绝对原样／`*` 与 `**`／
      只返回目录／Ordinal 稳定排序／缺失静默跳过。
      ⭐ **单测抓到一个会漏掉的缺陷**：`entry.join("../shared")` 的字面量是 `app/../shared`，
      与 `shared` 是**不同字符串** ⇒ 去重失效、同一目录搜两遍。加**词法**规范化（不用
      `canonicalize` —— 那会解析符号链接，改变用户写的语义）。
- [x] 3.4 侧车：`[profile.<n>.runtime] probing-paths` → `dist/<app>.runtimeconfig.toml`。
- [x] 3.5 两层门：`probing_tests.rs`（展开规则 8 格）+ e2e `_e2eProbingPathChecks`（三格：
      不配→跑不起来／配了→跑得起来／改旋钮值→侧车跟着变）。
      判别力实证：把 `search_dirs` 的接线换成空列表 → 门红在「配了却没生效」、rc=1。

### 批 3 顺带修掉的真缺陷

🔴 **所有运行时旋钮「改了但不生效」**：`[profile.*.runtime]` 与 `[properties]` **不进源 hash**
（它们不影响编译产物），于是「只改运行时配置、源码一字未动」恰好全命中增量缓存 ⇒ 走
preserved 早退 ⇒ **侧车留在上一次的值**。实测：probing-paths 从 `../../shared` 改成
`../../CHANGED`，重建报成功而侧车纹丝不动。修 = preserved 分支里也写侧车（幂等）。
**射程不止 probing-paths，是每一个运行时旋钮。**

### 已知限制

~~`probing-paths` 是**平台分隔符**分隔的字符串，跨平台清单写多条时分隔符不同。数组写法要改清单
模型（`pr.Knobs` 是扁平 `"key=value"`）= 卡 nightly。~~ ✅ **已解决（2026-09-26，见批 3.6）**，
**而且不卡 nightly** —— 当初判「卡 nightly」是因为以为要改 `Knobs` 的形状，实际不用。

另：`deploy = "shared"` 的依赖**编译期仍须可解析**（z42c 要读它的元数据），probing-paths 只管
运行期 —— fixture 要按「构建机有完整 libs、目标机只有 shared/」来搭。

## 批 4 —— `deploy` 字段

- [x] 4.0 support：`DepEntry.Deploy`（`""` = 未声明）+ ManifestLoader 解析。**无消费者**
      ⇒ byte-identical、已合并。**构造后赋值、不进 ctor 签名** —— ctor 是种子 ABI 的一部分，
      加参数会让上一版 z42c 编不动当前源码（同 `Pipeline.ParentPkg` 那几个「构造后填」字段）。
      解析层只忠实搬运、**不校验取值**：合法取值是消费方的事。
- [x] 4.1 use：构建期按 `deploy 显式 > 默认判据` 决定复制与否（`_bundleExeDeps` 加与 names
      并行的 `deploys[]`；path 闭包成员取 `""`）。非法取值报错 —— 否则被当成未声明静默走默认。
      门 `_e2eDeployUseChecks` 四格；判别力：撤掉两条决策 → 第②格红。
      ⭐ **门自己先踩了状态污染**：初稿每格之间没清 `dist/`，③ 读到 ②复制进去的残留 ⇒ 报
      「shared 没能阻止复制」，而实现是好的。**门报的失败要先怀疑门自己。**
- [ ] ~~4.2 `shared` 的构建期存在性校验~~ —— **明确不做**（暂）。要判断「运行期够不够得着」，
      构建侧就得把 VM 的 probing-paths 展开规则（相对 entry / 通配符 / 去重）重做一遍 ⇒
      **两份规则各自漂移**，正是本 change 一路在消灭的东西。等有共享途径（把展开暴露成
      可在构建期调用的能力）再补。
- [x] 4.3 ~~`role = compile-time`~~ → **`kind = "analyzer"`** 的工程写 `deploy` → 报错。
      ⚠️ role 字段已取消（见 add-package-roles/design.md 复盘），本条载体随之改成 `kind`。

      开工后发现范围比记的大：校验此前住在 `_bundleExeDeps` 里，而那个函数**只在 exe 分支跑**
      ⇒ 🔴 **`kind = "lib"` 里写 `deploy = "Copy"`（大写 typo）一直是静默无效的** —— 本 change
      一路在消灭的那个形状，自己身上还留着一处。
      ⇒ 校验提到 `_validateDeployDecls`，由 `_build` 在编译**之前**调用（与 `_validateProfileKnobs`
      同款「清单有问题不等一趟全量编译」），对所有 kind 生效；`_bundleExeDeps` 里那份删掉，
      不留第二处。覆盖三条：非法取值（全 kind）／analyzer 工程的 `[dependencies]`／任何工程的
      `[analyzers]` 条目。
      **不发新诊断码**：driver 的清单/CLI 错误一律不带码（E0496 那类来自 pipeline/semantics），
      加一个会破掉这条约定、并给码表凭空添一笔欠账。
      门 `_e2eDeployDeclChecks` 四格（含对照格），刻意只盖**非-exe** 那半 —— exe 的非法取值已由
      `_e2eDeployUseChecks` ④ 守着，两个门守同一件事的话，其中一个坏了不会有人知道。
      判别力实证分两步：① 撤掉整条接线 → `_e2eDeployUseChecks` ④ 先红（说明校验确实只有一个来源）；
      ② 退回「只对 exe 生效」→ 新门精准红在②格、exe 那格仍绿。

## 批 X —— `Z42_PATH` 死旋钮

- [x] X.1 User 裁「退役」（2026-09-25）：旋钮删除、`module_paths` 参数一并从加载器拿掉，
      `runtime-settings.md` 留一行退役记录说明它承诺的语义从未生效过。
      **不要让它的历史债决定 `probing-paths` 的形状**（见 proposal 裁决 C）。

## 批 5 —— `ModuleSearch.Dirs()`（#832，support）

VM 内部一直算着一份搜索序，但它是 `app.rs::run` 的**局部变量** —— z42 侧看不见，于是
「按名找一个 zpkg」完全无解，长出两份各四十行的 `_findCompilerZpkg` 与 scripting 那个恒失败
空壳。新 builtin 把**解析结果**（相对项已按 entry zpkg 解析、通配符已展开、缺失已剔除、
重复已去重）暴露出来。

- [x] 5.1 builtin `__search_dirs` + `Std.Runtime.ModuleSearch`；门挂 `runtime_config_query` golden。
- [x] 5.2 **use**（#842）：两份 `_findCompilerZpkg` 改读它 —— z42b 只剩一档开发树兜底，
      位置改由清单的 `probing-paths` 声明；scripting 保留 `Z42_LIBS` 兜底。⭐ 空数组是**有意义的答案**：这个部署形态不带编译能力，而不是"路径没配对"。

## 批 6 —— zpkg 产物引用（#836，已完成）

`path` 指向 `.zpkg` = C# 的 `<Reference HintPath>`；指向工程目录 = `<ProjectReference>`。

- [x] 6.1 `PathDepPlan` 按扩展名跳过产物引用（不代建）；driver 把其所在目录并入解析域。
- [x] 6.2 包名校验（zpkg 的 `[project].name` 须等于清单 key）。
- [x] 6.3 顺带接住 `PathDepPlan.Resolve` 的异常 —— 从未捕获异常改为干净的错误退出。
- [x] 6.4 门 `_e2eZpkgRefChecks` 四格；判别力：撤掉跳过 → 第①格红。

⭐ **运行期自包含没写一行代码**：`_bundleExeDeps` 的判据是「从哪个目录找到的」（批 1），
vendored 目录不是 shipped `libs/` ⇒ 自动复制进 dist。**批 1 那个为修误判而做的改动，
在这里白拿了一个新特性** —— 把判据从「按名字猜」改成「按来源判」的复利。

> 📌 由此，「编译器域包该放哪」**不再是个需要解决的问题**：想引用哪个 zpkg 就直接指它，
> 位置降级成纯粹的打包体积决策。`add-package-roles` 的批 2.5 / 2.6 因此取消。

## 批 2.5 —— 阶段-2 欠账挂账门（2026-09-26，2.4 的配套）

2.4 为种子 ABI 在 `z42c.pipeline` 留了一层转发层，那是个**阶段 1 过渡形态**。而
`bootstrap-seed.md` 自己写着「新开一个分阶段引入时，先想好阶段 2 由什么来提醒你 —— 没有提醒就
等于没打算做」，**除诊断码以外却没有任何东西执行这条**。

- [x] 2.5.1 门 `xtask test stage2` + 清单 `scripts/test/stage2-debt.txt`：源里写
      `// STAGE2-DEBT(<tag>): <阶段 2 要做什么>`，门做**双向棘轮**（源里多一条 / 清单多一条都红）
      + **挂账超 7 天即红**（7 而非 diagcodes 的 3：那条只要常量进种子，这条还要求真把活干完）。
      已有条目**保留原挂账日** —— 否则每次 `--update` 把时钟拨回今天、到期判据永不触发
      （diagcodes 规则 ⑥ 踩过，此处照抄结论）。
      判别力实证三格：① 注入未挂账标记 → 红；② 清单留一条源里没有的 → 红；③ 把挂账日改早 → 红「超期 25 天」。
- [x] 2.5.2 **立门当天就抓到 4 条「现在时的假断言」**（支持侧注释说「今天没有生产调用方 / z42c 尚不读」，
      而消费早已在后续 PR 落地，没人回来改那句话）：`ZpkgReader.ReadDependencies`（#849 已消费）、
      `z42.ir/tests/zpkg.z42` 同款、`DepEntry.Deploy`（#848 已消费）、`ICompiler.Excludes`（阶段4.2 已落地）。
      ⭐ **前三条出自同一条 support→use 链，作者同一个人（我）** —— 这正是诊断码那边规则 ⑦ 治的病，
      换个地方复发。
- [x] 2.5.3 `bootstrap-seed.md` 补**轴 ④ 豁免的边界**：「加 API 可同 commit 加+用」**只对增量成立**，
      改名/删除会抹掉旧 FQN ⇒ 跑这轮 bootstrap 的那个 driver 中途就死（实测见该节）。此前只写了
      ctor 签名一条残余约束。
- [x] 2.5.4 **已知未挂账的真债 → 已清**（`store-sync-values-in-heap` 阶段 2，2026-09-26）。
      19 个旧同步 builtin + 三个 registry + `corelib/sync.rs`（596 行）整批删除。
      ⭐ **我对这条债的判断连错两次，都是靠读码纠正的**：
      ① 第一次说「阶段 2 不是机械删除，因为 `vm_context/types.rs` 还在用 `sync::ChannelSlot`」——
         查下去那三个 registry 除声明与构造外**零使用**，是跟着旧 builtin 一起死的。
      ② 第二次改判「槽位永久不可删，因为 BuiltinId 是下标」并写了墓碑桩 —— 那是照 `builtin_table_ext.rs`
         头注写的，而**那句头注本身是错的**：zbc 存的是名字（`BuiltinInsn { name }`），`BuiltinId`
         是加载期按名填的派发令牌、AOT 不烤 ⇒ 能真删。两处文档互相矛盾时，**必须去代码里定论**。
      判据按归档规定核过：nightly 种子 `strings -n 3 | grep` 旧名，programs/z42c 与 libs 皆 0 引用。
      ⭐ 顺带：删掉的两个 Rust 测试的覆盖在 z42 那层活着（`z42.threading/tests/` 17 单元），留了指路注释。

## 批 3.6 —— `probing-paths` 的数组写法（2026-09-26）

- [x] 3.6 清单里 `probing-paths = ["../a", "../b"]` 生效。

      ⭐ **「卡 nightly」这个判断是错的**：当初以为要改 `Profile.Knobs` 的形状（扁平 `"key=value"`）
      才能承载数组 ⇒ 记成「随批 4 的清单改动一起做」。实际只要**不在 z42 侧摊平**就不用改形状：
      · `ManifestLoader._profileKnobs`：数组元素用 `"\n"` 连接成一个值。`\n` 只是**运输标记**，
        不是路径分隔符 —— 选它正因为它在任何平台都不会被误当成分隔符。
      · 侧车写入器：见到 `\n` 就还原成 **TOML 数组**写进 `[runtime]`，**不拼字符串**。
      · VM（`config/parse.rs` 的 `toml_value_to_string`）：`PathList` 旋钮接受 TOML 数组，用
        **本机**分隔符摊平。
      ⇒ **平台假设被推到唯一有权做它的地方**（运行这个应用的那台机器），中间各层都不碰它。

      🔴 **改前是「配了以为生效」**：`_profileKnobs` **静默跳过数组** —— 用户写了数组等于没配，
      没有任何东西会说话。这是本 change 一路在消灭的那个形状的最后一例。

      门：Rust 单测 4 条（本机分隔符拼接 / 空白与空串剔除 / 空数组=没配 / **非 PathList 的数组仍非法**）
      + e2e `_e2eProbingArrayChecks` 两格（侧车保持数组形态、两条共享目录都解析得到）。
      判别力：让 `_profileKnobs` 退回静默跳过 → 门红在①格「侧车里不是 TOML 数组」，rc=1。

      ⚠️ 顺带踩到一个**与本改动无关**的坑：`cargo test --release` 必然报 6 个
      `debug_validate_invariants` 错误 —— 那个方法是 `#[cfg(debug_assertions)]` 的，release 下不存在。
      仓库自己的入口是 **`xtask test runtime`**（debug）。别用 `cargo test --release` 判断 Rust 单测健康。
