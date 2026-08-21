# Tasks: perf-pipeline-skip-usings-reparse（F1）

> 状态：🟢 已完成 | 创建：2026-08-21 | 完成：2026-08-21 | 类型：refactor（纯内部，行为不变）

**变更说明：** 停止「仅为聚合 `using` 而对每个源文件全量 reparse」——包编译时已有解析好的
`CompilationUnit[]` 在手，直接读其 `UsingDecl` 收集 usings，零 reparse。

**原因：** `PackageCompile.Compile` 为聚合 `allUsings` 对每源调 `IrDump.ExtractUsings`，后者内部
做完整 `new Parser().ParseCompilationUnit()`——但 `inp.Cus[ui]`（已 parse）此刻已在手，且下一句就
把它传进 `BuildPackageCus`。`BuildPackageCus` 缓存分支（`IrDump.z42:164`）对每缓存文件再 reparse
第三次。直击 `IncrementalDriver` 自己标注的「lex 恒定成本吃掉全部增量收益」。

**文档影响：** 无（纯内部实现调整，不改对外行为 / 机制 / 命令面 / 格式）。改的两个文件所属目录
（z42c.semantics / z42c.pipeline）README 的功能索引 / 核心文件不变（无新增删除文件、无对外入口变化）。

**前置确认（已验）：** `inp.Cus` 在所有 `Compile` 入口均非 null 且元素非 null——
`BuildPackageCus:121` 已无条件 `Extract(cus[i],...)`，任一 null 早崩；三处 `CompileInputs` 构造
（Main.z42:224 driver / Z42cCompiler.z42:59 ParseAll / pkgcompile_tests.z42:21 ParseAll）均赋非 null
CU 数组。改动不引入任何新前置。

## 任务
- [x] 1.1 `IrDump.z42`：抽 `public static string[] UsingsOf(CompilationUnit cu)`
      （即 `ExtractUsings` 里 UsingDecl 收集循环，含 8→扩容修复）
- [x] 1.2 `IrDump.z42`：`ExtractUsings(src,file)` 改为
      `return UsingsOf(new Parser(src,file).ParseCompilationUnit());`——**保留文本入口**
      （仅供 `Main.z42` 的 `--emit-zbc` 无-CU 独立路径）
- [x] 1.3 `IrDump.z42:164`：缓存分支 reparse `ExtractUsings(srcs[i],files[i])` 换成 `UsingsOf(cus[i])`
- [x] 1.4 `PackageCompile.z42:101`：`ExtractUsings(inp.Texts[ui],inp.Files[ui])` 换成 `UsingsOf(inp.Cus[ui])`
- [x] 1.5 性能对比：baseline 干净重建 stdlib 102.59/102.43s；F1 100.57/99.35/100.19s →
      **-2.3s（~2.2%）**。核验合理：≈ 全 332 个 stdlib 源（4.8 万行）少 parse 一遍
      （~6.9ms/文件、~47µs/行，interp 解析器 sane）。增量 driver dev-loop 收益更大。
- [x] 1.6 字节不动点守卫：baseline / F1 编出的 stdlib 合并 sha256 **逐字节一致**
      （`27631e03…`）——证明行为完全保持。

## 验证
- [x] V1 完整 `xtask test` 全绿：e2e interp 256/0 · cross-zpkg 全 PASS · stdlib · z42c [Test] 25 units ·
      **self-host 不动点 5/5 gen1==gen2 逐字节复现** · vscode-syntax → GREEN all stages (C#-free)。
