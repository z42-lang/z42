# Proposal: 编译期函数内联 + 可独立开关的优化集（OptSet）

## Why

z42c 的编译期优化管线 `IrOptPipeline`（准则1 interp-first）目前只有**函数内**的
const-fold / copy-prop / temp-DCE / 代数恒等式,**没有任何跨过程内联**——最经典、ROI 最高的
缺失优化。函数内联消除每次调用的开销(interp:frame 分配 + 实参拷贝 + dispatch;JIT:`hr_call`
helper),**两个后端 + 移动端(纯 interp)通吃**,且解锁下游优化。调用密集 / OO 代码可两位数%。

但当前 `IrOptPipeline.Run` **无条件跑**(debug 也跑)——debug 已被 DCE/copy-prop 轻微扰动;加内联
会让**脚本调试时函数从栈回溯/断点/单步消失**。故优化必须**在调试构建默认关闭**,并且做成
**一组可独立开关的具名优化**:用户自助勾选开哪些(不是单调档位),profile 给默认。

## What Changes

1. **优化集 OptSet(枚举位集,取代数字档位)**:每个优化是一个**具名开关**——
   `ConstFold` / `CopyProp` / `Dce` / `Algebraic` / `Inline`。用户可任意组合开关。
2. **每个优化自洽、互不依赖(正确性)**:任一 pass 单独开启都必须正确(可互相增效,不得互为正确性
   前提)。这是本 change 对**所有**优化 pass 的设计约束。
3. **配置面**:
   - **profile 默认**:debug → `None`(空集,忠实可调试);release → `All`。
   - **用户覆盖**:toml `[optimize]` 逐项 bool + CLI `--opt <csv>` / `--no-opt <csv>`。
4. **函数内联 pass(新 `Inline` 优化)**:直接调用、同模块、小函数或单调用点、非递归的保守内联。

## Scope（允许改动的文件）

| 文件 | 变更类型 | 说明 |
|------|---------|------|
| `src/compiler/z42c.semantics/src/OptSet.z42` | NEW | 具名优化常量(位标志)+ `Resolve(profile, toml, cli)` + `Has(set, opt)` |
| `src/compiler/z42c.semantics/src/IrOptPipeline.z42` | MODIFY | `Run(irm, optSet)`;逐 pass 按 `Has(optSet, X)` 门控 |
| `src/compiler/z42c.semantics/src/IrInline.z42` | NEW | 内联 pass:资格判定 + 调用点展开 + 寄存器/块重命名 |
| `src/compiler/z42c.semantics/src/IrGen.z42` | MODIFY | `Generate(cu, model, optSet)` 透传 |
| `src/compiler/z42c.semantics/src/IrDump.z42` | MODIFY | 3 处 `Generate` 透传(dump 默认 None) |
| `src/compiler/z42c.pipeline/src/PackageCompile.z42` | MODIFY | 携 optSet,透传到 IrGen |
| `src/compiler/z42c.pipeline/src/Z42cCompiler.z42` | MODIFY | 从 req 解析 optSet(profile 默认 + 覆盖) |
| `src/libraries/z42.project/src/ManifestLoader.z42` | MODIFY | 解析 toml `[optimize]` 逐项 bool（manifest 模型已 converge 到 stdlib z42.project，非旧 `z42c.project`）|
| `src/libraries/z42.project/src/ProjectManifest.z42` | MODIFY | `OptimizeNames`/`Values`/`Count` 中性 name/value 对（default 空）|
| `src/libraries/z42.project/tests/manifest_roundtrip.z42` | MODIFY | `[optimize]` 解析单测（含连字符裸键）|
| `src/compiler/z42c.driver/src/Main.z42` | MODIFY | 解析 CLI `--opt` / `--no-opt`（toml `[optimize]` **消费**延后一 nightly，见 Out of Scope）|
| `src/compiler/z42c.semantics/tests/inline/` | NEW | 内联 + optSet 解析/门控单测 |
| `docs/book/src/compiler/optimization-pipeline.md` | MODIFY | OptSet + 独立性约束 + 内联机制/资格/不动点 |
| `docs/book/src/toolchain/`（构建配置页） | MODIFY | `[optimize]` + `--opt`/`--no-opt` |

**只读引用**：`src/compiler/z42c.driver/src/BuildPaths.z42`、`docs/design/compiler/self-hosting.md`。

## Out of Scope（v1 不做,后续 spec）

- **跨包内联**、**虚调用内联**、**带异常表/ref·out/闭包 callee 的内联**（v1 保守跳过）。
- **内联帧调试信息**(inline chain)→ v1 只映射 callee 源行。
- 新优化 pass(LICM / strength-reduction / 逃逸分析)→ 各自独立 spec,届时**各加一个 OptSet 具名开关**。
- **toml `[optimize]` 的 driver 消费**→ 延后一个 nightly（**两-nightly 纪律**，bootstrap-seed 轴 ②）：
  z42c.driver 源消费 z42.project 新 API(`OptimizeNames`/`Values`)会让冷启动 CI 用旧 nightly z42.project
  编当前 driver 源时缺 API 而红。本 change 只落**解析 support 侧**（z42.project）；driver 侧 `Opt.Resolve`
  的 `tomlBits`/`tomlMask` 已就位（当前传 0/0），随本 change 进 nightly 后再开 follow-up 让 driver 读
  `pm.OptimizeNames`→`Opt.ByName`→`Resolve`。届时补 `docs/book` toolchain 页的 `[optimize]` 用法。

## Open Questions
- [ ] toml 段名 `[optimize]` + 逐项 kebab（`const-fold`/`copy-prop`/`dce`/`algebraic`/`inline`）,OK?
- [ ] 内联体积阈值 INLINE_MAX_SIZE 初值(建议 24 IR)+ 单调用点恒内联。
