# Design: z42b test compile-then-test

## Architecture

`z42b test <target>` 的目标解析分叉（`_runModule`，builder_test.z42）：

```
z42b test <target> [--format pretty|json] [--release]
  target ∈ {.zbc, .zpkg}          → Runner.RunModule(target, format)        （已有，不变）
  target ∈ {.toml} 或 空(→z42.toml) → dist = _buildProjectDist(r)             （新增）
                                    → Runner.RunModule(dist, format)
  else                             → error（未知目标类型）
```

`_buildProjectDist(r)` 从 `_runVerb`（builder_commands.z42）的 build 分支抽出，build/publish/export
的 build 与 test 共用同一份 orchestrate + dist 组装逻辑。

```
_buildProjectDist(r) →  ManifestLoader.Load(toml)
                     →  _makeTarget(r, mode="")           // host RID, profile 由 --release
                     →  _orchestrate(m, t, sourceDir, hooks)
                     →  copy <Intermediate>/app.zpkg → <Dist>/<name>.zpkg
                     →  _pubBundleProjectDeps(...)         // deps bundle 到 dist 旁
                     →  return <Dist>/<name>.zpkg  ("" = 失败，已记录诊断)
```

## Decisions

### Decision 1: 复用 build，不新写编译路径
**问题：** compile-then-test 如何编译工程？
**选项：** A—`_runModule` 内联一套编译；B—抽 `_buildProjectDist` 复用 `_runVerb` build 分支。
**决定：** B。build 分支（orchestrate + copy app.zpkg→dist + dep-bundle）已是单一真相，test 只需
在其后接 `RunModule`。内联会复制 dist 路径推导 + dep-bundle 逻辑，违「单一真相」。抽助手后
`_runVerb` build 分支与 test 都调它，行为零漂移。

### Decision 2: 跑 dist zpkg 而非 intermediate app.zpkg
**问题：** 编完跑哪个产物？
**决定：** 跑 `<Dist>/<name>.zpkg`（其依赖已由 `_pubBundleProjectDeps` bundle 到旁边）。这与今天
`z42b test <artifact>`（xtask 恰好喂 dist zpkg，见 `xtask_test_targets.z42:268`）**完全同路**，
依赖解析不引入任何新问题。intermediate `app.zpkg` 无 sibling deps，不选。

### Decision 3: 后缀识别目标类型，不做文件内容探测
**问题：** 如何区分「已编译产物」与「工程 toml」？
**决定：** `.zbc/.zpkg` → 产物直跑；`.toml`（含 `.z42.toml`）或空(→`z42.toml`) → 工程编译；其余
后缀报错。最小、无歧义，与 `z42b build` 的 toml 约定一致（`_runVerb` 也按 `.toml` 路径处理）。

### Decision 4: test/bench 加 --release flag
**问题：** compile-then-test 用哪个 profile？
**决定：** `test`/`bench` ArgParser 加 `--release`（默认 debug——host 快编）。`_makeTarget` 已读
该 flag，无需改编排。已编译产物路径不受影响（不触发 build）。`--rid` **不加**（平台 deploy 属
阶段②core，out of scope）——`_buildProjectDist` 走 host RID。

### Decision 5: 无 target 默认 z42.toml
**问题：** `z42b test`（无 positional）今天报 "missing <file>"。
**决定：** 改为默认 `z42.toml`（对齐 `z42b build` 的 `_runVerb` 默认），更顺手。User 6.5 确认。

### Decision 6: Z42cCompiler 尊重工程 kind（扩展，User 裁决 Option 2）
**问题：** z42b 注入的 `Z42cCompiler`（wire-z42b MVP，`Z42cCompiler.z42:6-7,62,73`）**恒 `inp.Kind="exe"`
+ 强制 Main()**，忽略工程声明的 kind。→ 真实测试工程（lib 式、无 Main、只有 `[Test]`）编不了，
compile-then-test 对其无意义（实测 fixture `kind=exe` 报 "kind=exe but no Main() found"）。
**选项：** A—fixture 造 exe+空 Main()+[Test]，只盖 exe 工程（heuristic，回避编译器）；
B—Z42cCompiler 尊重 manifest kind（lib 免 Main）。
**决定：** **B**（User 裁决）。principled：manifest 的 `kind` 是真相源，编译器应尊重，而非要求测试工程
硬塞 Main。实现：
1. `ICompiler.CompileRequest` 加 `Kind` 字段（末位，"exe"/"lib"）。
2. `Pipeline.Compile` 构 req 传 `ctx.Project.Kind`（`IPipelineContext.Project : ProjectInfo` 有 `Kind`）。
3. `Z42cCompiler` `inp.Kind = req.Kind`（`""` → 默认 exe，保持既有 app 行为）。`PackageCompile` 的
   `isExe = inp.Kind=="exe"`、missing-Main 校验为 exe 专属（`PackageCompile.z42:247,287`）→ lib 天然免 Main。
4. `builder_hooks` 合成入口传 `"exe"`；3 个 z42ccompiler 测试构造点补 kind 参 + 新增 lib 编译测试。

**seed 维度（已核查，无两-nightly 风险）：** `CompileRequest`（record）+ 全部构造点（`Pipeline.z42`
在 z42.build、`builder_hooks` 在 z42b、测试在 z42c.pipeline）都随本 change 从当前源重建；加 record
字段=无新语法，cold-start 的 seed z42c 只需**编译**当前 z42.build 源（Gen0 `build --workspace` 整包
一致）。z42b 由**当前**重建的 stdlib 编（非 seed），故新 `Kind` 字段对其可见。

## Implementation Notes

- `_buildProjectDist` 与 `_runModule` 同属 `Z42Builder` namespace 跨文件自由函数，可直接互调。
- `_buildProjectDist` 失败即返回 `""`（诊断已在 orchestrate / manifest load 处打印）；`_runModule`
  见 `""` 直接返回非 0，不再 `RunModule`。
- `format` 选项仅 test 消费（`_buildProjectDist` 不读）；`--release` 仅 build 侧 `_makeTarget` 读。
- 顶注：删 builder_test.z42 的 "pending wire-z42b-host-build" 措辞，改述 compile-then-test 已接入。

## Testing Strategy

- **新 fixture** `src/tests/manifest-targets/compile-then-test/`：一个 test-kind 工程
  （`compile-then-test.z42.toml` kind=test + `src/*.z42` 含 1 个 `[Test]` 断言函数）。
- **fixtures stage smoke**（`xtask_test_fixtures.z42`，已在默认 GREEN gate 的 stage 4b）：加
  `_smokeCompileThenTest`——用已建的 `z42.builder.zpkg` 跑 `z42b test <fixture-toml>`，assert
  `rc == 0`（编译 + 全部 `[Test]` 通过）。直接覆盖新 toml→build→RunModule 路径。
- **完整 GREEN**：`xtask test`（全 stage）——确认 build/test 编排未回归、`z42b test <artifact>`
  旧路径不受影响（stdlib / compiler stage 大量走该路径）。
