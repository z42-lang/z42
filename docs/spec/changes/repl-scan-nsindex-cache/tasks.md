# Tasks: REPL 首次求值提速——惰性 prelude（T1）+ ns 索引（B）

> 状态：🟢 IMPL 完成（T1 lazy-prelude + B ns-index）| 创建：2026-07-31 | 类型：perf
> **交付**：`1+1` 秒回车 1.72s→**0.40s**（4.3×）。核心是 T1（惰性 prelude，学 Python「对 1+1 别干
> reconcile 整个 stdlib 的活」），B（ns 索引）为辅（Windows 免 open-all）。**权衡**：首次符号 eval
> （Console）走前台 E0401 ≈ 1.7s，彻底解由 **T2 按符号 reconcile**（独立 change，见 proposal 非目标）承接。
> **GREEN**：自举字节不动点 + e2e 215/0 + cross-zpkg 8/0 + multi-exe 1/0 + REPL 求值正确性全对。
> stdlib [Test] 的 `FieldGet Null` 崩溃**是 pre-existing**（clean origin/main 在本 worktree 同样复现、
> runner 未被本 change 触碰、main CI 全绿 → 本 worktree 环境态所致），非本 change。
>
> ⚠️ 下方阶段清单为原始 B-only 计划，最终以 proposal.md 的 T1+B 设计为准。旧类型注释：perf
> 子系统：`compiler`（z42c.pipeline `DepScan`）+ `toolchain`（—，A 否决后 Script 不改）
> + 借道 `z42.ir`（`LazyReconWorld` 惰性开包）。**A 已否决**（见 proposal）——本 change 仅 B。
>
> **GREEN**：① 自举 byte-identical（非惰性 `ScanDirs` 零改 → gen1==gen2 天然不受影响）；
> ② REPL eval 正确性（表达式/Console/集合/声明/跨轮 var）改前后一致；③ 缓存命中 vs 未命中（open-all）
> 产出的 nsMap/Exported 一致；④ libs 指纹变更 → 索引重建；⑤ `xtask test` 全绿。

## 阶段 1: z42.ir / LazyReconWorld 惰性开包（additive，bootstrap 轴④安全）
- [ ] 1.1 加字段 `public string[] WorldPaths;`（null = eager 模式，现有构造器/FromEager 设 null）
- [ ] 1.2 静态工厂 `LazyFromPairs(worldPaths, worldDirs, wc, pairNs, pairIdx, pairN)`：World[] 全 null、
      路由用传入 pair（不 open、不 ReadNamespaces）
- [ ] 1.3 `EnsureIdx(i)`：World[i]==null 且 WorldPaths!=null → `ZpkgReader.Open(File.ReadAllBytes(WorldPaths[i]))`
      填 World[i] 再读 TYPE/SIGS。eager 模式（World[i] 非 null）行为不变
- [ ] 1.4 保留 3-arg 构造器 + `FromEager` + `EnsureFq`/`AppendPackage` 签名不变（种子 z42c 运行期兼容）

## 阶段 2: z42c.pipeline / ns 索引读写 + 指纹
- [ ] 2.1 `_libsFingerprint(paths)`：排序后每 path 拼 `basename:GetSize:mtimeMs`（mtime 用本地 extern
      `__file_last_write_time_ms`，避免 z42.time 依赖）→ 单串
- [ ] 2.2 `_nsIndexPath(dirs)`：首个可写 libsDir 下 `.z42-nsindex`；无可写 → 返回 ""（不缓存）
- [ ] 2.3 `_readNsIndex(path, fp)`：读文本；header 指纹匹配 → 返回每包 (basename, namespaces[])；否则 null
- [ ] 2.4 `_writeNsIndex(path, fp, sortedBasenames, nssPerPkg)`：`WriteAllTextAtomic`（失败静默——只读 libs 容忍）

## 阶段 3: ScanDirsLazy 双路径
- [ ] 3.1 命中：从索引建 nsMap（nsNames/nsFiles，按排序保 first-wins）+ 路由 pair；`LazyFromPairs` 建
      lazyWorld（World 全 null + WorldPaths）；DepScanResult.Opened 惰性（存 path，Loaded 全 false）；
      **prelude 仍 open+reconcile**（首轮需要）
- [ ] 3.2 未命中：现有 open-all 路径（open 全部、ReadNamespaces、prelude reconcile）；顺带收集 nssPerPkg → 写索引
- [ ] 3.3 DepScanResult 加 `OpenedPaths`（惰性 open 用）；`Opened[i]` 命中路径下初始 null
- [ ] 3.4 `EnsurePackageLoaded`/`_loadOpenedPackage`：`Opened[i]==null` → 按 `OpenedPaths[i]` open 再 reconcile

## 阶段 4: GREEN
- [ ] 4.1 `xtask build sdk`（自建 z42c + scripting + interactive）
- [ ] 4.2 REPL eval 正确性套件（表达式/Console/List/声明 fn+class/跨轮 var carry）改前后一致
- [ ] 4.3 命中 vs 未命中差分（删 `.z42-nsindex` 跑一次 = 未命中；再跑 = 命中；两次 eval 结果一致）
- [ ] 4.4 指纹失效（touch 一个 zpkg → 下次重建索引）
- [ ] 4.5 `xtask test`（e2e + cross-zpkg + stdlib + runtime）+ 自举不动点
- [ ] 4.6 实测一次性 `-c "1+2"` 延迟（命中）对比 1.75s baseline

## 阶段 5: 文档 + 归档
- [ ] 5.1 `docs/design/toolchain/repl.md` 补 ns 索引缓存机制 + Deferred（change C 预热镜像）
- [ ] 5.2 归档（archive/2026-MM-DD-repl-scan-nsindex-cache；释放 compiler 锁）

## 备注
- 最高风险：命中路径 nsMap first-wins 顺序、路由与 open-all 不一致 → 4.3 差分双保险。
- 零格式 bump、零 VM 改动、非惰性 ScanDirs 零改。
