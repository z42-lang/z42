# Tasks: perf-depscan-memo-modulesigs（流程优化 F2-邻）

> 状态：🟢 已完成 | 创建：2026-08-22 | 完成：2026-08-22 | 类型：refactor（字节不动点 + perf）

**变更说明：** F2 把 workspace 构建里 DepScan 的两大原语（`ZpkgReader.Open` + `TsigReconcile.Rebuild`）
进程级 memo，消掉 O(N²) 重复。本项（F2-邻）补掉残留里**同类可 memo 的一块**——`ZpkgReader.ReadModuleSigs(z)`：
它是 zpkg 字节的**纯函数**、跨成员恒定，但此前每个成员对每个 allowed dep zpkg **重新解码一遍 SIGS**。
在 `CachedZpkg` 加 `Mods` 字段，`DepScan.ScanDirs` 的 DepIndex 分支懒填 + 命中零重解码。

**为什么这块能 memo、AddModule 不能**（profile 实测 breakdown，workspace 24 成员，post-F2）：

| DepScan 子步骤 | 累计耗时 | memo? |
|---|---|---|
| `ReadNamespaces` | 2ms | 免（已几乎零成本） |
| **`ReadModuleSigs`** | **929ms** | ✅ 纯函数、跨成员恒定 → 本项 memo |
| `AddModule` | 2033ms | ❌ 每成员按 `declaredDeps` 建各自过滤 `DependencyIndex`，内容不同、不可共享 |
| `TsigReconcile.Rebuild` | 598ms | 已由 F2 memo |
| `Seal` | 116ms | 每成员，小 |

`ReadModuleSigs` 缓存后每 zpkg 的 SIGS 只解码一次；`AddModule` **仍每成员执行**（按其 declaredDeps 过滤
建 index，这正是 memory 里「AddModule 仍按成员过滤」的由来），故 index 内容/Seal 歧义剔除/字节全不变。

**字节不动点天然成立**：`ReadModuleSigs(z)` 是 z 字节纯函数，memo 只是把「解码 N 次」换成「解码 1 次 +
命中」，返回值逐条相同；AddModule 消费顺序/内容不变 → 24/24 stdlib zpkg sha256 逐字节一致 + self-host 5/5。

**实测收益（A/B swap，只换 `z42c.pipeline.zpkg`，其余全同，workspace 全量重编 3 轮交替）：** clean
均值 **34.13s**（min 33.86）vs cached **33.26s**（min 33.06），**delta −0.86s（−2.5%），min −0.79s，
3 轮 cached 全部快过 3 轮 clean（无重叠）**。这是 F 程序里继 F2（-71%）之后**第二个实测到墙钟收益的杠杆**，
与 profile 预测（省 ~929ms 冗余 ReadModuleSigs）一致。

**文档影响：** DepScanCache 机制页已有 F2 记述；本项属同机制延伸（多 memo 一个纯函数原语），以
`CachedZpkg.Mods` 字段头注 + `ScanDirs` 处头注承载「为什么可 memo / AddModule 为何不可」，不新增 book 页。

## 任务
- [x] 1.1 `CachedZpkg` 加 `ZpkgModuleSigs[] Mods`（懒填，默认 null；构造置 null）
- [x] 1.2 `DepScan.ScanDirs` DepIndex 分支：`ReadModuleSigs` 改走 `cached[pi].Mods` 懒填缓存
- [x] 1.3 字节不动点守卫：F2-邻 单独在 origin/main 上编出 **24/24 stdlib zpkg sha256 逐字节一致 ✅**
- [x] 1.4 profile breakdown 确认 ReadModuleSigs=929ms 可 memo、AddModule=2033ms 不可（见上表）
- [x] 1.5 A/B 计时（只换 pipeline zpkg）3 轮交替：clean 34.13s vs cached 33.26s，**−2.5%（真收益）**

## 验证
- [x] V1 完整 `xtask test` 全绿（C#-free）：self-host 5/5 gen1==gen2 · z42c [Test] · e2e · cross-zpkg
      · stdlib · vscode-syntax
- [x] V2 base=origin/main tip `43379dcb`，最新 nightly SDK-41 供种本地编译，无需 CI 兜底
