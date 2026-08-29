# Design: 本地路径依赖（path 依赖）

## Architecture

```
z42.interactive.z42.toml
  [dependencies]
  "z42.core" = "0.1.0"              ← 名字依赖（stdlib）→ Z42_LIBS 按名解析
  "z42.repl" = { path = "../repl" } ← path 依赖（私有组件）→ 先建 + 并入 libsDirs + 随产物打包

z42c build z42.interactive.z42.toml
  │
  ├─ ManifestLoader.Load → pm.Deps[i].{Name, Version, Path}
  │
  ├─ PathDepPlan.Resolve(tomlDir, pm)        ← 新：path 闭包（传递+去重+拓扑序）
  │     叶子在前：[z42.repl, ...]，各带其 dist 目录
  │
  ├─ for dep in plan (拓扑序):                ← 先建依赖工程 → 各自 dist
  │     _build(dep.toml, ..., libsDirs = dep 的 path 闭包 dist + Z42_LIBS)
  │
  ├─ libsDirs(消费方) = [plan 全部 dist] + Z42_LIBS
  ├─ 编译 z42.interactive → 名字解析命中 repl dist 里的 z42.repl.zpkg
  └─ _bundleExeDeps: path 依赖无条件复制进 z42.interactive 的 dist（即便 z42. 前缀）
```

与既有 `WorkspaceBuild`（`--workspace`）的关系：两者都是"拓扑序先建被依赖者、per-member dist + Z42_LIBS 解析"。差异——workspace 靠**目录 glob** 发现成员、按**名字**连边；path 依赖靠 manifest 里**显式 path** 连边、可跨任意目录。PathDepPlan 借用 WorkspaceBuild 的拓扑/dist 范式，但以 path 边为图。

## Decisions

### Decision 1: `DepEntry` 加 `Path` 字段的构造函数形态
**问题：** 现 `DepEntry(name, version)`，唯一构造点在 `_parseDeps:224`。加 `Path` 怎么改最小且不破自举？
**选项：**
- A — 改成 3 参 `DepEntry(name, version, path)`，更新唯一构造点。受限子集无默认参数/重载依赖，全站改一处即可。
- B — 保留 2 参、加 3 参重载。z42 子集重载支持不稳，且徒增一个入口。
**决定：** 选 **A**。构造点仅 1 处（`new DepEntry(keys[i], ver)` → `new DepEntry(keys[i], ver, pth)`），`new DepEntry[0]` 数组初始化不受影响。字段默认由构造函数赋 `""`。

### Decision 2: 打包归属 —— z42.repl 是私有组件，colocate 进消费方（User 二次修正）
**问题：** `z42.repl` 名字 `z42.` 前缀，但它**不在 stdlib**（在 `src/toolchain/interactive/repl/`，host-only，只被 z42.interactive 消费）。它该私有复制进 z42i payload，还是留 stdlib libs？
**事实链（2026-08-29 查证）：**
- 运行期 [`app.rs:66`](../../../../src/runtime/src/app.rs) `support-colocated-zpkg-deps`：apphost **先搜 entry zpkg 自己目录、再搜 stdlib libs**。→ z42.repl.zpkg colocate 在 z42.interactive.zpkg 旁即可，运行期自动解析。
- publish 侧 [`_pubBundleProjectDeps`](../../../../src/toolchain/builder/core/builder_publish.z42) 判 stdlib 用**「`src/libraries/<name>/` 存在」+「已在 shipped libs」**（非名字前缀）。z42.repl 不在 `src/libraries/` → 本该复制；今天没复制**仅因 `_buildReplLib` 把它塞进了 libs**（`inLibs=true` → 跳过）。
- z42c 侧 [`_bundleExeDeps`](../../../../src/compiler/z42c.driver/src/Main.z42) 用 `dep.StartsWith("z42.")` 当 stdlib —— **这个判据对 z42.repl 是错的**（名字带 z42. 却非 stdlib）。

**决定（User 二次修正 2026-08-29）：** **z42.repl 是私有组件，colocate 进 z42i payload**。落实：
1. 删 `_buildReplLib`（不再把 z42.repl 塞进 libs）；path 依赖让 z42c 先建它（进自己的 dist）。
2. `_bundleExeDeps` 判据从名字前缀改为**与 publish 一致的真-stdlib 判定**：**path 依赖 = 私有 = 复制**；名字依赖里真 stdlib（在 `src/libraries/` 或 shipped libs）不复制。→ z42.repl（path 依赖）复制进 exe dist + colocate 进 payload。
3. z42.project 是真 stdlib（`src/libraries/z42.project/`）→ 判定为 stdlib、不复制、走 Z42_LIBS。不变。

> `z42.` 名字前缀不再作"是否 stdlib"判据——**是否 stdlib = 是否真属 `src/libraries/`（或 shipped libs），path 依赖一律私有**。这修掉了"名字带 z42. 就当 stdlib"的脆弱假设。

### Decision 3: 单工程 dist 解析辅助放哪
**问题：** 建 path 依赖要知道它产物落哪（消费方要把该 dir 并入 libsDirs）。dist 解析规则（`[build].dist_dir` + `${output_dir}` 模板，缺省 `<projDir>/dist`）现只在 xtask `_toolchainZpkg` 里有。
**选项：**
- A — 移植进 `z42.project`（如 `ManifestLoader.ResolveDistDir(manifest, tomlPath)`），z42c + 未来工具共享单一 SoT。
- B — driver 内私有小函数，不外溢。
**决定：** 倾向 **A**（单一真相、消灭 xtask 与 z42c 各持一份 dist 规则的漂移风险）。但 A 让 `z42.project` 承载更多 → 若发现 BuildConfig 已够表达则就地补。**最终 A/B 待实施时看 BuildConfig 现状定**，Open Question 记之。

> 注（D2 修正后）：被依赖工程用 `[build].dist_dir` 声明产物落哪。**z42.repl 声明 dist_dir = 共享 libs 目录**（镜像 `_buildWorkload` 那批 workload 库），产物落 libs、随 SDK 发货、运行期按名解析——这是"不私有复制、仍在 stdlib libs 找得到"的落点。消费方 build 仍把该 dist 并入 libsDirs（对 z42.repl 即 libs，已在 Z42_LIBS 内，冗余无害）。通用 path 依赖不声明 dist_dir 时默认 `<projDir>/dist`。

### Decision 4: path 依赖何时（重）建 —— 幂等与增量
**问题：** `z42c build <consumer>` 每次都要重建 path 依赖吗？warm 树重复建浪费。
**决定：** **总是走 `_build`（拓扑序），交给 z42c 既有增量缓存（.zbc cache）兜幂等**——warm 时增量命中≈no-op，冷时正常建。不额外加"dist 存在即跳过"启发式（会漏掉源改动）。path 闭包通常极小（interactive→repl 仅 1 层），成本可忽略。

### Decision 5: 环检测
**问题：** path 依赖成环（A path→B，B path→A）怎么办？
**决定：** PathDepPlan 拓扑排序时检测无进展 → 抛 `Exception("path dependency cycle: ...")`，镜像 `WorkspaceBuild.TopoOrder` 的环处理。

### Decision 6: 两阶段边界（自举）
**问题：** 为什么不能一个 PR 做完？
**决定：** `z42.project` 在 z42c 编译期依赖闭包里（driver+pipeline 都依赖）。z42c 源码读 `DepEntry.Path` 需**种子 stdlib 已含 `Path`**。种子=上一 nightly。故：
- PR-1 只动 `z42.project`（加字段+解析），z42c 源码一字不改 → 当前种子能编 → 发 nightly。
- PR-2 待 PR-1 nightly 发布后，z42c 源码才 `using` 到 `.Path`。
用 `xtask test bootstrap` 验 PR-2 未越界（[bootstrap-seed.md](../../../../.claude/rules/bootstrap-seed.md) 边界检查）。**无格式 bump、无新语法** → 不触发自举能力版本号（那是语法/格式轴）。

### Decision 7: 打包合并 —— 本 change 只做 colocated；合并交给正交的 single-file（User 裁决 2026-08-29）
**问题：** z42i payload 里 z42.interactive.zpkg + 私有依赖该"多文件 colocate"还是"合成一个"？
**分析（对标 .NET）：** 两种"合成"原语——
- **single-zpkg**（把私有依赖 zbc 源级合编进一个 zpkg）：换来少几个文件，代价是丢模块边界 / 改 `internal` 可见性 / 依赖被重编 / 无独立增量缓存。.NET 正因这些代价**刻意不做 assembly 合并**（ILMerge 是第三方、劝退）。
- **single-file**（把**分离的** zpkg 内嵌进 native apphost 二进制，对齐 .NET PublishSingleFile）：达到"一个可分发文件"而不牺牲模块边界；是部署模型的**正交 D 轴**。
**决定（User 裁决）：** **砍掉 single-zpkg**；"合成一个文件"由 single-file 承担。本 change **不引入 `[build].bundle` / 不做源级合编**，只做 **colocated**：path 依赖建成分离 zpkg、colocate 进 payload（`programs/interactive/`），运行期 [`app.rs` entry-dir search](../../../../src/runtime/src/app.rs) 解析。**零 runtime 改动、零 publish 魔法**（publish `_pubBundleProjectDeps` 本就 colocate 非 stdlib 依赖）。

### Decision 8: single-file 延后为独立 follow-up（需 runtime 支持内嵌 bundle）
**问题：** single-file 能否"纯 publish 合成、不改 runtime"？
**事实：** **不能**。真 single-file = zpkg 内嵌进 apphost 二进制 → apphost/vm 必须支持"从内嵌 bundle 解析 zpkg"（类 .NET host 读附加 bundle）。这是 runtime 侧改动，非纯 publish。
**决定：** single-file 记入**部署模型 book 页的规划 D 轴** + roadmap Deferred，作独立 change（自带 DRAFT）。**合成范围（future）**：只合**私有（path）依赖**，stdlib 永远外部（self-contained 时另由运行时嵌入，仍是分离 zpkg）。本 change 不碰。

### Decision 9: native 依赖 = path 依赖的另一半（同族 colocation；User 2026-08-29，Supersedes #332）
**问题：** path 依赖 colocate 私有组件的 **zpkg**（z42.repl.zpkg）进 payload；但 z42.repl 还有 native 库 `libz42_repl`（host-only cdylib）。native 库该怎么跟随组件？
**决定：** **native 库与 zpkg 同族——也 colocate 在消费方 zpkg 旁，运行期在声明它的 zpkg 目录解析。**
- **运行期布局唯一 = 平铺** `<zpkg-dir>/libX.<suffix>`（非标准库）。**标准库 native 不变**（`<sdk>/native/` eager + `[Native(lib=)]` 注册，已支持——本 change 不碰）。
- **多 rid → 发布期拍平**（User 简化）：不在运行期做 rid 子目录选择；**z42b publish 按目标 rid 把对的 native 平铺进 dist**（镜像 `_pubBundleProjectDeps`）。移动端复制到 OS 目录（jniLibs/`<abi>`、framework），运行期交 OS loader。→ 运行期 resolver 只有「平铺 beside zpkg」一条路径，dead simple。
- **按名定向、不盲扫**：resolver 只找**被声明需要**的库（按名），不 dlopen 目录里所有 `libz42_*`——从根消除 repl-WARN 那类盲扫跨污染。
**边界/nuance：** repl 不是 `[Native(lib=)]` ext-builtin，是带回调 C ABI 的专用 host-editor cdylib（repl 子系统 dlopen）。故共享的是**路径解析层**；repl 回调式加载仍专用。当前唯一非标准库 native = repl。

### Decision 10: 运行期 resolver 形态 + repl 接入（Supersedes #332）
**问题：** resolver 放哪、repl 怎么拿到「它的 zpkg 目录」？
**决定：**
- 抽 `native::ext::resolve_native_beside(zpkg_dir: &Path, lib_name: &str) → Option<PathBuf>`：试 `zpkg_dir/lib<name>.<平台后缀>`（复用 `parse_z42_lib_name` 的反向：`DLL_PREFIX`/`DLL_SUFFIX`）。
- **repl 接入**：`repl_native::candidates()` 里，repl 的「zpkg 目录」= interactive apphost 的 payload 目录 `<sdk>/programs/z42i/`。**#332 已实现**「从 apphost current_exe 派生 `<sdk>/programs/z42i/`」——本 change 直接搬 #332 的 repl_native diff，再把那段派生+平铺查找抽成调用共享 `resolve_native_beside`（供未来 `[Native]` 非标准库消费者复用）。packaging 侧（libz42_repl → programs/z42i/）**直接搬 #332 的 4 文件 diff**。
- **z42b publish native 复制**：`builder_publish` 加一步——按声明的 native 依赖 + 目标 rid，把 native 库平铺进消费方 dist（挂在 `_pubBundleProjectDeps` 邻位）。**当前无 `[native.dependencies]` 声明面**（唯一 native 消费者 repl 由 packaging 直接放），故此步先留**骨架/占位**，待 app native 声明面落地再充实（Deferred）。
- **为什么不改标准库 native：** compression 等是运行时横切库、随 SDK 分发、不属某个 app zpkg；保留 `<sdk>/native/` eager 现状最简，User 明确「标准库保持现在逻辑，已支持」。

## Implementation Notes

- **path 解析**：`depDir = normalize(Join(GetDirectoryName(consumerToml), dep.Path))`；`Glob(depDir, "*.z42.toml")` 恰 1 份 → 依赖 manifest；0/多份 → 报错。
- **PathDepPlan**（新 `src/compiler/z42c.pipeline/src/PathDepPlan.z42`）：DFS path 边，post-order（依赖先于消费方）累积，`in-progress` 集检环，`visited`（按规范化 toml 路径）去重。产出 flat 平行数组 `Tomls[] / DistDirs[]`（z42 无交错数组，仿 `WsMembers`）。
- **libsDirs 组装**：`_build` 里若 `pm` 有 path 依赖 → 先 `PathDepPlan.Resolve` → 逐个 `_build`（其 libsDirs = 它自己的 path 闭包 dist + 继承的 Z42_LIBS/override）→ 消费方 libsDirs 追加全部闭包 dist（去重）。
- **`_bundleExeDeps`**：改跳过条件（D2）；path 依赖复制 `<name>.zpkg` + `<name>.zsym` 进消费方 dist。运行期解析依赖 apphost payload 目录搜索——需验证 `_z42bPublish` 把消费方 dist 里 bundle 的 zpkg 带进 payload（`programs/z42i/`）。
- **DepScan 不变**：libsDirs 多目录扫描已支持（`ScanDirs`）；path 依赖只是多喂几个 dist 目录，名字解析逻辑零改。
- **declaredDeps 白名单**：path 依赖名仍进 `declaredDeps`（`pm.Deps[i].Name`），`_allowedForIndex` 放行——与名字依赖一致。
- **native resolver（D9/D10）**：`native::ext::resolve_native_beside(zpkg_dir, lib_name)` 用 `parse_z42_lib_name` 的 `DLL_PREFIX`/`DLL_SUFFIX` 反向拼 `lib<name>.<suffix>`，仅 stat 该单一路径（不遍历目录）。`repl_native::candidates()` 从 apphost `current_exe` 派生 `<sdk>/programs/z42i/` 作 zpkg 目录传入。#332 的 5 文件 diff 直接搬（packaging 4 + repl_native 1），仅把 repl_native 里 programs/z42i 派生+查找那段改为调 `resolve_native_beside`。

## Testing Strategy

- **单元（PR-1）**：`z42.project` 解析 `{ path = "..." }` / `{ version, path }` / 省 version / 纯字符串回落 → `DepEntry.Path` 正确/为 `""`。
- **端到端（PR-2）**：新建最小两工程 fixture（lib `foo` + exe `bar` 依赖 `bar.toml → { path = "../foo" }`），`z42c build bar.toml`：① 自动先建 foo；② bar 解析到 foo 符号；③ foo.zpkg 打进 bar 的 dist。
- **回归（PR-2）**：`z42.interactive` 切 path 依赖后，`xtask build toolchain` 产出 z42i、payload 含 z42.repl.zpkg、REPL 交互冒烟仍绿。
- **自举边界（PR-2）**：`xtask test bootstrap` 绿（上一 nightly z42c 能编当前 z42c 源）。
- **GREEN**：两阶段各自 `xtask test` 全 stage；PR-2 额外 `xtask test compiler`（z42c 自举）+ 手验 z42i。
- **native（PR-2 或 PR-3）**：`cargo build --release`（runtime）；`xtask test` 的 stage-components 断言 repl 在 `programs/z42i/`、不在 `bin/`；跑 `xtask test` 确认无 `ignoring unknown lib repl` spurious WARN（golden 不再被污染）；REPL 交互冒烟（行编辑 cdylib 正常 dlopen）。

## Deferred / Future Work

### native-future-app-declaration：`[native.dependencies]` app 声明面
- **来源**：本 change（native 折入）。
- **触发原因**：当前唯一非标准库 native 是 repl（由 packaging 直接 colocate），无 app 经 manifest 声明 native 依赖的真实消费者。
- **前置依赖**：出现「z42 app/类库经 `[Native(lib=)]` 依赖一个随工程分发的非标准库 native」的真实用例。
- **触发条件**：那时落 `[native.dependencies] libX = { path=..., rids=[...] }` 语法 + z42c/z42b 按 rid 拍平复制 + resolver 已就绪（本 change 铺好）。
- **当前 workaround**：repl 由 packaging `_pkgStageReplCdylib` 直接放 programs/z42i/；resolver 已通用可复用。

> roadmap 的 Deferred Backlog Index 需加一行索引指向此条（归档时落实）。
