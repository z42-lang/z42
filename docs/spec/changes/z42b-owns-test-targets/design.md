# Design: z42b 接管测试目标

> 规范：[proposal.md](proposal.md) · [specs/](specs/)

## 1. 核心落法：**在内存里派生 manifest**，不落文件

`_buildProject` 的形状（[builder_commands.z42:37](../../../../src/toolchain/builder/core/builder_commands.z42#L37)）：

```
tomlPath → ManifestLoader.Load → ProjectManifest m
         → _makeTarget(r, mode) → Target t
         → _orchestrate(m, t, sourceDir, hooksRel)
```

⇒ **建测试目标不需要新的编译路径**。把 `m` **派生**一份（换 `Project.Name` / `Sources` / `Deps`），
走同一个 `_orchestrate`：

```
DevTarget { Name, Sources[], Deps[], Harness, Entry(可空) }
   │
   ├── _deriveManifest(m, target) → ProjectManifest（内存对象，**不写文件**）
   │        Project.Name = m.Project.Name + ".test." + target.Name
   │        Sources      = target.Sources（或 [tests].include 的 glob）
   │        Deps         = m.Deps ∪ m.TestsSection.Deps ∪ target.Deps   ← 三层合并
   │        Kind         = Harness ? "lib" : "exe"
   │        Pack         = true（z42b 只能加载 packed zpkg —— 强制）
   │
   └── _orchestrate(derived, t, sourceDir, "") → <pkg>.test.<name>.zpkg
```

xtask 现在做的「伪造 manifest 文本 → 落盘 → z42c 读回来」整条往返**消失**。

## 2. 目标解析

```
_resolveDevTargets(m, kind /* "test" | "bench" */, nameFilter) → DevTarget[]
  ① 显式：m.Tests[] / m.Benches[]（RunTarget：Name / Harness / Entry / Sources / Deps）
  ② glob：m.TestsSection / m.BenchSection 的 Include/Exclude → 扫出文件 → 按**今天的分法**成单元
  ③ nameFilter 非空 → 只留 Name == nameFilter（一个都没匹配 → 报错列出可选名，不静默跑零个）
```

`ManifestLoader` **已经把这些解析好了**（`pm.Tests` / `pm.TestsSection` / `pm.Benches` / `pm.BenchSection`），
z42b 直接读；**零解析工作量**。

## 3. `internal` 可见：父包身份直达编译器

**三条实测决定了这件事有多便宜**：

| 实测 | 结论 |
|---|---|
| `ClassExtractor` 零条可见性过滤 | 导出端不丢 internal |
| `ImportedSymbolLoader` 不跳过 internal 类，`nct.Visibility = cl.Visibility` | 导入端也不丢 |
| `AccessChecker.z42:40`：`if (!dc.IsImported) { return; }` | 跨包拦截**只靠一个布尔** |

⇒ **internal 成员本来就完整躺在 zpkg 里**，拦的只是「允不允许用」。

```diff
  public static ImportedSymbols Load(ExportedModuleZ[] exported, string[] pkgNames, int exportedCount,
-                                    string[] usings, int usingCount) {
+                                    string[] usings, int usingCount, string parentPkg) {
  ...
-     nct.IsImported = true;
+     // z42b-owns-test-targets：本次编译是 <parentPkg> 的测试目标 → 加载父包时按同包待遇
+     // （其 internal 可见）。其余依赖照旧跨包。parentPkg == "" → 恒 true（现状）。
+     nct.IsImported = !(parentPkg != "" && pkgNames[mi] == parentPkg);
```

`parentPkg` 经 `CompileInputs` 从 z42b 传入。**`AccessChecker` 一行不改**——它只读 `IsImported`。
`private`（比 `CurrentClass()`）与 `protected`（走基链）**都不经 `IsImported`** ⇒ 行为不变：
**给 internal，不给 private**，边界同 C# `InternalsVisibleTo`。

## 4. `[Test]` 只能出现在测试目标里

`DeclEnforcer` 用**三态**（不能用布尔——`--emit-zbc` 单文件无 manifest，误判会打爆 golden）：

| 状态 | 含义 | 行为 |
|---|---|---|
| 无 manifest | `--emit-zbc` 单文件 | **不判**（豁免） |
| 有 manifest、非测试目标 | 普通包 | **判错 E0457** |
| 测试目标 | `parentPkg != ""` | 放行 |

z42 无 nullable string 惯用法 → 用并行 `bool HasPkgContext`（同 `RunTarget.HasEntry` 的既有写法）。

## 5. `harness = false`：entry 自动探测

[`ZpkgBuilder.AutoDetectEntry`](../../../../src/libraries/z42.ir/src/ZpkgBuilder.z42#L178) **已经存在**，
四级优先：FQ `.Main` → 裸 `Main` → FQ `.main` → 裸 `main`；同级多候选 → `"<ambiguous>"`。

⇒ `[[test]] harness = false` 的 `entry` **改为可选**。不写即自动探测；歧义时报诊断提示写 `entry`
（比「必须先知道要写它」好）。`RunTarget.HasEntry` 保留作显式覆盖。

## 6. 运行

| harness | 产物 | 怎么跑 |
|---|---|---|
| `true`（默认） | lib zpkg | `Runner.RunModule(dist, format)` —— 反射跑 `[Test]`/`[Benchmark]`（**既有路径**） |
| `false` | exe zpkg | 直接执行，**退出码即判定**（刀二收编；刀一先支持 harness=true） |

多目标：逐个建 + 跑，任一非零 → 整体非零。零目标匹配 `--name` → 报错列出可选名。

> **零发现测试判红**（[#571](https://github.com/z42-lang/z42/pull/571) 已合）在这里继续兜底：
> 某个目标编出来一个 `[Test]` 都没有 → `Runner.RunModule` 判红，而不是静默绿。

## 7. 分期

| 刀 | 内容 |
|---|---|
| **刀一（本变更）** | 目标解析 + `--name` + 内存派生 manifest + `internal` 可见 + E0457 + **harness=true** 跑通；xtask `test stdlib` 转发 |
| **刀二** | `harness=false` 的 exe 路径、并行策略、其余 xtask 测试路径（targets/dist/cross）转发、`[tests]` 段缺失→不发现（破坏性，含补 28 个 manifest） |

## 8. 已知风险

1. **产物/缓存隔离**：xtask 现在给每个单元独立 `output_dir` + `cache_dir`（防两个单元的
   `source.zbc` 撞名）。派生 manifest 必须同样隔离，否则并行/串行都可能互相覆盖。
2. **packed 强制**：z42b 只能加载 packed zpkg；debug 默认 `pack=false` 会产出 indexed + 散装 zbc，
   runner 直接拒。派生时必须置 `Pack = true`。
3. **并行**：xtask 现在分批并行编多个单元。刀一先串行跑通（可能变慢），并行策略留刀二。
