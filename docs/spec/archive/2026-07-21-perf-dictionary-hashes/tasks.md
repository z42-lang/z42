# Tasks: perf-dictionary-hashes

> 状态：🟢 已完成 | 创建：2026-07-21 | 完成：2026-07-21 | 类型：perf（最小模式，行为保持）
> 子系统：`stdlib`（短占 converge 锁，隔离 worktree `z42-bench`）

**变更说明：** perf change #4——`Dictionary<TKey,TValue>`（开放寻址线性探测表，prelude）
热路径三处优化。

**原因：** perf 分析 + 新 bench 实测 `dict_set_get`（`Dictionary<string,int>` 64 插入 + 64
查找）1.030ms。原探测每步 `key.GetHashCode()` 重算 + string `.Equals()`（O(len)）派发 +
`% capacity` 取模；Grow 走 `Set()` 重算全部 hash。

**文档影响：** 无外部 API 变化。行为保持（探测语义、迭代顺序、扩容阈值均不变）。

- [x] **D1 存储 hash**：新增 `int[] hashes` 平行数组，Set 时存入掩码 hash
      （`GetHashCode() & 0x7FFFFFFF`）；探测循环命中判据改 `hashes[slot]==h && keys[slot].Equals(key)`
      —— int hash 不等直接短路，跳过绝大多数 string `.Equals()` O(len) 派发。
- [x] **D3 掩码取代取模**：capacity 恒为 2 的幂（初 8，2 倍扩容），`slot % capacity`
      → `slot & (capacity-1)`；探测步进、Remove 重插链、Grow 均改掩码。
- [x] **Grow 免重算**：扩容复用 `oldHashes[i]`（key 必唯一且新表全空 → 直接线性探测首个空槽，
      无需 Equals、无需重算 hash），不再走 `Set()`。
- [x] GREEN + 量化：`test stdlib z42.core` **8 文件全绿**（Dictionary 为 prelude，反射/集合/
      字符串测试全程重度依赖）；**dict_set_get 1.030ms → 0.881ms = 1.17×**（≈14%，该 bench
      loop 含 128 次 string 分配，故 dict 操作本身提速显著大于整体 14%）。

## 备注
- string key 上收益来自「存储 hash 短路 Equals」+「掩码免取模」+「Grow 免重算」三者叠加；
  int key（GetHashCode/Equals 均 O(1)）上主要收益是后两者。
- **D2（TryGetValue）不可行**：z42 无 `out` 参数，无法一次探测同时返回「是否存在 + 值」。
  留待 z42 加 out/Result 后重估。
- 后续 perf 靶子（承 change #3 备注）：Regex.Replace（R7 O(matches²)）、StringBuilder
  `Append(string)`/`Append(char)` 重载（P2）、parser run-based append（P1），及大 n 的 native
  `__str_join` / Thompson NFA regex——均留独立 change。
