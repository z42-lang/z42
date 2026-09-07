# Tasks: `available!()` 符号可用性宏 + 加载期死分支剪枝

> 状态：🟡 IMPL（User 2026-09-08 确认 DRAFT + 全部裁决按推荐）| 创建：2026-09-08
> 分支/worktree：`add-symbol-availability-macro` / `../z42-avail`
> **纯 support 阶段**：z42c / stdlib / xtask 源码**一律不使用** `available!()` → 产出字节不变 →
> 单 PR 可落地，不跨两个 nightly（design D7）。
> 语义耦合知会：与 [`add-invariant-attribute`](../add-invariant-attribute/tasks.md) 同动 VM 函数准备路径。

## User 裁决（2026-09-08，全部按推荐）

| # | 问题 | 裁决 |
|---|---|---|
| A1 | 符号粒度 | **类型 + 唯一方法**；重载 → 报错要求消歧，消歧语法 Deferred |
| A2 | 加载期 force-load | **允许**：DEPS `namespaces` 先做否定判定 → 只定向加载认领该 ns 的那一个文件 + 环检测 |
| A3 | debug 折叠统计量 | **加**；测试必须同时断言「折叠发生了」与「行为正确」 |
| A4 | `CallerMacro.z42` 改名 | **改**为 `MacroRegistry.z42` |

## 进度概览
- [ ] P0 前置：worktree 供种 + baseline GREEN
- [ ] P1 编译器前端：parser `name!(args)` + 位置放宽 + 改名
- [ ] P2 编译器语义：`available!` 绑定、符号解析、dispatch key 生成、诊断
- [ ] P3 编译器发射：`BuiltinInstr("__sym_available", ConstStr(key))`
- [ ] P4 VM：`__sym_available` builtin + `fold_availability` 加载期 pass + 统计量
- [ ] P5 测试：skew 脚手架 + 用例（interp + jit）
- [ ] P6 文档 + GREEN + PR

---

## P0 前置
- [ ] 0.1 worktree 供种（nightly SDK == origin/main `54c8a1df`，格式/API 双零 skew）
- [ ] 0.2 baseline `xtask test` 全绿（**改动前**跑一次，区分 pre-existing 红）
  - ⚠️ GREEN 前 `rm -rf /tmp/z42c-e2e-*`（stale fixture 假失败）

## P1 编译器前端（syntax）
- [ ] 1.1 `ExprParser.z42`：`ident '!' '(' ...')'` 支持**带参**形态。
      现状仅三 token 前瞻 `Bang + LParen + RParen`（无参）。改为：见到 `Bang + LParen` →
      进宏路径，解析括号内表达式列表（v1 仅 1 个），产 `IdentExpr("$macro:"+name)` + 参数。
      **载体待定**：`IdentExpr` 无参数槽 → 需评估「复用 `CallExpr(callee=IdentExpr(哨兵), args)`」
      （零新 AST 节点，F2 安全）vs 新节点（会引入 syntax→semantics 新跨包符号，**踩 F2 冷启动**，
      design.md:743-760 记载过一次实现转向）。**倾向复用 `CallExpr`。**
- [ ] 1.2 保持零新 token（`Bang` / `LParen` / `RParen` 全是既有）
- [ ] 1.3 `CallerMacro.z42` → `MacroRegistry.z42`（A4）；更新 4 个引用点
      （`ClassDescBuilder` / `DeclBinder` / `OverloadBinder` / `ExprTyper`）

## P2 编译器语义
- [ ] 2.1 `MacroRegistry.KindOf`：白名单加 `available`
- [ ] 2.2 宏元数据分类：区分「参数默认值位宏」（caller 类，位置受限）与「表达式位宏」
      （`available`）。`ExprTyper._bindIdent` 的 E0450 拦截改为**按 kind 判定**，
      不再一刀切（现状：任何宏落到 `_bindIdent` = E0450）
- [ ] 2.3 `available!(X)` 的参数解析：X 是符号引用（`IdentExpr` / `MemberExpr` 链），
      解析到类型或唯一方法。**不存在 → E0401**（复用）
- [ ] 2.4 重载消歧：解析到多个方法 → 新诊断码 A「ambiguous」（A1）
- [ ] 2.5 参数形态非法（字面量 / 任意表达式）→ 新诊断码 B
- [ ] 2.6 dispatch key 生成（design D2）：
      - 方法 → `"m:" + <与运行期 dispatch 同源的 mangled 全签名键>`
      - 类型 → `"t:" + FQN`
      **关键**：键必须与 VM 实际 dispatch 用的键同源，否则「探测说有、调用却炸」
- [ ] 2.7 `DiagnosticCodes.z42`：新增码 A/B（按现状顺延号段）

## P3 编译器发射
- [ ] 3.1 emit：`Intern(key)` → `ConstStrInstr` → `BuiltinInstr(dst:Bool, "__sym_available", [key], 1)`
      （形状照抄 `TypeOpEmitter._emitBox` 的 `__box_prim`）
- [ ] 3.2 结果类型 `bool`，可直接进 `if` 条件 / 局部初始化

## P4 VM
- [ ] 4.1 `builtin_table.rs`：**表尾追加** `("__sym_available", …)`（下标即 BuiltinId，只可追加）
- [ ] 4.2 builtin 实现：按 key 判定可解析性。**正常路径永不执行**（已被 P4.3 折掉）→
      debug profile 下走到即告警/panic「fold pass did not run」（design Implementation Notes）
- [ ] 4.3 `fold_availability` 加载期 pass（新文件，`metadata/loader/` 下）：
  - [ ] 4.3a 判定：DEPS `namespaces` 否定判定 → 定向 force-load 单个 dep → 精确判定（A2/D4）
  - [ ] 4.3b 环检测：`available!` 目标间形成加载环 → 诊断（非静默降级）
  - [ ] 4.3c 折叠：`Builtin(__sym_available)` → `ConstBool`
  - [ ] 4.3d 剪枝：`BrCond`(常量) → `Br`；BFS 移不可达块。
        **照抄 `IrDeadBranch.z42` 算法形状，不跨语言复用代码**（D8）。
        约束：**有异常表的函数只折不删块**（CFG 不含异常隐式边）
- [ ] 4.4 fast path：decode 时若模块无 `__sym_available` → `Module` 置 flag，pass 直接 return
- [ ] 4.5 接入装配链 `artifact.rs`：必须在 `build_block_indices` **之前**（或之后重建派生侧表：
      `block_index` / `branch_targets` / `site_index`）
- [ ] 4.6 **不做**寄存器重编号（`max_reg` 只增不减；避免动 `reg_types` 与 JIT 2C 提升资格分析）
- [ ] 4.7 debug-only 统计量（A3）：折叠数 / 移除块数，经 `Z42_LOG` trace 或测试查询暴露

## P5 测试
- [ ] 5.1 **skew 脚手架**（今天零覆盖，可能是本 change 最大工作量）：
      同一依赖包两份源（`dep-v1/` 缺符号 / `dep-v2/` 有），各建 zpkg，用例按需切换 `libs/`。
      需 test-runner 支持「按用例切换依赖产物」——**IMPL 前实测评估，若过大则先出最小形态**
- [ ] 5.2 编译器单测：解析 / 位置放宽 / E0401 / 码 A / 码 B
- [ ] 5.3 Rust 单测：`fold_availability` pass（合成 Module，覆盖折叠 + 剪枝 + 异常表约束）
- [ ] 5.4 e2e 符号存在：v2 在场 → 走 if；断言 else 被剪（统计量 > 0）
- [ ] 5.5 e2e 符号缺失：v1 在场 → 走 else；断言 if 被剪
- [ ] 5.6 **interp / jit 一致**：`xtask test stdlib --mode jit`（本地 `xtask test` 只跑 interp）
- [ ] 5.7 负例：重载目标 / 非符号参数 / 宏在非法位置

## P6 收口
- [ ] 6.1 book：语言页（`available!` 语义、限制、与 `[Invariant]` 的能力差异）
- [ ] 6.2 book：runtime 页（加载期 pass 流程图 + 与 token 解析的顺序约束）
- [ ] 6.3 **修正 `.claude/skills/add-ir-op/SKILL.md`**（已严重过时：只列 3 步、指向已重构掉的
      路径、完全没提 version bump；本 change 因此差点误判成本）
- [ ] 6.4 `xtask test bootstrap`（改了 parser / 语法能力，必跑）
- [ ] 6.5 GREEN 全绿 + 归档（`changes/` → `archive/`，**随 PR 同提交**）
- [ ] 6.6 开 PR（body 三段 + 页脚）

---

## 风险登记
| 风险 | 缓解 |
|---|---|
| P1.1 载体选择踩 F2 冷启动 stale-cache | 优先复用 `CallExpr`，不新增 syntax AST 节点 |
| P4.3a force-load 引发加载环 / 启动放大 | ns 否定判定先行；只加载认领该 ns 的单个文件；环检测报错 |
| P4.5 剪枝破坏派生侧表 | 明确插桩点在 `build_block_indices` 前；否则重建 |
| P5.1 skew 脚手架工作量失控 | 先出最小形态（手工两份 zpkg + 用例级切换），不追求通用框架 |
| 剪枝不可观测 → 变成假保障 | A3 统计量 + 测试双断言（行为 + 折叠发生） |
