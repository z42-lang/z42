# Proposal: `available!()` —— 符号可用性宏 + 加载期死分支剪枝

> **状态：DRAFT，待 User 确认。** 本文与 [`add-invariant-attribute`](../add-invariant-attribute/proposal.md)
> 是两条独立线（一个「加载期符号存在性」、一个「运行期不变量」），可并行；与
> `fix-silent-symbol-resolution`（尚未起草）是**强前后序**关系，见「与 PR2 的耦合」。

## Why

**问题：依赖 zpkg 版本 skew 时，消费方无法安全地写降级分支。**

场景：包 A 编译时依赖包 B v2（有 `B.Foo()`），部署时 `libs/` 里放的是 B v1（没有 `Foo`）。
A 想写：

```z42
if (/* B.Foo 可用？ */) { B.Foo(); }
else                    { legacyPath(); }
```

今天做不到，两个原因：

1. **没有可靠的探测原语。** `Type.GetType("Missing.Type")` **不返回 null**——全 miss 时走
   [`type_object.rs:120-122`](../../../../src/runtime/src/corelib/reflection/type_object.rs)
   造一个 handle-less 的合成 `Std.Type` 返回。方法级更没有：`Std.Type` 只有 `GetMethods()`，
   **没有 `GetMethod(name)`**；VM 内部的 `try_lookup_function` **没有暴露成 builtin**。
2. **猜错的代价是静默错误答案，不是异常。** 缺失静态字段读到
   `Value::Null`（[`statics.rs:37,55`](../../../../src/runtime/src/vm_context/statics.rs)），
   `new` 缺失类型得到零字段零 vtable 空壳（[`exec_object.rs:60`](../../../../src/runtime/src/interp/exec_object.rs)），
   基类缺失时子类静默退化成「无基类」、丢掉全部继承字段和 vtable
   （[`type_registry.rs:208,223`](../../../../src/runtime/src/metadata/loader/type_registry.rs)
   注释直言 "degrades to own only"）。

第 2 条由 `fix-silent-symbol-resolution`（PR2）解决。**但 PR2 一旦让缺符号抛异常，
今天「侥幸能跑」的 guarded 代码会立刻炸**——那个永远走不到的 `B.Foo()` 也会在函数
准备期被校验到。所以必须先有一个机制，让被保护的分支**根本不参与符号解析**。

这就是本 change：`available!()` + **加载期死分支剪枝**。

## What Changes

- **新增表达式位编译期宏 `available!(<符号引用>)`**，求值为 `bool`。
  - **编译期**：目标符号**必须存在**（不存在 → E0401，防拼写错误与重构失配）。
    宏被 desugar 成 `Builtin("__sym_available", ConstStr("<dispatch-key>"))`。
  - **加载期**：VM 判定该 key 在当前加载图里是否可解析 → 折叠成 `ConstBool` → 跑一次
    死分支剪枝，**不可达块物理移除**。
  - **结果**：被剪掉的分支里的 call site **不进入 `resolve_function_tokens`**，interp 与
    JIT 都再也看不到它。
- **parser 扩展**：现有宏 v1 只支持无参形态 `name!()`（[`ExprParser.z42`](../../../../src/libraries/z42c.syntax/src/ExprParser.z42) PR6b 注）。
  本 change 扩为 `name!(args)`，仍**零新 token、零新 AST 节点**（继续译成
  `IdentExpr("$macro:"+name)` 哨兵 + 参数表）。
- **放宽宏的位置限制**：现在宏只合法于参数默认值位
  （[`ExprTyper.z42:82-89`](../../../../src/compiler/z42c.semantics/src/ExprTyper.z42) 硬拦 → E0450）。
  `available!()` 必须能出现在普通表达式位（`if` 条件、局部初始化）。
- **新增 builtin `__sym_available`**：追加进 `BUILTINS` 表（只能追加，下标即进程内稳定
  `BuiltinId`）。运行期语义仅作 fallback——正常路径在加载期就被折掉了，永不执行。
- **新增加载期 pass `fold_availability`**：在模块 decode + registry 构建之后、模块发布给
  VM 之前运行；折叠 + 剪枝。模块内无 `__sym_available` 时整段跳过（decode 时置一个 flag）。

**不需要**：新 IR opcode、zbc/zpkg 格式 bump、新 AST 节点、新 token。
复用 `BuiltinInstr` + 常量字符串参数编码语义，`__box_prim`
（[`TypeOpEmitter.z42`](../../../../src/compiler/z42c.semantics/src/TypeOpEmitter.z42)）是逐字先例。

## 与 PR2（`fix-silent-symbol-resolution`）的耦合

**顺序约束是硬的**：剪枝必须发生在 token 解析**之前**。

```
load module
  → build type registry
  → fold_availability：折叠 __sym_available + CFG 剪枝     ← 死分支的指令在这里消失
  → publish module
  → （首次进入某函数）resolve_function_tokens              ← PR2 在这里加急切校验
  → execute / JIT
```

- **PR1 单独落地的价值 = 优化 + API**（缺符号仍然静默，所以剪不剪都跑得起来）。
- **PR1 的正确性价值要等 PR2 才兑现**：PR2 让未被保护的缺符号抛异常，`available!()` 成为
  用户显式 opt-in 的唯一豁免通道。
- **PR2 不能先于 PR1 落地**——否则一切 guarded 降级代码全部炸。

## Scope（允许改动的文件）

### 编译器（z42c）
- `src/libraries/z42c.syntax/src/ExprParser.z42` — `name!(args)` 形态
- `src/compiler/z42c.semantics/src/CallerMacro.z42` — 宏白名单加 `available`；kind 与参数约定
  （文件名与「caller」绑定已不贴切，是否改名见 Open Question 4）
- `src/compiler/z42c.semantics/src/ExprTyper.z42` — 放宽位置拦截；`available!` 的绑定与
  符号解析（复用既有名字解析路径产出 dispatch key）
- 发射：新增或就近扩展 emitter，产 `BuiltinInstr("__sym_available", ConstStr(key))`
- `src/libraries/z42c.core/src/DiagnosticCodes.z42` — 新诊断码（见 design D5）

### 运行时（Rust VM）
- `src/runtime/src/corelib/mod.rs` — `BUILTINS` 追加 `__sym_available`
- `src/runtime/src/corelib/`（新文件或就近）— builtin 实现
- `src/runtime/src/metadata/loader/` — 新增 `fold_availability` pass + 接入 `artifact.rs` 装配链
- `src/runtime/src/metadata/bytecode.rs` — `Module` 上的 `has_availability_marker` flag

### 测试
- `src/tests/` — 新增 availability 用例目录
- **新建 skew 测试脚手架**（今天零覆盖）：同一依赖包的两个版本（v1 缺符号 / v2 有符号）

### 文档
- `docs/book/src/language/` — 宏与 availability 页
- `docs/book/src/runtime/` — 加载期 pass 说明（含流程图）

### 只读引用
- `src/runtime/src/metadata/resolver.rs`、`lazy_loader.rs`、`formats.rs`（DEPS section）

## Out of Scope

- **急切的全程序 link 校验**（`--verify-links`）——独立后续，见 design Deferred。
- **包级版本元数据**（`ZpkgDep` 加 version）——独立 change；本 change 不依赖它。
- **签名级消歧语法**（`available!(Foo.Bar(int, string))`）——v1 要求目标唯一，见 D3。
- **stdlib 自身使用 `available!()`**——本 change 只落 support，stdlib 使用需晚一个 nightly
  （[bootstrap-seed.md](../../../../.claude/rules/bootstrap-seed.md) 分阶段纪律）。
- 运行期不变量折叠（`[Invariant]`）——见 [`add-invariant-attribute`](../add-invariant-attribute/proposal.md)。

## Open Questions（需 User 裁决）

1. **符号引用的粒度**：v1 支持「类型 + 唯一方法」，还是只做「类型 + 命名空间」？
   方法级更有用但要处理重载消歧（见 D3）。
2. **加载期强制加载依赖**：判定「符号是否存在」可能需要 force-load 一个依赖 zpkg。
   接受这个启动期加载放大，还是要求 `available!` 的目标必须在已加载图内（更弱但零风险）？
   见 D4，我倾向前者 + 环检测。
3. **剪枝的可验证性**：加载期剪枝在内存里发生，zbc 字节不变，**没有现成的外部可观测面**。
   接受新增一个 debug-only 的 VM 统计量（`pruned_blocks`）专供测试断言吗？见 D6。
4. **`CallerMacro.z42` 是否改名为 `MacroRegistry.z42`**：文件将承载非 caller 类宏，
   名实不符。改名会动 4 个引用点（纯机械）。

## 需 User 确认的既有事实冲突

无。调查确认 [philosophy.md](../../../../.claude/rules/philosophy.md) 的「不为旧版本提供兼容」
五条适用范围**全是 z42 自身产物面**（zbc/zpkg 格式、stdlib API、IR 指令、编译器约定、CLI），
**不涵盖用户包之间的依赖**——包生态层的兼容性是设计空白，不是禁区。

但项目文化倾向明确（strict-pin、「缺则清晰报错」、
philosophy「❌ 解析失败时降级为 sentinel 值让下游猜」），故本设计遵循：
**默认行为是报错（PR2），`available!()` 是显式 opt-in 的例外。**
