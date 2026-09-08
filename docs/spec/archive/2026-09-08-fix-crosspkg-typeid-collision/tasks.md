# 实施清单：fix-crosspkg-typeid-collision

> 分支 `fix-crosspkg-typeid-collision` / worktree `../z42-assert` @ origin/main `54c8a1df`

---

## 0. 排查（证据链，已完成）

- [x] Exp1 只删 `Assert.Skip` 方法 → 绿（排除方法集合维度）
- [x] Exp2 只删 `SkipSignal` 类 → 红（锁定类集合维度）
- [x] 探针实证 `PIC-MISMATCH recv=CompileCuTask recv_id=139 -> callee=SrcReadHashTask.Run`
- [x] 反向对照：未改动 main + 同探针 VM → 0 mismatch / 25-25 绿（latent-but-live）
- [x] 核实 `type_by_id` / `register_lazy_type` / `type_registry_vec` 生产零消费者
- [x] 核实 `src/tests/` 无 golden 捕获 `TypeId(` ⇒ 无字节/文本漂移风险

## 1. 正确性修复（commit 1：`fix(runtime)`）

- [x] `metadata/tokens.rs`：`alloc_type_id_block(n)` —— 进程级 `AtomicU32` 批量发号，越界 `panic!`
- [x] `metadata/loader/type_registry.rs`：`next_type_id` 改从发号器取
- [x] 更新 `TypeId` / 模块级 doc-comment（per module → 进程全局唯一，并写明**为什么**）

## 2. 常驻断言门 + 回归单测（commit 2：`fix(runtime)`）

- [x] `interp/vcall_resolve.rs::assert_pic_target`：PIC 命中时校验 callee 的 declaring class
      在 receiver 的继承链上（走 `is_subclass_or_eq_td`），不匹配 `panic!`
- [x] `metadata/resolver/ic.rs::assert_field_ic_slot`：校验 `field_index[name] == cached_slot`
- [x] 两者 `#[cfg(debug_assertions)]`，release 侧为 `#[inline(always)]` 空函数 ⇒ 热路径不变
- [x] 接线：interp `exec_vcall.rs` / `exec_object.rs`(×4)、JIT `helpers/vcall.rs` / `helpers/object_field.rs`(×4)
- [x] 删掉临时 `Z42_PIC_PROBE` env 旁路
- [x] `loader_tests.rs::type_ids_are_unique_across_modules`：两个 Module 各跑
      `build_type_registry`，断言 id 集合不相交
- [x] 🔴 **退回对照已做**：把发号改回 `= 0` 重跑，测试如期红
      （`PkgA got {0, 1, 2}, PkgB got {0, 1, 2}`）⇒ 这是一道会红的门
- [x] `loader_tests.rs::allocated_type_ids_stay_below_import_base`：号不越进 `IMPORT_BASE`

## 3. 死设施清除（commit 3：`refactor(runtime)`）

- [x] 删 `Module::type_registry_vec` / `type_by_id` / `register_lazy_type`（`impl Module` 整块空掉）
- [x] 删 `type_registry.rs` 的 `registry_vec` 与下标不变量 `debug_assert_eq!`
- [x] 删 `lazy_loader/registry.rs` 的两处 `.clear()`
- [x] 删 `loader_tests.rs` 的 Phase 3 S1 测试块（243–405）+ 9 处 `.type_registry_vec.clear()`
- [x] 清 17 个文件里 `Module { … }` 字面量的 `type_registry_vec:` 行 + `jit/vm_interface_tests.rs` 的构造
- [x] 行数：`tokens.rs` 188 / `ic.rs` 270 / `vcall_resolve.rs` 290，均在软限 500 内；
      `loader_tests.rs` 1043→943（`_tests.rs` 本就被 `xtask test lines` 排除）

## 4. 回归门选型（design.md D5 定稿）

- [x] 放弃多-exe fixture，采用 Rust 单测 —— 理由：修复后撞键在源码层已**无法构造**，
      fixture 只能退化成正向门；而那条正向路径（`ParallelFor.Run` 同时接两个 zpkg 的
      `IParallelBody` 实现）**每次构建都在跑**，再加 fixture 是重复覆盖。负例由
      §2 的可退回对照单测 + 常驻断言（全局覆盖）守。理由已写进 design.md D5。

## 5. 端到端验证

- [x] 干净供种 + **Exp2 差分**（原本必红）+ 修好的 VM → **25/25 绿，0 `✗`，REAL_EXIT=0**
- [x] 干净供种 + 未改动 main + 修好的 VM → **25/25 绿**（无回归）
- [x] `cargo test` 全量（含 `tests/` 集成测试）→ **1223 passed / 0 failed**
- [x] ⚠️ `signal_handler_e2e` 在本机卡死 —— **已做退回对照**：`git stash` 掉全部 runtime
      改动（= 干净 origin/main）后**同样卡死 >180s**；同机上还有 z42-subclass /
      wt-objblock / z42-lang 三个无关 worktree 留下的 `signal_crash_helper` 卡了 3–4 天
      （9/4、9/5 启动）⇒ 环境问题，与本改动无关。该目标不在 `xtask test` GREEN gate 内
- [x] `xtask test`（完整 GREEN gate）→ **✅ GREEN — all stages passed**，0 `✗`，3m39s
- [x] `xtask test stdlib --mode jit` → **331 file(s) passed / 0 failed**（23 lib）
- [x] bench：**未跑，理由记录如下** —— release 构建下本改动的可执行差异只有
      「load 期发号从 `= 0` 变成一次 `fetch_add`」，两道断言由 `cfg(debug_assertions)`
      编译掉、PIC 查找与 JIT codegen 一个字节未动 ⇒ 热路径无可测面。且本仓 micro A/B
      门已知误报率高（见 [[investigate-micro-ab-false-regressions]]），跑它只会引入噪音

## 6. 文档同步（与代码同 PR）

- [x] `docs/design/runtime/vm-architecture.md`：新增「TypeId 的作用域：为什么必须进程内全局唯一」
- [x] `docs/book/src/runtime/interp-jit-semantics.md`：新增「PIC 的键必须是全局类型身份」
      （与既有 `IsaCache` 键指针身份的段落对照成一组）
- [x] `.claude/rules/common-pitfalls.md` §2：「在作用域 S 内发的号，不得拿到 S 之外做相等比较」
- [x] 归档 `docs/spec/changes/fix-crosspkg-typeid-collision` → `archive/2026-09-08-…`（**同 PR 内**）

## 7. 收尾

- [x] `scratch/` 本就在 `.git/info/exclude` 内，不进仓
- [ ] PR body 按 `parallel-development.md` §1.1 三段式
- [ ] 合并前 rebase origin/main + 重跑完整 GREEN
- [ ] 合并后删远程/本地分支 + worktree

---

## 复跑配方（排查期建立，留给后来人）

```bash
cd ../z42-assert
./scratch/reseed-build.sh <标签>     # 干净供种 + xtask build stdlib + 自动 grep ✗ + REAL_EXIT
```

- ⚠️ **每个实验前必须重新供种**（失败会污染 flat dist），脚本已内置
- ⚠️ 判读**必 grep `✗`** —— `xtask build` 失败仍打 `✔` 并 exit 0
- ⚠️ 供种别拷 `artifacts/build/runtime/`（CMakeCache 烤死绝对路径）；`artifacts/xtask` 必须拷
- 自建 VM：`cd src/runtime && cargo build --release --bin z42vm`
  → 产物在 `artifacts/build/runtime/release/z42vm`（cargo target dir 被重定向到那里）
