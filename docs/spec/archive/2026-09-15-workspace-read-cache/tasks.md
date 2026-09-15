# Tasks: workspace 读增量 cache —— 封闭构建 + 修增量读回三处与全量不一致

> 状态：🟢 已完成 | 完成：2026-09-15
> 变更类型：`fix`（最小化模式）
> 所属程序：缓存与构建号（memory `z42-cache-and-build-id`）PR-D；前置 #658 / #661 / #662 均已合。

**变更说明：** User 目标「workspace 也支持增量」。此前 workspace 成员恒全量（只写 cache 不读）。

**修法：**
1. **放开读**：`Main._build` 的 probe 门对 workspace 成员（`cacheDirOverride` 非空）放开；`effCacheDir` 统一 probe /
   indexed dist 读回 / 写 cache 三处（此前 indexed dist 固定按 manifest 解析 cache 目录，workspace 一读就错位）；
   `--no-incremental` 透传到成员（`_buildWorkspace*`）。
2. **封闭构建**：成员看不到拓扑序在后的成员——`ZpkgPathSort._sortedZpkgsMulti` 加 hidden 名单，经
   `DepScan.ScanDirs` / `CompileInputs.HiddenPkgs` / `DepIdentity.Of` 同口径透传，driver `_laterMemberNames`。
   不封闭时：前序成员能扫到后序成员上一轮的旧产物 ⇒ 依赖身份每轮变化（实测无改动第二次构建仍全量）；且产物随可见性漂移。
3. **产物变化 ⇒ 指纹 6→7**（main 上 #663 已占用 6）：封闭后 z42.json 的 `Dictionary.Get/Set/ContainsKey` 由派发变直调（后序成员同名方法此前
   让依赖索引出现歧义键被剔除）。`test fingerprint` 如实判红，bump 后放行。
4. **增量读回三处与全量不一致**（main 单工程增量同样受影响，只是没人在 stdlib 规模上验过）：
   - `ZbcReader._readSigs` 丢弃方法级泛型形参名、重建 `IrFunction` 不设 `TypeParams` ⇒ SIGS 丢 tp 块
     （z42.core 少 240B，下游 E0402 `String[] to T[]`）
   - `ZbcReaderInstr.Retype` 手写白名单漏 4 条 Struct* 指令 ⇒ 读回寄存器类型 Unknown ⇒ REGT 与全量不同
     （KeyValuePair / DictionaryEnumerator）；末尾加走统一操作数接口的兜底
   - 编译期无参数名的函数（struct 合成 Equals、属性 setter）writer 写占位 "?"，读回原样留下后被打包 writer 当真名入池
     ⇒ STRS/SIGS 不同；全部参数名都是占位时还原为空
5. **对账补覆盖**：demo 语料加方法级泛型 + struct；新 `demo-packed`（packed 从读回 IR 重打包）；新 stdlib 整体读回对账
   （`scripts/test/xtask_test_incremental_ws.z42`：全量 → 清全部 dist → 全命中重打包 → 25 包逐字节一致）。

**实测（stdlib 25 包 release）：** 全量 12.4s；无改动 0.6s；z42.core 改注释 1.3s；z42.text 改实现 10.5s；z42.yaml 改实现
7.2s；每个场景增量产物与同源全量逐字节一致。

**文档影响：** `docs/design/compiler/project.md`（Deferred「workspace wiring」改为已落地说明）。

- [x] 1.1 `Main.z42` / `WorkspaceBuild.z42`（driver）：probe 放开 + effCacheDir + `--no-incremental` 透传 + hidden 名单
- [x] 1.2 `ZpkgPathSort` / `DepScan` / `PackageCompile` / `DepIdentity`：hidden 透传
- [x] 1.3 `CacheStore.CompilerFingerprint` 6 → 7（#663 已占用 6）
- [x] 1.4 `z42.ir` `ZbcReader` / `ZbcReaderInstr`：三处读回修复
- [x] 1.5 e2e `_e2eWorkspaceIncrChecks`：无改动全命中 / 依赖改注释下游命中 / `--no-incremental` 不读 / 依赖删方法下游报错
- [x] 1.6 `xtask test incremental`：demo（+泛型/struct）/ demo-packed / xtask / stdlib 整体读回全过。
      **阴性对照**：修复前编译器 demo 即 MISMATCH
- [x] 1.7 `test fingerprint --base <#662 树>`：输出变 + 已 bump ⇒ 通过
- [x] 1.8 GREEN：`xtask test` 全 stage 绿
