# Tasks: 本包跨-ns 自有类被记成跨包依赖

> 状态：🟢 已完成 | 完成：2026-09-12
> 变更类型：`fix`（最小化模式）

**变更说明：** **单包构建一个多命名空间的库时，如果该库自己的 zpkg 躺在扁平 `Z42_LIBS` 里，
会编不过**——本包另一个命名空间被当成「跨包依赖」，要求写 `using`（E0436）。

实测 `z42.ir`：
```
z42c build src/libraries/z42.ir/z42.ir.z42.toml
  src/BinaryFormat/ZbcStringPool.z42(1,1): E0436: namespace `Z42.IR` is used but not imported…
```
把 `z42.ir.zpkg` 移出 `Z42_LIBS` 即刻通过。

**原因：** `EmitContext.ImportedClassNs` **有意含本包跨-ns 自有类**（其声明处 G17b 注释写明——
为了让 `Z42.IR.BinaryFormat` 里能把 `StrMap` 限定成 `Z42.IR.StrMap`）。而 `TrackImportedClass`
见名字在表里就无条件记进 `UsedDepNs`；后者的语义却是**跨包**依赖，它同时驱动 DEPS 与
`add-file-scoped-usings` 的 E0436。于是本包的 ns 被当成外部依赖。

DEPS 侧本就容忍这种自记（`ZpkgBuilder._addPair` 按 `selfZpkgFile` 事后过滤），是后加的 E0436
没跟上这条过滤。

**为什么长期隐形：** 两个条件要同时成立——① 包是**多命名空间**的（单 ns 的走
`ns == cu.Namespace` 放行）；② 该包**自己上一次构建出的 zpkg** 在扁平 `Z42_LIBS` 里。
`xtask build stdlib` 走 `--workspace`（那时 libs 里还没有自己），恰好绕开。
是 `xtask-forward-tests-to-z42b` 让父包走单包构建才撞上。

**修法：** `TrackImportedClass` 先查 `LocalClasses`（包级「自有声明类名集」）——本包声明的类，
其 ns 必属本包，不是依赖。

**排除过的方向（留给后来者）：** `DepScan.ScanDirs` 的 `excludeZpkg` 自排除**是生效的**
（探针确认 `skipped=yes`，且它裹住了 DepIndex 与导出符号）；在 z42b 侧把自身 zpkg 从 dep 列表
剔掉**无效**（`LibsDirs` 是按目录整扫的）。真正的来路是 `ImportedClassNs`。

- [x] 1.1 `EmitContext.TrackImportedClass`：本包自有类不记入 `UsedDepNs`
- [x] 1.2 回归单测（正反各一）：`codegen_tests.z42` 的
      `test_local_cross_ns_class_is_not_a_dep` / `test_genuinely_imported_class_is_tracked`
      ——后者确保修复没把依赖追踪修哑
- [x] 1.3 两个测试文件补真实缺失的 `using`（见下）
- [x] 1.4 GREEN：`xtask test` 全 13 stage 绿；自举不动点 3/3

## 顺带修掉的两处真实缺失 `using`

按包编译才会跑文件级 using 检查，旧的裸 `--emit-zbc` 单文件路径不跑，所以一直没人发现：
- `z42.net/tests/http_keepalive.z42` —— `Stream` 来自 `Std.IO`
- `z42.ir/tests/zpkg.z42` —— 用了 `Z42.IR.BinaryFormat` 的类型（`using` 不作用于子命名空间）
- `z42c.semantics/tests/codegen/codegen_tests.z42` —— 本次新增用例要的 `Z42.IR`（StrMap/StrBox）
