# Design: `available!()` —— 符号可用性宏 + 加载期死分支剪枝

> DRAFT，待 User 确认。Proposal 见 [proposal.md](proposal.md)。

## Architecture

三段式，**编译期解析 → 加载期折叠剪枝 → 执行期零参与**：

```
┌─ 编译期（z42c） ───────────────────────────────────────────────┐
│ available!(B.Foo)                                              │
│   ├─ parser：ident '!' '(' expr ')' → IdentExpr("$macro:available")│
│   │                                   + 参数表                  │
│   ├─ ExprTyper：解析 B.Foo 到具体符号（不存在 → E0401）         │
│   └─ emit：BuiltinInstr(dst, "__sym_available", ConstStr(key))  │
│           key = 与 dispatch 同源的稳定键                        │
└────────────────────────────────────────────────────────────────┘
                              ↓ zbc / zpkg（零格式变更）
┌─ 加载期（VM） ─────────────────────────────────────────────────┐
│ decode → build_type_registry                                   │
│   → fold_availability                        ★ 本 change 新增   │
│       ├─ 扫 __sym_available 站点，判定 key 是否可解析            │
│       ├─ Builtin → ConstBool                                    │
│       ├─ BrCond(常量) → Br                                      │
│       └─ BFS 移除不可达块（复用 IrDeadBranch 的算法形状）        │
│   → publish module                                              │
└────────────────────────────────────────────────────────────────┘
                              ↓
┌─ 执行期 ────────────────────────────────────────────────────────┐
│ resolve_function_tokens（首次进入该函数）                        │
│   └─ 已剪掉的分支不在 blocks 里 → 其符号永不被解析               │
│      → PR2 的急切校验不会对它抛异常                              │
│ interp / JIT 都看不到死分支                                     │
└────────────────────────────────────────────────────────────────┘
```

**关键性质：`available!()` 的值在加载期完全可知**——只需查符号表，不执行任何用户代码、
不依赖 `__static_init__`。因此可以做**真正的原地 CFG 剪枝**，而不是像
[`add-invariant-attribute`](../add-invariant-attribute/design.md) 那样只能 first-call
惰性常量化。这是两条线最本质的能力差异。

## Decisions

### Decision 1: 复用 `BuiltinInstr` + 常量字符串，不加 IR opcode

新增 IR opcode 需改 ~20 文件、**必然 bump zbc + zpkg**（strict-pin，minor 不等直接 bail），
外加 zbc 6 个 + zpkg 4 个 fixture regen（后者无一键 regen）、golden hex 单测重截、触发
ci-bootstrap 两代自举。

`BuiltinInstr` 的 `BuiltinId` **不进 wire**——解析在模块加载期按名完成，
`BUILTINS` 表下标是**进程内**稳定 id。故新增 builtin **零 wire 变更、零格式 bump**。

「用常量字符串参数编码语义」在本仓是已验证的一等做法：`__box_prim`
（[`TypeOpEmitter.z42`](../../../../src/compiler/z42c.semantics/src/TypeOpEmitter.z42)）
的 arg1 就是一个常量 FQ 类名串，Rust 侧按串查表分派、未知则 `bail!`。

> ⚠️ `.claude/skills/add-ir-op/SKILL.md` 已严重过时（只列 3 步、指向已重构掉的路径、
> 完全没提 version bump）。**不要照着它做**；本 change 顺带在 tasks 里记一条修正它。

### Decision 2: 键的形态 —— 与 dispatch 同源

`ConstStr` 里存什么，决定加载期能否精确判定。

- **方法**：用与运行期 dispatch **完全同源**的 mangled 全签名键（zbc 1.27 / zpkg 0.32
  `stabilize-dispatch-keys 方案A` 之后是全签名 mangled）。这样加载期的判定与实际调用能否
  成功**是同一个问题**，不会出现「探测说有、调用却炸」。
- **类型**：FQN。
- 前缀区分 kind：`"m:"` / `"t:"`（未来 `"f:"` 静态字段）。

**反例（不采用）**：用短名或 `Class.Method` 无签名形式——那正是
[common-pitfalls.md §1](../../../../.claude/rules/common-pitfalls.md) 记载的
「同短名跨 ns 串味」和「重载塌缩」两个历史 bug 的形状。

### Decision 3: v1 的符号粒度 —— 类型 + **唯一**方法（⚠️ Open Question 1）

方法 dispatch 键带全签名，而 `available!(B.Foo)` 没有实参可供重载决议。

**v1 方案**：编译期解析 `B.Foo`，若解析到**唯一**方法 → 用其全签名键；若重载 →
新诊断码报错「ambiguous，请消歧」，**消歧语法本身 Deferred**。

理由：绝大多数版本 skew 场景是「整个 API 新增了」，不是「某个重载新增了」。为不确定的
需求先做签名语法不划算。

> **待 User 裁决**：接受 v1 只支持唯一方法，还是干脆 v1 只做类型级（更保守、但方法级
> 才是真需求）？我倾向「类型 + 唯一方法」。

### Decision 4: 加载期如何判定「不存在」（⚠️ Open Question 2）

难点：`UNRESOLVED` 在现有设计里**同时编码「跨包待解析」和「根本不存在」**
（[`resolver.rs:128`](../../../../src/runtime/src/metadata/resolver.rs) 注释写死了这一点）。
加载期直接查表，可能因为依赖还没加载而误判为「不存在」。

**方案（推荐）：DEPS section 的 `namespaces` 做否定判定 + 定向 force-load。**

`ZpkgDep { file, namespaces }`（[`formats.rs`](../../../../src/runtime/src/metadata/formats.rs)）
记录了每个依赖提供哪些 namespace：

1. 目标 key 的 namespace **不被任何已声明依赖认领**，且不在当前模块 →
   **确定不存在**，零加载，直接折 `false`。
2. namespace 被某个 dep 认领 → **只 force-load 那一个文件**再精确判定。
3. 环检测：`available!` 目标之间形成加载环 → 报诊断（而非静默降级）。这类环是病态的。

加载放大因此**有界**（最多 = `available!` 触及的不同 dep 文件数），不会退化成「加载全世界」。

**备选（更保守）**：要求 `available!` 的目标必须已在加载图内，否则报错。零加载风险，
但把「按需探测可选依赖」这个主用例挡在门外。**不推荐。**

### Decision 5: 诊断码

| 码 | 场景 |
|---|---|
| E0401（复用） | `available!(X)` 的 X 在**编译期**不存在 |
| 新码 A | `available!` 目标是重载方法，无法唯一确定（D3） |
| 新码 B | `available!` 参数形态非法（不是符号引用，如字面量、表达式） |
| E0450（复用） | 未知宏名 / 宏出现在非法位置 |

具体码号在 IMPL 阶段按 `DiagnosticCodes.z42` 现状顺延。

### Decision 6: 剪枝的可观测性（⚠️ Open Question 3）

剪枝在内存里发生，**zbc 字节不变**——没有现成的外部可观测面来断言「分支真的被剪掉了」。
不解决这点，这个 change 就是一个「从不打印、从不失败的门」，正是
`audit-silent-gates-program` 记录的反面教材。

**方案**：VM 加一个 debug-only 统计量（`fold_availability` 折叠数 / 移除块数），经既有
`Z42_LOG` trace 或一个测试专用查询暴露。测试断言 `pruned_blocks > 0`，**同时**断言行为
正确（死分支不执行）。两者缺一不可——只断言行为的话，「根本没折但分支恰好没走到」也会绿。

> 这是 memory 里反复吃过亏的形状：**「零违反」与「通道根本没通」必须能区分开。**

### Decision 7: 两阶段自举纪律的适用范围

本 change 引入**新语法能力**（`name!(args)` 形态 + 宏可用于普通表达式位）。按
[bootstrap-seed.md](../../../../.claude/rules/bootstrap-seed.md) 的「support 先行、
晚一个 nightly 再 use」：

- **本 change = 纯 support 阶段**：z42c / stdlib / xtask 的**源码一律不使用** `available!()`。
- support-only 不改变既有源码的产出字节 → gen1==gen2 字节不动点保持 → **单 PR 可落地，
  不需要跨两个 nightly**。
- stdlib 若将来要用 `available!()`，那是**独立的 use 阶段 PR**，等本 change 进 nightly 之后。

`xtask test bootstrap` 必跑（改了 parser / 语法能力）。

### Decision 8: 剪枝算法复用编译器侧的形状，但**不共用代码**

编译器侧已有 [`IrDeadBranch.z42`](../../../../src/compiler/z42c.semantics/src/IrDeadBranch.z42)
（`br.cond`(常量) → `br`；`ExcCount==0` 时 BFS 移不可达块）。VM 侧是 Rust、数据结构不同，
**照抄算法形状、不做跨语言复用**。

两个约束照搬：
- **有异常表的函数只折不删块**（CFG 不含异常隐式边，删块会破坏 handler 可达性）。
- 折叠只针对确定为常量的 cond。

## Implementation Notes

- `fold_availability` 的 fast path：decode 时若整个模块没出现过 `__sym_available`，在
  `Module` 上置 flag，pass 直接 return。绝大多数模块走这条。
- 剪枝会改变 `block_index` / `branch_targets` / `site_index` 等派生侧表 → 必须在
  `build_block_indices` **之前**跑，或跑完后重建。装配链顺序见
  [`artifact.rs`](../../../../src/runtime/src/metadata/loader/artifact.rs)。
- 剪枝后寄存器编号有空洞是可接受的（`max_reg` 只增不减，不影响正确性）；**不做**寄存器
  重编号，避免动 `reg_types` 与 JIT 的 2C 提升资格分析。
- `__sym_available` 的运行期实现只是 fallback（正常路径已被折掉）。它**必须存在且正确**，
  否则「pass 没跑」会静默变成「运行期查一次」——那又是一个假保障。考虑让 fallback 路径在
  debug profile 下直接 panic/告警（"availability fold pass did not run"）。

## Testing Strategy

**必须新建 skew 测试脚手架——今天零覆盖**（`src/tests/cross-zpkg/` 下 16 个用例全是
「依赖版本正确」的正向用例；`grep skew|missing method|version mismatch` 全仓零命中）。

| 层 | 用例 |
|---|---|
| 编译器单测 | `available!` 的解析、位置放宽、E0401/新码 A/新码 B |
| 加载期单测（Rust） | 折叠 + 剪枝的 pass 单测（构造带 `__sym_available` 的合成 Module） |
| e2e：符号存在 | v2 依赖在场 → 走 if 分支；断言 `pruned_blocks > 0`（else 被剪） |
| e2e：符号缺失 | **v1 依赖在场** → 走 else 分支；断言 if 分支被剪 |
| e2e：interp / JIT 一致 | 同一用例两种 mode 结果一致（本地 `xtask test` 只跑 interp，**必须补 `--mode jit`**） |
| 负例 | 重载目标 → 报错；非符号参数 → 报错；宏在非法位置 → E0450 |
| 自举 | `xtask test bootstrap` |

**skew 脚手架形态（待定）**：同一个小依赖包的两份源（`dep-v1/` 缺符号、`dep-v2/` 有），
各建成 zpkg，e2e 用例按需把其中一份放进 `libs/`。需要 test-runner 支持「按用例切换依赖
产物」——这可能是本 change 里最大的一块基建工作量，IMPL 前需实测评估。

## Deferred / Future Work

- **`avail-future-signature-disambiguation`**：`available!(Foo.Bar(int, string))` 重载消歧语法。
- **`avail-future-verify-links`**：opt-in 的全程序 link 校验（CI / 部署前体检）。与 PR2 的
  per-function 急切校验互补——后者只覆盖被调用到的函数，不是 link-time verification。
- **`avail-future-field-availability`**：静态字段级 `available!`（`"f:"` 前缀已预留）。
- **`avail-future-macro-registry-rename`**：`CallerMacro.z42` → `MacroRegistry.z42`
  （Open Question 4）。
- **`avail-future-zpkg-dep-version`**：`ZpkgDep` 加 version 字段 + META `version` 被消费，
  让 skew 本身可被发现（今天 `ZpkgDep` 只有 `{file, namespaces}`，META 的 `version` 读进
  结构体后全 runtime 无一处消费）。**独立 change，本 change 不依赖它。**
