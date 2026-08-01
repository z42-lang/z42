# Spec: OSR / 循环回边分层

## ADDED Requirements

### Requirement: 热循环就地升级为原生（OSR）

#### Scenario: 一次性调用、内部大循环 → OSR 升级
- **WHEN** `--mode jit` 下，某可翻译函数被调用（哪怕只一次），其内部循环在解释器里的
  **向后跳转次数达到 `osr_threshold`**
- **THEN** VM 就地编译该函数的 OSR 变体（入口在循环头），把当前活跃寄存器状态交给原生码，
  **从循环头继续执行**，函数余下部分走原生码
- **AND** 计数器 `jit_methods_compiled` 反映该次 OSR 编译

#### Scenario: 输出与纯解释器逐字节一致
- **WHEN** 一个触发 OSR 的程序分别以 `--mode interp` 和 `--mode jit` 运行
- **THEN** 两者标准输出逐字节相同（OSR 不改变可观察语义）

#### Scenario: 短循环不触发 OSR
- **WHEN** 某函数的循环向后跳转次数 **< `osr_threshold`**
- **THEN** 不编译该函数（除非它另经 call-count 分层达阈值），循环在解释器跑完，
  无 OSR 开销

#### Scenario: 不可翻译函数不 OSR
- **WHEN** 一个含 `LoadLocalAddr`（`ref`/`out` 参数）或其它 interp-only 指令的函数循环很热
- **THEN** 不触发 OSR（`resolve_osr_entry` 返回 None），继续解释执行——与现有
  `jit_unsupported_reason` 判定一致

#### Scenario: OSR 后函数返回值正确回到调用者
- **WHEN** OSR 变体在原生码跑完函数体、执行 `Ret`
- **THEN** 返回值以 `ExecOutcome::Returned` 交回解释器的调用点；抛异常则 `Thrown`；
  帧链（GC root / stack trace）正常 pop

### Requirement: `Z42_OSR_THRESHOLD` 配置

#### Scenario: 环境变量覆盖默认阈值
- **WHEN** 设置 `Z42_OSR_THRESHOLD=<N>`
- **THEN** 回边计数达 N 时触发 OSR；未设置用编译内默认值；值 clamp ≥ 1

## IR Mapping
无新增 IR 指令 / 无 zbc·zpkg 格式变更——纯 VM 执行策略（回边计数 + OSR 编译入口）。

## Pipeline Steps
受影响的 pipeline 阶段：
- [ ] Lexer — 无
- [ ] Parser / AST — 无
- [ ] TypeChecker — 无
- [ ] IR Codegen — 无
- [x] VM interp — 回边计数 + OSR 触发 / handoff
- [x] VM JIT — `translate_function` OSR 入口 + OSR-entry 编译/缓存
