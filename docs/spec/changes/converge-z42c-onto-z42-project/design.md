# Design: z42c 收敛到 z42.project

## Architecture

```
BEFORE                                    AFTER
─────────────────────────────            ─────────────────────────────────────
z42c.project (Z42.Project)               z42c.zpkg (Z42.Project.Zpkg*)
  ├ ProjectModel/ManifestLoader/           └ ZpkgWriter/Indexed/Reader/Builder
  │ SourceDiscovery/PathTemplate  ──删──┐    PackageTypes/CacheStore  （编译器后端，保留+改名）
  └ Zpkg*/PackageTypes/CacheStore        │
                                         └─► z42.project (Z42.Build.Project)  ← 唯一 manifest 模型
z42.project (Z42.Build.Project)              组合式 ProjectManifest/WorkspaceManifest
  未接编译（无 toml）              ──登记──►    + ManifestLoader/SourceDiscovery/PathTemplate
                                             （z42c.pipeline/driver 改引用它）
```

一处定义 schema、两处（z42c + z42.build）复用——正是 z42.project README 的设计目标。

## Decisions

### Decision 1: zpkg 后端独立成 `z42c.zpkg`，不保留 `z42c.project` 名
> **⚠️ 已被 [converge-z42c-ir-metadata-onto-stdlib](../converge-z42c-ir-metadata-onto-stdlib/proposal.md)
> 更正（2026-07-21，User 裁决）**：zpkg 后端**不再独立成 compiler-local `z42c.zpkg`**，而是**下沉
> stdlib 单库 `z42.ir`**（连同 IR 模型），因 REPL 需读写 zpkg → 后端是**格式契约**、天生可共享。
> `z42c.zpkg` rename 计划作废（本就未落地）；CacheStore（构建工具策略非格式）迁 z42c.pipeline。
> 下面原文保留作历史。

**问题：** 「删 z42c.project」不可能——它含 `ZpkgWriter`（持 zpkg 版本 pin 0/24）、`ZpkgReader`、`ZpkgBuilder`、`ZpkgWriterIndexed`、`PackageTypes`、`CacheStore`，是编译器产物后端，`z42.project`（纯清单模型）按设计永不含。
**选项：** A—重命名包 `z42c.project`→`z42c.zpkg`，名副其实；B—保 `z42c.project` 名，只剥 manifest-model 4 文件（包名从此名不副实）。
**决定（原）：** A（改名 z42c.zpkg）。**现更正为**：后端下沉 stdlib `z42.ir`（见上框）——「编译器产物机器」是**用法**不是**归属**，格式契约可共享。

### Decision 2: z42c 采纳组合式 `ProjectManifest`，不回填扁平层
**问题：** z42c.project 的 `ProjectManifest` 扁平（`Name`/`IncludeGlobs`/`HasOutputDir`…）；z42.project 组合式（`Project.Name`/`Sources.Include`/`Build.HasOutputDir`…）。约 30 调用点访问形态不同。
**决定：** z42c 直接改用组合式（z42.project 是最终设计，README 明示 z42c 后续引用它）。**不**给 z42.project 加扁平兼容层（那是打补丁）。迁移映射见下「Field Mapping」。`WorkspaceManifest` 缺的 `SharedVersion`/`SharedLicense`：勘察证 z42c 现无消费者（仅读 `ws.OutputDir`），**默认弃**；若实测发现 member 展开路径需要，则补回 z42.project（走设计完整性停点，不在 z42c 侧兜）。

### Decision 3【核心】: 分阶段形态由「present-unconsumed 是否炸自举」实测决定
**问题（自举死结）：** 两条约束疑似互斥——
- **种子轴**（[bootstrap-seed.md](../../../.claude/rules/bootstrap-seed.md) stdlib-API）：z42c 源一旦 `using Z42.Build.Project`，CI ci-bootstrap 用**上一版 nightly 的种子 stdlib** 编当前 z42c 源 → 种子 stdlib 必须已含 `z42.project.zpkg` → z42.project 须先随一个 nightly 发布。
- **串味轴**（workspace 注释）：`z42.project.zpkg` 与 `z42c.project.zpkg` 共存于 flat libs → first-wins 炸自举。
- 若两者都成立：「先发 z42.project」的那个 nightly 里两 zpkg 共存 → 该 nightly 坏 → 不能当种子 → **无解**。

**破结点（可实测）：** 串味轴的**精确触发条件**未定——是「两 zpkg 一共存即炸」，还是「只有当有代码跨 zpkg 按文件名解析到错的那份才炸」？z42c 对 manifest 类的调用是**同包显式依赖**（z42c.pipeline→z42c.project），理论上解析锁定本包；present-but-unconsumed 的 z42.project 未被任何 dep 链引用，**可能**不参与 z42c 的解析。勘察两方对此结论相反 → **必须实测**。

**候选排序（spike 结果二选一）：**

| spike 结果 | 分阶段 | 说明 |
|-----------|--------|------|
| **共存不炸**（unconsumed 安全）| **两 nightly**：① `publish-z42-project`（仅登记 z42.project member + 测试，z42c 不动）随 nightly N 发布 → ② 本 change（切 z42c + 删 manifest-model + 改名后端）随 N+1，此时种子已含 z42.project.zpkg | 干净、低风险。①可拆为独立小 change |
| **共存即炸** | 「先发」nightly 不可行 → 需**同批原子**（登记 z42.project + 删 z42c.project manifest-model + 切 z42c，同一 commit，flat libs 任一时刻只有一份同名文件）。但种子轴要求 z42.project 先在种子里 → 与原子矛盾 → **须先用一个 nightly 把 z42c.project 的同名文件重命名**（消除文件名碰撞），使共存安全，回到「共存不炸」路径 | 多一个 rename 前置 nightly；最后手段 |

**🔬 SPIKE 结果（2026-07-11，已实测）：共存即炸——落「须原子 / rename-前置」那档。**
临时登记 z42.project 为 member（z42c 完全不动）→ `xtask test compiler`：7 个 z42c.* 编出，但自举**崩**：
```
Error: type mismatch in comparison: I64(0) vs Null
  at Z42.Build.Project.SourceDiscovery.Discover     ← 解析到了 z42.project 的实现
  at Z42.Driver._build (line 97)
```
z42c.driver 本应调**自己依赖的 `Z42.Project.SourceDiscovery`**，但 flat-libs **跨 zpkg 按文件名 first-wins、无视声明依赖**，绑到了新 member `Z42.Build.Project.SourceDiscovery`（两者 Discover 行为不同 → 崩）。更严重：自举**自建**阶段会把这种错绑**烤进 z42c.pipeline.zpkg**（`ManifestLoader.LoadWorkspace` 绑到 z42.project），产出**污染的编译器产物**。**坐实 workspace 注释（权威），推翻勘察 agent 的「Phase 1 安全」判断。**

**据此定分阶段——「先发 z42.project」的 2-nightly 路径作废**（那个 nightly 里两 zpkg 共存即炸 → 坏种子）。剩两条候选，且**都还卡在 ci-bootstrap 种子轴**（converge 的 ci-bootstrap 用**上一版 nightly 的种子 stdlib** 编当前 z42c 源；当前源用 z42.project → 种子必须已含 z42.project.zpkg → 上一版 nightly 必须已发布 z42.project；但上一版发布 z42.project 又会共存即炸）——形成真死结，唯一破法是**消除文件名碰撞**：

- **rename-前置（推荐）**：先发一个 nightly，把 z42c.project 的 3 个碰撞**文件**（`SourceDiscovery.z42`/`ManifestLoader.z42`/`PathTemplate.z42`）**改名**（类名可留），使 z42.project 能与之安全共存 → 同一 nightly 登记 z42.project member。此 nightly 无毒、可当种子。**下一** nightly 再 converge（z42c 切 z42.project + 删这些改名文件）。**待验子问题**：改文件名（类名不变）是否足以避开 first-wins（机制是文件名还是类名？spike 只证了「同文件名同类名」会炸，未单独验「异文件名同类名」）——converge 实施前需补一个 micro-probe。
- **atomic**：登记 z42.project + 删 z42c.project manifest-model + 切 z42c 同一 commit（任一时刻 flat libs 无同名文件）。**但仍被种子轴挡**：converge 的种子（上一版 nightly）无 z42.project → 编不过。除非 ci-bootstrap 改为「先建当前 stdlib 再编 z42c 源」（用 fresh stdlib 而非种子 stdlib）——需核 ci-bootstrap 实际机制（`.github/actions/ci-bootstrap`）。

**下一步**：converge 阶段 2 实施前，(a) 补 micro-probe 验「异文件名同类名」是否避碰；(b) 核 ci-bootstrap 是 seed-stdlib 还是 fresh-stdlib 编 z42c 源。二者定「rename-前置」还是「atomic」。

**回滚记录**：spike 的 `z42.project.z42.toml` + default-members 条目已删（`git diff` 净）；污染的 in-tree z42c 产物已用 drop-tsig-expt 两代自举 scratch（gen2run 0.31 z42c + flat31 0.31 stdlib）重播种重建，自举不动点恢复 **7/7 green**。（恢复期踩到 tree 的格式-过渡态：`.z42` SDK 是 0.29、源已 0.31，属 `fix-bootstrap-format-bump-deadlock` in-flight territory。）

### Decision 4: byte-identical 策略
zpkg 写入器原地保留（`z42c.zpkg`），**不动格式**（zbc 1/20、zpkg 0/24 pin 不变）→ 用户 golden 零漂移（勘察 §5 已判定）。z42c 自身 zpkg 会因源改而变字节，但自举门禁比的是 **gen1==gen2 同源不动点**，非历史快照 → 只要迁移行为等价即绿。**行为等价 = 组合式字段值与扁平字段值逐一对应**（Field Mapping 是等价性的唯一判据）。

## Field Mapping（扁平 → 组合式；等价迁移的完整对照）

| z42c.project（扁平）| z42.project（组合式）| 调用点 |
|------|------|------|
| `pm.Name` / `pm.Version` / `pm.Kind` / `pm.Entry` / `pm.HasEntry` / `pm.HasPack` / `pm.Pack` | `pm.Project.Name` / `.Version` / `.Kind` / `.Entry` / `.HasEntry` / `.HasPack` / `.Pack` | Main.z42:94,125-259 |
| `pm.IncludeGlobs` / `pm.IncludeCount` / `pm.ExcludeGlobs` / `pm.ExcludeCount` | `pm.Sources.Include` / `.IncludeCount` / `.Exclude` / `.ExcludeCount` | Main.z42:97 |
| `pm.HasOutputDir` / `pm.OutputDir` / `pm.HasCacheDir` / `pm.CacheDir` / `pm.HasDistDir` / `pm.DistDir` | `pm.Build.HasOutputDir` / `.OutputDir` / `.HasCacheDir` / `.CacheDir` / `.HasDistDir` / `.DistDir` | BuildPaths.z42:25-73 |
| `pm.Deps` / `pm.DepCount` | `pm.Deps` / `pm.DepCount`（组合式仍顶层，**不变**）| Main.z42:160-330, WorkspaceBuild.z42:129 |
| `ws.MembersPatterns` / `ws.MembersCount` | `ws.Members` / `ws.MemberCount`（**改名**）| WorkspaceBuild member 展开 |
| `ws.HasOutputDir` / `ws.OutputDir` | 同名，**不变** | WorkspaceBuild.z42:67-88 |
| `ws.SharedVersion` / `ws.SharedLicense` | **无**（Decision 2：默认弃）| ManifestLoader.z42:88-89（仅解析，无消费）|
| `PathTemplate.Expand` / `TemplateContext` / `SourceDiscovery.Discover` | **逐字节相同**（勘察证）| BuildPaths/WorkspaceBuild |

> 迁移完成的判据：每处调用点按本表改完后，`xtask test compiler` 自举不动点 7/7 + 全 e2e/stdlib GREEN，且 dist 对账逐字节等价（若增量对账器可用）。

## Testing Strategy
- **阶段 0 spike**：`xtask test compiler`（临时登记 z42.project，判 present-unconsumed 是否炸）。
- **z42.project 首个 GREEN**：`tests/manifest-roundtrip/` [Test]——覆盖 `[project]`/`[sources]`/`[build]`/`[profile.*]`/`[[exe]]`/`[platform.*]` 解析回读 + PathTemplate/SourceDiscovery。
- **迁移等价**：全量 `xtask test`（e2e + cross-zpkg + stdlib + compiler 自举不动点 7/7 byte-identical + vscode-syntax）。
- **种子边界**：`xtask test bootstrap`（上一版 nightly z42c 能编当前源）——按 Decision 3 的排序确保不越界。
- **CI**：`bootstrap-no-csharp` 全量（下载 nightly → C#-free 重建全栈）为最终权威（cold 路径本地不可验）。
