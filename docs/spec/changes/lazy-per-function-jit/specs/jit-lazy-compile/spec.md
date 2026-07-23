# Spec: JIT 惰性逐函数编译

## ADDED Requirements

### Requirement: 首次调用时编译（compile-on-first-call）

#### Scenario: 入口函数在 run 时编译并执行
- **WHEN** 以 `--mode jit` 运行一个程序
- **THEN** 入口函数被 cranelift 编译成原生码并执行，结果与 `--mode interp` 逐字节一致

#### Scenario: 被调用的函数首次调用时才编译
- **WHEN** 入口函数首次调用一个尚未编译的、合并模块内可 JIT 翻译的函数 F
- **THEN** F 在该调用点被编译并缓存，随后以原生码执行；再次调用 F 时直接命中缓存、不重新编译

#### Scenario: 未被调用的函数永不编译
- **WHEN** 程序运行结束，模块内存在从未被任何执行路径调用的可翻译函数 G
- **THEN** G 从未被 cranelift 编译（`jit_methods_compiled` 计数不含 G）

### Requirement: 语义与覆盖不变

#### Scenario: 全部现有 golden 在 jit 模式下输出不变
- **WHEN** `xtask test e2e --mode jit` 跑全部 golden 用例
- **THEN** 每个用例的 stdout 与改动前一致（与 interp 参考输出逐字节相同）

#### Scenario: interp-only 指令的函数仍走解释器
- **WHEN** 调用一个含 JIT 无法翻译指令（`jit_unsupported_reason` 非 None）的函数 H
- **THEN** H 不被编译，经 `cross_zpkg_via_interp` 在解释器上执行（与改动前行为一致）

#### Scenario: 真正跨包未加载的目标仍走 lazy-loader interp
- **WHEN** 调用一个不在合并模块、仅经 lazy loader 可达的跨包函数
- **THEN** 经 `try_lookup_function` 加载并在解释器执行（与改动前一致）

### Requirement: 虚调用（vcall）同款惰性

#### Scenario: 虚方法首次分派时编译目标
- **WHEN** `jit_vcall` 解析出的目标函数尚未编译且可翻译
- **THEN** 就地编译该目标、缓存进 `fn_entries_by_id`，随后以原生码执行

### Requirement: 线程安全

#### Scenario: 多线程首次调用同一未编译函数
- **WHEN** 两个 z42 线程（`spawn`）几乎同时首次调用同一个未编译函数 F
- **THEN** F 只被编译一次（编译经 Mutex 串行化），两线程都得到有效的 `FnEntry` 并正确执行；无数据竞争 / 无重复 define

#### Scenario: 已编译函数的热路径调用无需加锁
- **WHEN** 调用一个已编译并缓存的函数
- **THEN** 从 append-only 稳定槽读取 `FnEntry`，不获取编译锁（热路径零锁）

## MODIFIED Requirements

### Requirement: JIT 编译计数器语义

**Before:** `jit_methods_compiled` 在每次 `compile_module` 递增为「模块函数总数」；
`JitModuleCompiled` 事件在模块编译时一次性携带 `function_count = 模块函数总数`。

**After:** `jit_methods_compiled` 递增为「**实际被编译**的函数数」（= 被调用到的可翻译函数）；
计数在每次 `compile_one` 成功后 +1。`JitModuleCompiled` 事件的语义与触发时机在 design.md
Decision 3 中确定（保留每模块一次的 setup 事件，或改为聚合）。

## Pipeline Steps

受影响的 pipeline 阶段：
- [ ] Lexer — 无
- [ ] Parser / AST — 无
- [ ] TypeChecker — 无
- [ ] IR Codegen — 无
- [x] VM interp — 无行为变更；作为未编译/不可翻译函数的 fallback 执行体（已存在）
- [x] VM JIT — 编译时机由 eager 全量改为 lazy 逐函数（核心变更）

## IR Mapping

无新 IR 指令 / 无 zbc 格式变更。`Call` / vcall 指令的 codegen 不变（仍经 `hr_call` /
`hr_vcall` 运行时间接派发）；变更纯在 runtime JIT 后端的「何时编译」。
