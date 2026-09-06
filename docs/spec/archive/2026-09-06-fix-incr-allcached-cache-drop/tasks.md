# Tasks: 全命中但不能 preserved 时，增量缓存被整份丢弃

> 状态：🟢 已完成 | 创建：2026-09-06 | 完成：2026-09-06 | 类型：fix（最小化模式）

**变更说明：** `IncrementalDriver.Prepare` 在「全命中」时**无条件早退**，返回 `Cus = null` +
全 null 的 `cached[]`。调用方一旦**没走** `preserved` 捷径，`Main` 就落回全量 parse 分支、
`cachedMods` 全是 null → **整份缓存作废、全部文件重编**。把 preserved 的三个条件提到 probe 之前
算好（`canPreserve`）传给 `Prepare`，只有确实要 preserved 时才早退。

**原因：** 三个触发条件都是日常场景——
① dist 主文件被清但 cache 还在（清 dist / 换输出目录）；
② `packed ↔ indexed` 切换（`_distModeMatches` 为假）；
③ **`[[exe]]` 多目标工程**（`pm.ExeCount > 0` 恒不走 preserved）——这类工程只要缓存全命中就**永远**全量重编。
结果永远是对的，只是慢，所以逐文件 touch 的对账器抓不到（那条路径下 `AllCached` 恒为假）。
实测 xtask（64 文件）：**3.7s → 2.18s**。这也是「文件存在 = 最新」bug 族的近亲：一条为快路径设的
捷径，在快路径不成立时静默降级成最坏路径。

**文档影响：** `src/compiler/z42c.driver/README.md`（`Prepare` 的 canPreserve 契约）。

## 任务

- [x] 1.1 `src/compiler/z42c.driver/src/IncrementalDriver.z42`：`Prepare` 增 `canPreserve` 形参，
      早退条件改为 `plan.AllCached && canPreserve`
- [x] 1.2 `src/compiler/z42c.driver/src/Main.z42`：三个 preserved 条件提前算成 `canPreserve` 并传入；
      preserved 判定改用它（语义等价，不再重复求值）
- [x] 1.3 `scripts/test/xtask_test_incremental.z42`：对账器新增 `_reconcileDistWiped` 一轮
      ——清 dist（保留 cache）→ 增量重建 → 与全量 dist **逐字节**比对。这条路径**本次修复前从未
      真正用过 cache**，现在第一次走 cached 装配，必须有门看着
- [x] 1.4 文档同步（driver README）
- [x] 1.5 GREEN：`xtask test` 全绿（2m49s，exit 0）+ `xtask test incremental` 全绿（exit 0）
      ——含新一轮 dist-wiped：demo 6/6、xtask 66/66 与全量逐字节相等

## 备注

- 修复后 `AllCached && !canPreserve` 走的是**完整 Prepare 路径**：parse 全包（`want` 全 false →
  不捕获 idents/surface）→ 闭包空转 → 读回全部 cached zbc + 残留回填。`WriteMetas` 因 `Fresh` 全假
  而不写任何 meta，只刷包级源清单——与修复前的落盘行为一致。
- 实测佐证：修复后「清 dist + 全命中重装配」产出的 `xtask.zpkg` 与 `--no-incremental` 全量构建
  **sha256 完全相同**。
- **一处刻意接受的小代价**：`_distModeMatches` 现在每次增量构建都会求值（此前被 `AllCached`
  短路掉），而它内部是 `File.ReadAllBytes` 整读 zpkg 只为看偏移 8 的两个字节（xtask 约 520KB）。
  实测 ~1ms / 2250ms = 0.04%，且函数整体 try/catch 兜底、任何异常返回 false（安全）。
  改成只读文件头需要在 driver 里引入 `FileStream` 部分读——**新跨库 API 面**，踩自举轴 ③ 的风险
  远大于这 1ms 的收益，故不在本 change 做。
- 2.18s 同时也量出了增量的**地板**（编译 0 个文件）：读+哈希 → 全包 parse → 全包符号收集 + TSIG
  重算 → 读回 cached zbc → 装配 + zpkg 落盘。逐文件编译成本实测仅 ~31ms/文件，故地板才是大头。
