# Design: 统一的包构建会话（BuildSession）—— 第一步

## Architecture

```
            今天                                        本 change 之后                      第二步之后
 z42c build ──► Main._build (700 行，完整)       z42c build ──► Main._build（不动）        z42c build ─┐
 z42b test  ──► Z42cCompiler (140 行，残缺)      z42b test  ──► Z42cCompiler ─┐                        ├─► BuildSession
                     │                                                        └─► BuildSession         │
                     └──────► PackageCompile.Compile（共享核心：依赖解析 / generator / 编译 / analyzer / 组装）
```

`PackageCompile.Compile(CompileInputs)` 已是纯编译核心（不写盘、不打印）。缺的是它**之上**那一层——
「读清单 → 决议全部旋钮 → 组 CompileInputs → 写产物」——今天在 driver 里有一份完整的、在 Z42cCompiler 里
有一份残缺的。BuildSession 就是这一层的唯一实现。

```
BuildOptions ──► BuildSession.Run() ──► BuildResult
  Manifest / ProjectDir / Role            ① ManifestKnobs：opt / syntax / lints / pack / strip（错误行 → reporter）
  IsRelease / CliAdd / CliRemove          ② 源发现（含 [build] hooks 排除）+ 读源 + hash
  BaseLibs / Tier / OutputZpkg            ③ [analyzers] 解析（handler zpkg 路径）
  IBuildReporter / IDeployStep            ④ 组 CompileInputs → PackageCompile.Compile
                                          ⑤ 声明依赖存在性检查、entry 校验、诊断分级 → reporter
                                          ⑥ 写产物（packed / 可选 .zsym）→ IDeployStep.AfterArtifact
```

## Decisions

### Decision 1: 会话放在 z42c.pipeline，部署留在 driver
**问题**：exe 装配、运行配置侧车依赖 z42.toml / z42.text 与「Z42_LIBS 下是不是框架包」的运行期事实。
**选项**：A — 全部搬进 pipeline（pipeline 多依赖 z42.toml / z42.text）；B — 部署经可空回调 `IDeployStep`，实现留 driver。
**决定**：B。z42b 不需要这些（HostTarget 传 null），pipeline 依赖面不扩大。

### Decision 2: 用显式 `Role` 替换隐式调用约定
**问题**：`_build` 靠 `libsDirsCount == 0`、`tier != null && MemberDirs == 0`、`tier == null && libsDirsCount > 0`
推断「顶层 / flat 成员 / 闭包子建」，三处隐式约定（今天的 pack 默认、闭包是否解析都依赖它）。
**决定**：`BuildRole` 常量（Root / WsFlat / WsMember / ClosureChild / AnalyzerChild / HostTarget）作为 BuildOptions 字段。
本 change 只实现 HostTarget 用得到的路径，其余角色在第二步随 driver 迁移补齐。

### Decision 3: `CompileRequest` 传清单对象
**问题**：会话需要整份清单。**选项**：A — 传 `ProjectManifest` 对象；B — 只传清单路径；C — 继续平铺字段。
**决定**：A。B 不可行——z42b 测试目标的清单是 `_deriveTargetManifest` 在内存里派生的；C 每加一段就要改 ABI。
以 body 字段加入（`public ProjectManifest Manifest = null;`），不进 ctor（种子 ABI）；为 null 时退回今天的平铺字段
（兼容旧调用方，第二步删）。

### Decision 4: 诊断经 reporter，不直接打印
BuildSession 从不写 stdout/stderr。`IBuildReporter` 四个回调：`Progress` / `ProgressErr` / `FileDiags(isError, origin,
msgs, n)` / `Fail(code, msg)`。Z42cCompiler 用收集型 reporter，把错误行与警告行回填进 `CompileResult.Diagnostics`
/ `Warnings`；第二步的 `DriverReporter` 逐字复现 z42c 今天的文案（`z42c build:` 前缀、`cached:`、`wrote ->`）——
学习手册会话重放与 `--quiet` 依赖这些文案。

### Decision 5: 两步走，第一步不碰 driver
driver 引用 pipeline 新符号属「新跨成员符号」，按 bootstrap-seed.md 走 support 先行、晚一个 nightly 再 use。
本 change 期间 BuildSession 与 `Main._build` 并存（旋钮决议两份）——`ManifestKnobs` 把可共享的纯逻辑先抽出，
第二步 driver 直接改用它并删掉自己的那份，重复窗口只有一个 nightly。

## User 裁决（2026-09-30 已确认：全部按下表默认取值）

| # | 问题 | 默认 |
|---|---|---|
| 1 | z42b 的警告怎么呈现 | 经 `ctx.Warn`（z42b 既有通道，stderr），前缀 `warning:`；错误维持现状 |
| 2 | z42b 路径是否立即执行 E0497 与「声明的依赖找不到」 | **立即执行**，与 `z42c build` 同一判据（stdlib 自动可用的包照旧豁免）——同一工程两条构建路径必须同判 |
| 3 | z42b 是否认 `[project].pack` / `[[exe]]` | 不认：HostTarget 恒 packed 单产物（z42b 只能加载 packed，见 builder_dev_targets:146）|
| 4 | z42b 路径的增量缓存 | 不做（Out of Scope）|
| 5 | analyzer path 条目代建写到 Dirs 之外 | 允许（与 z42c 相同：写进 analyzer 工程自己的 dist）|

## Implementation Notes

- 字节不变量：本 change 不改 driver ⇒ gen1==gen2 自举不动点与 z42c 产物字节不受影响。z42b 产物字节**会**变
  （opt / version / entry 生效），这是本意。
- `ProjectManifest` 构造器默认各段为空；`_deriveTargetManifest` 必须逐段拷贝（optimize / syntax / lints / analyzers / entry），
  否则测试目标照旧丢段。
- `BuildOptions.OutputZpkg` 非空时产物写到该路径（z42b 的 `Intermediate/app.zpkg`），`.zsym` 同名旁挂。
- 行数：BuildSession 新文件 < 500 行。

**实施中的偏差（2026-09-30 落地时记录）**
- **`IDeployStep` 未引入**：HostTarget 没有部署步骤，本 change 无消费方；随第二步 driver 迁移（exe 装配 / 运行配置侧车）一并加。
- **`CompileRequest.ProjectDir` 未加**：`req.SourceDir` 就是清单目录，直接作为 `BuildOptions.ProjectDir`。
- **`IBuildReporter` 收敛为三个回调**：`Progress` / `Diags(isError, origin, msgs, n)` / `Fail(msg)`；stderr 进度与退出码映射
  留给第二步的 `DriverReporter`（按需再加）。
- **Z42cCompiler 旧路径保留**，没有缩成薄封装：`builder_hooks.z42` 编 hook 目录时仍不带清单（`Manifest == null`）。
- **HostTarget 下 `[analyzers]` path 条目代建为 `AnalyzerChild`**（子 BuildSession、产物写 analyzer 工程自己的 dist，裁决 #5）；
  带 path 依赖的 analyzer 直接拒绝（闭包代建属第二步）；generator 生成源不落盘（z42b 无 `obj/` 约定）。
- **纯测试工程的父包不列为依赖**：`_deriveTargetManifest` 新增 `parentDep` 参数，父包无源（不会被建）时不把它放进
  `[dependencies]`——否则新的依赖核验撞「未找到 <父包>.zpkg」（`manifest-targets/basic` 实测）。
- **夹具修正**：`manifest-targets/compile-then-test` 写的是未加引号的点号键 `z42.test = "*"`，TOML 解析成依赖名 `z42`；
  旧路径不核验依赖所以一直没暴露。改为 `"z42.test" = "*"`。
- **测试覆盖收窄**：smoke 只锁 `[syntax]`（E0301，父包与派生目标两半各自回退验证过红）；E0497 / 警告回传没有单独的 smoke——
  依赖核验由 `tests/buildsession` 的缺失依赖单测覆盖，警告回传路径只有结构性覆盖（`Warnings == ""` 断言），触发警告需要
  analyzer 夹具，留待第二步随 driver 的 analyzer e2e 复用。

## Testing Strategy

- 单元（`z42c.pipeline/tests/buildsession`）：ManifestKnobs 的 opt / syntax（未知名报错）/ lints / pack 决议；
  BuildSession 在 HostTarget 下：版本号与 entry 进 META、E0497、缺失依赖报错、警告进 `BuildResult`、release 产出 `.zsym`。
- 单元（`tests/z42ccompiler`）：`CompileRequest.Manifest` 设了与未设两条路径。
- smoke（`xtask test` 的 targets 段）：`z42b test src/tests/z42b/manifest-sections/z42.toml`——`[syntax]` 关掉一个特性后
  测试源用到它必须报 E0301；未声明依赖报 E0497；警告出现在输出里。
- 修复前红：smoke 与单测在今天的 Z42cCompiler 上必须失败（逐条记录）。
- `xtask test` 完整 GREEN（含 compiler 自举不动点、lines）。
