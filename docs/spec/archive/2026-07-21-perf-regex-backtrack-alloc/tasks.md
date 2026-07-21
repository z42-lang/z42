# Tasks: perf-regex-backtrack-alloc

> 状态：🟢 已完成 | 创建：2026-07-21 | 完成：2026-07-21 | 类型：perf（最小模式，行为保持）
> 子系统：`stdlib`（短占 converge 锁，隔离 worktree）

**变更说明：** perf 攻坚 change #2——消除 z42.regex 回溯引擎的 O(k²) 重匹配 + 无捕获组的
每位置/每回溯步数组分配。行为保持（匹配结果完全一致）。

**原因：** perf 分析指出 regex IsMatch 7ms 的主因是量词回溯丢弃已记录的 `positions[]`、
从头重跑子匹配（O(k²)），以及无捕获组时仍分配快照/状态数组。

**文档影响：** 无外部行为变更（纯内部优化）。

- [x] R1 `Regex.z42` MatchQuant：回溯用 `positions[i]`（贪婪 pass 已记录的第 i 次重复末位）
      取代 `while(j<i)` 重跑子匹配（O(k²)→O(k)）；`_groupCount>1`（有捕获组）时保留重跑以
      重建子捕获槽。
- [x] R2 `ResetState`：复用 group-slot 数组（一次分配/组数变时重分配 + 就地 reset），取代每
      起始位置 `new int[groupCount]×2`；匹配成功时 Match 取**自有拷贝**（FindFrom 经 SnapshotStarts）
      → 无别名。
- [x] R3 `SnapshotStarts/Ends`：`_groupCount<=1`（无捕获组）返 null、`RestoreStarts/Ends`
      null no-op —— 消每 ALT/GROUP/QUANT + 每回溯步的 size-1 分配。Match ==1 时从不读该数组
      （Group(0) 用 start/end），传 null 安全。
- [x] GREEN + 量化：`test stdlib z42.regex` **13 文件全绿**（匹配行为一致）；bench diff（同机）：
      - **regex_is_match 6.97ms → 3.90ms = 1.79×** ✅
      - regex_find_all 2.15 → 1.74ms = 1.23×
      - regex_compile/replace ≈ 噪声（replace 的 R7 O(matches²) 拼接留 change #3）
- [x] 完整 gate 以 CI 为权威

## 备注
- 剩余 regex 成本（仍 3.9ms）= 解释器回溯 + 逐 CharAt builtin 分发（R6，需 char[] 输入）+
  根本的回溯算法（R5 Thompson NFA）。R1-R3 是隔离的高 ROI 增量步；NFA 重写是天花板、留战略级。
- childHasCapture 用保守 `_groupCount>1` 门（有捕获组即对所有量词重跑）——覆盖 bench 的无捕获
  模式；per-child 精确检测可后续 refine。
