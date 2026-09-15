# Tasks: 依赖身份进增量缓存键 —— 依赖改了 API，消费方不能命中旧产物

> 状态：🟢 已完成 | 完成：2026-09-15
> 变更类型：`fix`（最小化模式）
> 所属程序：缓存与构建号（memory `z42-cache-and-build-id`），workspace 读缓存（PR-D）的前置。

**变更说明：** 增量缓存键只有「本包源哈希 + 编译器版本号 + 格式版本」，**没有依赖**。实测（main 1a2c37437）：
path 依赖 foo 删掉 `Answer()`（改名为 `Renamed()`），消费方 bar 源码没动 ⇒ `cached: 1/1 files` ⇒
`no changes; preserved` ⇒ 运行期 `MissingSymbolException: undefined function Foo.Answer$0`，而本应在编译期报 E0401。
单工程今天就会中招（path 依赖、stdlib 重建后的应用）；workspace 读缓存后 stdlib 成员之间也会。

**修法：** 依赖身份 = 编译实际扫描的全部依赖 zpkg（`DepScan.ScanDirs` 扫的就是 libsDirs 下全部 `*.zpkg`，排除自身）
每包 BLID（无 BLID 回落整文件 Murmur3）的组合哈希。写 cache 时进 `package.meta` 的 `deps` 行；probe 时不符 ⇒ 整包全量。
- #654 起 BLID 可复现：依赖只改注释 / 重编但输出不变 ⇒ 身份不变 ⇒ 下游照样命中（不连锁重编）。
- 粒度是包级（任一依赖变 ⇒ 全量）。按命名空间细分是后续优化，不影响正确性。
- 计算走 `DepScanCache`（进程级 memo，编译期扫描本就要读）+ 每包身份 memo，额外开销仅 BLID 提取。
- `CacheStore.MetaVersion` 5 → 6（package.meta 新增行，旧条目一次全量后恢复）。

**顺带发现（另案，未修）：** z42c 不诊断实参个数不足——同包内 `Foo.Answer()` 调 `Answer(int x)` 也编译通过、运行时 x=0。
e2e 夹具因此用「改名」而非「加参数」制造破坏性变更。

**文档影响：** `docs/design/compiler/project.md` cache 条目格式段。

- [x] 1.1 `z42c.pipeline/src/DepIdentity.z42`（新）：`DepIdentity.Of(dirs, n, selfName)`
- [x] 1.2 `DepScanCache.CachedZpkg.Identity` memo
- [x] 1.3 `CacheStore`：`SaveSrcList(..., depsId)` 写 `deps` 行、`LoadSrcList(..., depsId)` 不符 ⇒ null；MetaVersion 6
- [x] 1.4 `IncrementalBuild.ProbeFiles` / `IncrementalDriver.Prepare` / `WriteMetas` / `BuildCache._writeBuildCache` 透传；
      `Main._build` 在 libsDirs 定形后算一次，probe 与写 cache 共用
- [x] 1.5 单测：`test_src_list_roundtrip_and_pin` 加依赖身份不符 ⇒ 作废
- [x] 1.6 e2e `_e2eDepIdentityChecks`：依赖只改注释 ⇒ 消费方命中缓存；依赖删被调方法 ⇒ 消费方重编并报错。
      **阴性对照**：修复前 driver 同夹具 ⇒ `preserved`、exit 0
- [x] 1.7 `xtask test incremental` 暴力对账
- [x] 1.8 GREEN：`xtask test` 全 stage 绿
