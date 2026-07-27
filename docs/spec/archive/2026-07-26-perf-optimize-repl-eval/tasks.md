# Tasks: perf-optimize-repl-eval

> 状态：🟢 已完成（①②③⑤）| 创建：2026-07-26 | 完成：2026-07-26 | ④ defer → repl-future-persist-static-scan

**变更说明：** REPL 每轮 eval ~3.5s（几乎不可用）。根因：`Script.Eval → PackageCompile
→ DepScan` 每行都在解释器上重解整个 stdlib+编译器 zpkg 世界。分阶段消除。
**原因：** baseline 实测 5-eval=18.23s、per-eval~3.6s、启动仅 0.13s；成本恒定，是每轮固定开销。
**文档影响：** `docs/design/toolchain/repl.md`（Deferred repl-future-incremental-compilation 更新）；
`bench/README.md`（新 REPL bench）；`src/compiler/z42c.pipeline/README.md`（DepScan 行为不变，如涉及）。

## 子系统占用
- `compiler`（z42c.pipeline: DepScan / PackageCompile）
- `toolchain`（z42.scripting: ScriptState 缓存）——② 阶段
- ② 与已规划 `extract-compile-pipeline-api`（DepScan provider 抽象）重叠 → 需与 User 协调后再动

## 进度概览
- [x] ① DepScan 双读消除（decode 每 zpkg 一次）—— 实测 −10%（18.23→16.43s/5-eval）；双读非主导
- [x] ② 跨轮缓存依赖世界（CachedScan provider + ExtendWithPackage 内存增量）—— 实测每轮 3.5s→70ms（~50×）；5-eval 18.23→3.67s（−80%）
- [x] ③ 表达式→语句双编译消除 —— `_isStatement`（语句关键字 / 顶层赋值符）→ 语句直编，跳过表达式尝试
- [x] ⑤ 仅 var 声明轮发 Vars 类 —— 抹平 O(n) 增长：expr 轮引用现有 Vars{VarsRound}（赋值就地改静态、持久），只有 decl 轮并入缓存 scan + 前进 VarsRound。实测 expr 轮 ~72ms 恒定（20-eval 5.03s）
- [ ] ④ 静态 scan 跨会话磁盘持久化（消除首轮 ~3.3s）—— **成本/收益待 User 裁决**（见下）

## Profiling 定论（② 依据）
DepScan.ScanDirs = 每轮 ~3000ms / 98%；ImportedSymbolLoader ~50ms；BuildPackageCus ~10ms。
→ 缓存 DepScanResult 一次 + 每轮 ExtendWithPackage 增量并入 = 消除 98%。

## ① DepScan 双读消除
- [x] 1.1 `DepScan.ScanDirs` 主循环复用 world-open 阶段已解的 ZpkgInfo（parallel `opened[]`），
      每 zpkg 只 `File.ReadAllBytes + ZpkgReader.Open` 一次
- [x] 1.2 bench：xtask build compiler → 换入 .z42 → bench/repl/run.sh → −10%（BASELINE.md 已记）
- [x] 1.3 GREEN：full `xtask test` 全绿——e2e / cross-zpkg / stdlib / vscode-syntax ✓；
      z42c self-host 5/5 gen1==gen2 逐字节 ✓；z42c [Test] 20/20（含新增 ②⑤ 单测）✓
- [x] 1.4 文档同步：repl.md（状态模型 + carry-forward 机制 + Deferred ④）+ bench/repl/ + tasks

## 测试
- [x] pkgcompile 新增 `test_cached_scan_reused`（② CachedScan 复用）+ `test_extend_with_package_adds_namespace`（⑤ ExtendWithPackage）
- 手动 REPL 回归：carry-forward / 重赋值 / 重声明 / void 调用 / 字符串 / 错误恢复 全绿

## ② 跨轮缓存依赖世界（待 User 协调 extract-compile-pipeline-api 后）
- [ ] 2.x 设计入口：PackageCompile 接受已建 DepScanResult；ScriptState 缓存（libsDirs 不变即复用，
      session-dir 变化增量）

## ③ 表达式→语句双编译消除
- [ ] 3.x _classify 判 expr vs stmt，避免语句输入先编失败的表达式

## 备注
- baseline：bench/repl/BASELINE.md（2026-07-26，nightly SDK，5-eval=18.23s）
- 迭代环：编辑 → xtask build compiler/stdlib → cp zpkg 进 .z42/{programs/z42c,programs/interactive,libs} → bench/repl/run.sh
