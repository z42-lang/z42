# Tasks: 可复现 build_id —— 同一份源 + 同一编译器，release zpkg 逐字节一致

> 状态：🟢 已完成 | 完成：2026-09-15
> 变更类型：`fix`（最小化模式）

**变更说明：** release（packed）zpkg 的 build_id 是主字节的 MurmurHash3。此前它对**构建环境**敏感：
同一份源换个目录编、或只改一行注释/空白，build_id 都会变。User 要求：相同代码与编译环境 ⇒ 输出不变，
改注释这类非实质修改也保持不变。

**原因：** zpkg MODS 头每模块写 `SourceFile` + `SourceHash` 两个字符串（`ZpkgWriter.z42:303-304`），二者进主字节：
1. `SourceFile` 写的是**发现到的绝对路径**（`PackageCompile.z42:290` 传 `files[i]` 原样），违背 `PackageTypes.z42`
   早已写明的契约「项目相对路径（跨机器可复现）」。
2. `SourceHash` 是源文本哈希 —— 注释/空白一动就变。
二者在 runtime（packed reader 读 `_src_idx` 即丢）与编译器（`ReadSourceHashes` 已无调用方，增量判定改走 `.meta`）
都**没有消费者**。实测：换目录编 3 个 stdlib 包，差异只有这两处 + BLID 本身；行号表在 release 下已在 `.zsym`。

**修法：** packed 产物写盘前把 `SourceFile` 改为项目（源根）相对路径、`SourceHash` 置空。
不改 wire 布局（不 bump 格式）；`.zbc` 增量缓存条目不含这两个字段（只在组装 zpkg 时写入）⇒ 不 bump 编译器指纹。
刻意**不**给 `CompileInputs` 等加新字段 / 新 API（分阶段引入纪律），只用既有 `ZbcFileZ` 字段 + `IncrementalBuild.Rel`。

**文档影响：** `PackageTypes.z42` 字段注释。

- [x] 1.1 `z42c.driver/src/BuildPaths.z42`：`_stabilizeSourceIdentity(z, projectDir)`
- [x] 1.2 `z42c.driver/src/Main.z42`：`isPacked` 时调用（单产物与多 exe 共用同一 `z`）
- [x] 1.3 `z42c.pipeline/src/Z42cCompiler.z42`：z42b 进程内编译路径同逻辑（相对 `req.SourceDir`）
- [x] 1.4 `z42.ir/src/PackageTypes.z42`：`SourceHash` 注释
- [x] 1.5 单测 `z42ccompiler_tests::test_release_zpkg_reproducible_across_dirs_and_comments`：换目录 / 只改注释 →
      逐字节一致；真改代码 → 不同；读回 `SourceFile == "lib.z42"`、`SourceHash == ""`。
      **阴性对照**：还原 `Z42cCompiler.z42` 重建后该用例 FAIL（values not equal）。
- [x] 1.6 e2e `scripts/build/xtask_compiler_e2e.z42::_e2eReproChecks`（driver 路径，挂 `test compiler`）
- [x] 1.7 手测：z42.collections / z42.test / z42.json 换目录编 zpkg+zsym 逐字节一致；z42.json 11 个文件加注释、
      改空白、插空行后 build_id 不变（`9a1938e9…`），改一处 `return true`→`false` 后变化
- [x] 1.8 GREEN：`xtask test` 全 stage 绿（10m56s），自举不动点 3/3
