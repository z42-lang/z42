# Tasks: fix-callee-entry-safepoint-drops-args

> 状态：🟢 已完成 | 创建：2026-09-10 | 完成：2026-09-10

**变更说明：** 把被调函数入口的 `check_safepoint` 从四个 `exec_function*` 入口（帧建立**之前**）
移进 `exec_function_body` 的 `push_frame` **之后**；`resolve_function_tokens` 一并挪到 push 之后。

**原因：** 原位置上，callee 的实参只存在于**调用方的临时切片**里（`args: &[Value]`），不是 GC 根。
对 `new T(..)`，那个临时里的 `args[0]` 是刚分配出来的接收者，堆里没有第二条引用 ——
在此处回收就把它扫掉，构造函数随后把一个已死的值写进字段，几百次回收后编译器崩在悬垂引用上
（`FieldGet … got Null`）。**与分代无关**：STW 压到 `Z42_GC_MAX_BYTES=8M` 同样复现。

**文档影响：** `docs/book/src/runtime/gc-tuning-and-safepoint.md`（新增「safepoint 只能放在
活值已经是根的位置」一节；删掉 known-open 一节；CI stage nursery 表改写）。

- [x] 1.1 `interp/exec_support.rs`：删掉四个入口的 `check_safepoint`
- [x] 1.2 `interp/mod.rs`：`push_frame` + `FrameGuard` 之后加 `check_safepoint`；
      `resolve_function_tokens` 移到其后（它会跑静态初始化器 = z42 代码 = 能到 safepoint）
- [x] 1.3 `scripts/test/xtask_test.z42`：分代 gate stage 的 nursery 16M → 1M（26 → 365 次回收）
- [x] 1.4 文档同步（book 机制页）
- [x] 1.5 GREEN（`./xtask test` 全绿，gcgen stage 15.0s @ 1M nursery）

## 定位过程（留给下一个碰到同类症状的人）

1. 探针挂在 minor 的 **mark 之后 / sweep 之前**：surviving owner 没有任何「年轻且未标记」的孩子
   → **不是标记漏了**。
2. 探针挂在 **每次回收之后**：有一条悬垂边，受害者 `age=0`、死于第 128 轮、**第 131 轮才被发现**
   → 这条边是在它死后才建立的 → 有人跨过一次回收还攥着它。
3. A/B：让 minor 的标记**穿透老对象**（`Z42_GC_PROBE_TRACE_OLD`），结果与对照**逐字节相同**
   → **卡表清白**，问题在根集合。
4. A/B：`Z42_JIT_THRESHOLD=4000000000` 关掉 JIT，照样复现 → 在解释器。
5. A/B：`Z42_GC_MODE=stw Z42_GC_MAX_BYTES=8M`（199 次回收）照样复现 → **与分代无关**。
6. 在 `exec_function` 入口 safepoint 的**前后各测一次实参存活**（前活后死才归罪于它）→ 命中。
7. 在写屏障处报「存入了一个已死的值」+ z42 栈 → 现场是构造函数
   （`Token.Token(Token,int,string,Span)` 存 `Z42.Core.Span`）。

## 备注

- **验证配方**：`env Z42_GC_MODE=generational Z42_GC_NURSERY_BYTES=1M artifacts/build/runtime/release/z42vm
  artifacts/.z42/programs/z42c/z42c.driver.zpkg -- build src/compiler/z42c.semantics/z42c.semantics.z42.toml
  --release --no-incremental`。修前 3/3 必崩；修后 365 次回收全绿。
- ⚠️ **zsh 不对未加引号的参数展开分词**，`for cfg in "generational 1M"; do set -- $cfg` 会得到一个参数
  → MODE 非法、NURSERY 为空 = **根本没武装**。量 GC 前先用 `Z42_GC_TRACE=1` 数一下回收次数
  （trace 里这条路径的 kind 打的是 `Cycle`，不是 `Minor`/`Full`）。
- **两头反证**（放回缺陷 / 干净树，各跑同一条命令）：
  | 树 | nursery | 结果 |
  |---|---|---|
  | 放回缺陷 | 1M | **红 3/3** |
  | 放回缺陷 | 16M（旧 gate 值） | 绿 2/2 ← **旧 gate 永远抓不到它** |
  | 干净树 | 1M | 绿 10/10，且「存入已死值」探针 0 命中 3/3 |
- ⚠️ **验证过程中踩过的坑**：做「放回缺陷」对照时，把改动文件拷到临时目录、`git checkout` 出旧版、
  跑完再拷回来 —— 拷回来的那份**带着我为对照加的那行 `check_safepoint`**，于是修复被悄悄回退了一半。
  之后的「残留缺陷」全是这行造成的（还一度被 JIT 开关的时序差异伪装成「JIT 侧的洞」）。
  **恢复之后必须 `git diff` 复核一遍，别信拷贝。**
- **不在本 change 范围**：翻 `Z42_GC_MODE` 默认（本 change 只清掉它的前置，翻不翻由 User 裁决）。
