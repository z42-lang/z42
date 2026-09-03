# Tasks: 编译器查找表去线性扫描（perf-compiler-lookup-tables）

> 状态：🟢 已完成 | 创建：2026-09-03 | 完成：2026-09-03 | 类型：perf（compiler + z42.ir）
**变更说明：** ① 新增 `z42.ir/StrIndex`（string→int 开放寻址，无装箱）；`ZbcStringPool.Intern/Idx`（每条带串指令
查池、BuildStrs 段字典）与 `IrGen.Intern`（字面量池）从全表线性扫描改 O(1)；② `Lexer._kwLookup` 从 84 项全表
线性比较改首字符分桶链（注册序数组保留，DumpTool / vscode-syntax 生成序不变）；③ `StrMap` 加无分配槽位遍历访问器
（`Capacity/UsedAt/KeyAt/ValAt`，槽序 = Keys() 序），`Z42ClassType.OverloadsOf` / `Conversion._findConvOn` 改槽位直扫
（去掉每次 `Keys()` 分配 + 逐键 Get 哈希）；三处 `Keys().Length == 0` 改 `Count() == 0`；`OverloadResolver._stripSpaces`
从 `Substring(i,1)` 拼接 O(n²) 改单趟 char[]。
**原因：** 三面评审 C-4 / C-5 / C-6 / C-7——大模块 STRS 池数千串 × 每条带串指令查一次 = 平方级；每个标识符最多 84 次
字符串比较（每次比较是一次 builtin 调用）。两者均不改产物字节（插入序数组不变，只加反查）。
**文档影响：** `src/libraries/z42.ir/README.md`（核心文件表加 StrIndex）；`.claude/rules/compiler-z42c.md`
Lexer 段「`_kwLookup()` 线性查」改分桶。

## 进度概览
- [x] 1. StrIndex + ZbcStringPool / IrGen.Intern 接入
- [x] 2. Lexer 关键字分桶
- [x] 2b. StrMap 槽位访问器 + OverloadsOf / _findConvOn / Count() / _stripSpaces（C-6/C-7；类型名缓存因 fixup 就地改写风险本轮不做）
- [x] 3. 对比数据：同一源码（z42c.semantics 包）改前/改后编译 wall time（hyperfine，同机、同 VM、各自 driver+libs）
- [x] 4. `xtask test` GREEN + 产物字节对比（预期除路径外逐字节一致）
- [x] 5. 文档同步 + 归档

## 对比数据（2026-09-03，macOS arm64，同一 z42vm，hyperfine -w 2 -r 8，单包 `--output-dir` 构建无增量缓存）
base = main ea983dfb 编译器（driver + libs），pr = 本分支编译器；输入源码同一份（wt-vcall 树）。

| 输入 | base mean ± σ | pr mean ± σ | 加速 |
|---|---|---|---|
| z42.core（96 文件 / 6.5k 行）| 5.440 s ± 0.080 | 5.111 s ± 0.072 | 1.06× ± 0.02（−6.0%）|
| z42c.semantics（28k 行，STRS 池最大）| 18.124 s ± 0.206 | 13.778 s ± 0.111 | **1.32× ± 0.02（−24.0%）**|

大模块收益主要来自 ZbcStringPool.Intern/Idx 去平方（semantics 池数千串 × 每条带串指令一次查池）；小模块以关键字分桶与
重载枚举免分配为主。

## 验证记录（2026-09-03）
- `xtask test` ✅ GREEN 12:48（不动点 3/3）。
- 产物字节对比（vs 同基线仅改 runtime 的 wt-vcall）：stdlib 25 包中 22 包逐字节相同；`z42.ir`（新增 StrIndex）/
  `z42c.syntax`（Lexer 改动）为源码变更；`z42c.core` 为增量缓存沿用的主树副本（仅调试路径差）；`z42c.driver` /
  `z42c.pipeline` 相同 → 查找结构改动不改任何输出字节。
