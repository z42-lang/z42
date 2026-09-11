# Tasks: z42b 接管测试目标

> 状态：🟢 已完成（刀一）| 完成：2026-09-12
> 规范：[proposal.md](proposal.md) · [design.md](design.md) · [specs/](specs/)
> **本 tasks = 刀一**（host + harness=true 跑通）。刀二另开。

## 编译器侧（internal 可见 + 判错）

- [x] 1.1 `PackageCompile.CompileInputs`：加 `ParentPkg` + `HasPkgContext`（三态，design §4）
- [x] 1.2 `ImportedSymbolLoader.Load`：加 `parentPkg` 形参；类与接口的 `IsImported` 按
      `pkgNames[i] == parentPkg` 判（design §3）
- [ ] （刀二）1.3 `DeclEnforcer`：非测试目标里出现 kind attr → E0457（三态，`--emit-zbc` 豁免）
- [ ] （刀二）1.4 `DiagnosticCodes.z42`：加 E0457（语义层先用字面量发码 —— F2 冷启动）

## z42b 侧（目标解析 + 建 + 跑）

- [x] 2.1 `DevTarget` 模型 + `_resolveDevTargets(m, kind, nameFilter)`（显式 + glob + 过滤）
- [x] 2.2 `_deriveManifest(m, target)` —— **内存派生，不落文件**；三层依赖合并；
      `Kind` 按 harness；**`Pack = true` 强制**；独立 `output_dir`/`cache_dir`
- [x] 2.3 `_runModule` 接目标路径：无 `--name` → 全部；有 → 只跑该目标；零匹配 → 报错列可选名
- [x] 2.4 CLI：`--name` 选项 + help 登记
- [x] 2.5 把父包名交给进程内编译器（接 1.1）

## 配置面收敛

- [ ] （刀二）3.1 `RunTarget`：`entry` 改可选（`harness=false` 不再必填，走 `AutoDetectEntry`）
- [ ] （刀二）3.2 歧义时的诊断（`"<ambiguous>"` → 提示写 `entry`）

## 收编 + 验证

- [ ] （刀二）4.1 `xtask_test_lib_units.z42`：合成 manifest 退休 → 转发 `z42b test`
- [ ] （刀二）4.2 单测：目标解析 / `--name` 选择 / 三层依赖合并
- [x] 4.3 跨包 golden `src/tests/cross-zpkg/dev_target_internal/`：
      测试目标能访问父包 internal；**普通消费包仍被拒**（回归保护）
- [x] 4.4 GREEN：`xtask test` 全 stage
- [x] 4.5 文档同步（cross-platform-testing 的阶段 ③、testing.md、error-codes ×2、book）

## 备注

（实施中的发现记这里）


## 完成记录（刀一，2026-09-12）

**GREEN**：`xtask test` 全 13 stage 绿（含自举不动点、gc modes、walkers）。

**端到端**：新 fixture `src/tests/z42b/dev-target-internal/` 经 `xtask test targets` 的
`_smokeDevTargetInternal` 驱动 —— `z42b test <toml>` 按 `[[test]]` 目标编译 + 运行，3/3 通过，
锁住两条此前不成立的性质：① 目标**只编自己声明的源**；② 目标**看得见父包的 internal**
（internal 类 + 公开类上的 internal 成员，两条判定路径都覆盖）。
回归方向（普通消费包仍被拒）由既有 `cross-zpkg/class_internal_access` 守着。

## ⚠️ 途中挖出**三个先于本变更就存在的缺口**（均已修）

### ① `IrGenFacts._visCode` 漏 `internal` 分支 —— 显式 internal 成员被编成 private

```z42
if (_hasWord(mods, "private"))   { return 1; }
if (_hasWord(mods, "protected")) { return 2; }
if (_hasWord(mods, "public"))    { return 0; }
if (_hasWord(mods, "override"))  { return 0; }
return dflt;        // ← 显式写的 `internal` 掉到这，变成 dflt = 1 = private
```

同文件的 `classVisCode` **一直有**那条分支。长期没暴露，是因为**跨包 internal 成员本来就一律拒绝**
—— 记成 private 还是 internal 结果一样；只有当测试目标可以合法访问父包 internal 时才顶出来。

### ② `SourceDiscovery._expand` 不认 `sub/x` 形态的 glob

只处理 `**/x`、`pre/**/suf`、裸 `x` 三档；`tests/queue_*.z42` 这种**带目录但无 `**`** 的落到
`Path.Glob(projectDir, pattern)`，而它只在 projectDir 单层匹配 ⇒ **静默返回空**，
表现为「no .z42 sources under \<dir\>」。

### ③ in-process 编译路径用**源目录尾名**当包名

`Z42cCompiler`：`inp.Name = Path.GetFileName(req.SourceDir)` —— 忽略 manifest 的 `[project].name`。
于是产物 zpkg 的 META 包名可与声明不符（目录 `dt/lib` + `name = "demo.lib"` → META 写 `lib`），
而依赖按 META 名解析 ⇒ 按名找依赖在这条路径上错位。加 `CompileRequest.PackageName` 修正。

## 其它实施记录

- **`[Record]` 加字段必须走 body**：`CompileRequest` 的三个新字段（`ParentPkg` / `Includes` /
  `PackageName`）都是 body 字段而非位置参 —— `z42.build` 是 z42c 的自依赖库之一，加 ctor 参数
  会撞种子 ABI（bootstrap-seed.md「残余真约束」）。
- **z42 自由函数不重载**（E0408）：`_orchestrate` 的多签名版本改名 `_orchestrateFor` / `_orchestrateWith`。
- **父包要并置**：编译期依赖经 `extraDepDir` 显式供给，但**运行期**依赖按 VM 搜索目录解析 ——
  不把父包 zpkg 拷到测试产物旁会撞 `undefined function <父包>.X`。
- **一次假阳性**：最早那次「internal 可见」验证是假的 —— 当时源没隔离，父包源被一起编进了测试目标，
  `Secret` 是本地类而非 imported。修好源隔离后真正的路径才跑起来（并随即暴露了缺口 ①）。
- **fixture 放置**：初版放 `src/tests/manifest-targets/` 会被 xtask 既有的 target 路径**再跑一遍**
  （走旧的合成 manifest，无 internal 放行 → 红）。移到 `src/tests/z42b/`；等刀二让 xtask 转发 z42b
  后，这层重复自然消失。
