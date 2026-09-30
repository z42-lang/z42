# Tasks: 读侧线格式编解码收成一份（unify-package-codecs）

> 状态：🟢 已完成 | 创建：2026-09-30 | 类型：refactor（最小化模式；源自 2026-09-30 z42c 审查第 4 步「合并平行实现」）

**变更说明**：`.zbc` / `.zpkg` / `.zsym` 三个读者（`ZbcReader` / `ZpkgReader` / `SidecarReader`）各自手抄了同一套线格式的
解码：两个逐字相同的游标类（`ZpkgCursor` / `ZbcCursor`）、两个段目录类型、三份段目录解析、三份 STRS 解码、三种型参约束包读法
（完整 / 只跳过 / 只认 bit3+接口）、两份 SIGS 条目解码。抄本已经漂移：STRS 损坏时 `SidecarReader` 静默返回空池（帧名 / 文件名
全成 `""`，符号化「成功」却对不上），另两个拒绝；`ZbcReader` 的方法级约束包读法缺 bit2/6/7（writer 今天对方法级恒写空包才没炸）。

**原因**：两个游标并存的理由「z42c.ir 是叶子包，不能反向依赖」随两包合并为 `z42.package` 已不成立。

**文档影响**：`z42.package/README.md`（新增「读侧线格式」行）；reference `generic-constraints.md` 的 reader 名。

## 任务
- [x] 1.1 新增 `z42.package/src/ZpkgWire.z42`：`ZpkgCursor`（自 ZpkgReader 迁入 + `Str`）/ `ZpkgSection` / `SectionDir` /
      `StrsCodec` / `ConstraintCodec`（完整布局）/ `SigsCodec`（`ReadEntry` 原样条目 + `ToFunction` 唯一字段映射）/ `WireStr`（池索引越界抛）
- [x] 1.2 `ZbcReader`：删 `ZbcCursor` / `ZbcSectionZ` / `_readDir` / `_sec` / STRS 解码 / `_readConstraintBundle`；`_readSigs` 改用
      `SigsCodec.ReadEntry` + 本读者自己的归一化（全 `?` 参数名 → 空、全空每参 attr 块 → 0 长）；`_readFunc` 用 `SigsCodec.ToFunction`；
      `DecodeStrs` 保留为转发（跨成员单测在用）
- [x] 1.3 `ZpkgReader`：段目录 / META / STRS / SIGS 改用共享实现；删 `_skipConstraintBundle` / `_execName` / `_tag` / `_utf8`
- [x] 1.4 `SidecarReader`：段目录 / STRS 改用共享实现；**STRS 损坏 → `Read` 返回 null**（调用方既有「跳过 + warn」路径）
- [x] 1.5 `ZbcReaderInstr` / `tests/zpkg.z42`：`ZbcCursor` → `ZpkgCursor`
- [x] 2.1 `tests/wire.z42`：sidecar 正常 / STRS 越界被拒（**修复前红**：旧 SidecarReader 返回非 null）、StrsCodec 越界、
      约束包完整布局（哨兵字节断言游标不错位）、池索引越界抛
- [x] 2.2 字节中立核对：同一 worktree 先建 origin/main、再建本改动，`artifacts/build/**/dist/*.zpkg|*.zsym` 逐文件 shasum——
      除 `z42.package` 自身（源码变了）外全部一致
- [x] 3.1 文档：README / generic-constraints.md / constraint_bundle_tests 头注
- [x] 3.2 `xtask test` 全绿

## 备注
- `ZpkgCursor` 保留原名（不改成 ByteReader）：`z42c.pipeline/DepIdentity` 跨成员引用它，改名 = 新跨成员符号，须晚一个 nightly。
- **未做「写端预扫与写入合一」**：`ZbcWriter.InternPoolStrings` 的入池顺序就是 STRS 字节序，合成一遍（写时入池）必然改变池序 ⇒
  所有 `.zbc` / `.zpkg` 字节变化（格式不变）。漏入池今天已由 `ZbcStringPool.Idx` 当场抛出（#963），剩余收益是「少维护一份遍历」，
  代价是全量字节漂移 + golden 重生成——留给 User 裁决，不在本 change。
