# Spec: 参数默认值统一常量表示 + caller 宏（PR6, D3 + 默认值行为修复）

> attribute-handler-registry 阶梯的 PR6。原计划仅「caller 编译期宏（D3）」；系统设计后**扩展**为
> 参数默认值机制的整体重设——因为发现现有 `kind` 枚举是**错的抽象层**（见下）。三件事一次做完：
> ① 统一常量表示（含 struct/enum/数组，替代标量-only 的 `kind`）；② **修复跨包默认值静默塌零 bug**；
> ③ caller-context 编译期宏（原 PR6）。
>
> **采用零格式-bump 的 attr-ref 哨兵持久化**（同 PR5 `$Deprecated`）：参数早有 per-param attr-ref blob
> 通道（zbc 1.15 `add-parameter-attribute-reflection`），默认值以 `$Default` / `$Caller:*` 哨兵骑上去，
> **不 bump zbc/zpkg 格式** → 绕开当前两代自举格式-bump 回归墙（[[two-gen-bootstrap-regressed-blocks-format-bumps]]）。

## 背景：现有机制为什么是错的抽象层

参数默认值当前有**两条互不相干的路径**：

- **同包**（[OverloadBinder._adaptArgs](../../../../../src/compiler/z42c.semantics/src/OverloadBinder.z42)）：调用点**重新绑定真正的
  `Param.Default` AST 表达式** → 任何表达式都行（struct/enum/拼接），但 `kind` 在此**完全没用到**。
- **跨包**（导入的 `Z42FuncType` **不携带任何默认值信息**）：省略实参 → `BoundDefault(T,-1)` →
  emit **`default(T)` 零值**，**而非作者声明的默认值** ⇒ **静默正确性 bug**（`f(int x=5)` 跨包调 `f()` → x=0）。
- 持久化的标量 `(kind,val,str)`（fold，kind 0-5）：**只被反射消费**，从不参与真实调用填充。

结论：`kind` 只是给反射用的标量-only 有损投影，不是调用行为的真相源；它表达不了 struct/enum/数组，
且跨包默认值根本没接。**本 PR 用统一的编码常量 `ConstBlob` 取代 `kind`，让两条路径从同一真相源物化。**

## ADDED Requirements

### Requirement: 常量值模型——参数默认值必须可编译期常量折叠

#### Scenario: 标量 / null / bool / char 默认值
- **WHEN** 参数默认值是 int（任意宽度）/ float / bool / char / string / null 字面量或其常量表达式（`-5`/`1+2`/`!false`）
- **THEN** 折叠为对应 `ConstBlob` 并作为该参数默认值持久化

#### Scenario: enum 成员默认值
- **WHEN** 参数默认值是 enum 成员（如 `Color c = Color.Red`）
- **THEN** 折叠为 `ConstBlob`（携带 enum 类型名 + 底层整数值），可同包 / 跨包忠实重建

#### Scenario: struct 常量默认值
- **WHEN** 参数默认值是 struct 的常量构造（如 `Point p = new Point(0, 0)`，所有实参可常量折叠）
- **THEN** 折叠为 `ConstBlob`（struct 类型名 + 逐字段 `ConstBlob`），同包 / 跨包都从它物化默认值

#### Scenario: 常量数组默认值
- **WHEN** 参数默认值是常量元素的数组字面量（如 `int[] xs = [1, 2, 3]`）
- **THEN** 折叠为 `ConstBlob`（元素类型 + 各元素 `ConstBlob`）

#### Scenario: 命名常量默认值
- **WHEN** 参数默认值引用一个编译期 `const`（如 `int n = MaxRetries`，`MaxRetries` 是 const）
- **THEN** 折叠为其常量值的 `ConstBlob`

#### Scenario: 非常量默认值 → 编译错误
- **WHEN** 参数默认值不可常量折叠（如引用运行期变量、调用非纯函数、`new T(random())`）
- **THEN** 报编译错误（新诊断码，`E04xx`），指明默认值必须是编译期常量或 caller 宏
- **AND** 这收窄了同包现行「重新 emit 任意表达式」的宽容度（常量值模型）；实测 stdlib/编译器**无**非常量同包默认值 → 零破坏

### Requirement: 统一持久化——`$Default` 哨兵（零格式-bump）

#### Scenario: 带默认值的可选参数持久化
- **WHEN** 一个参数有默认值（非 caller 宏）
- **THEN** 编译器在该参数的 attr-ref 列表追加哨兵 `IrAttrRef{ TypeName="$Default", FactoryFunc=<ConstBlob 编码串> }`
- **AND** SIGS 的 per-param `default_kind` 字节写 **0（vestigial，退休不再使用）**——物理删除该字节需格式 bump，留待墙修好后 follow-up
- **AND** 不 bump zbc/zpkg 格式（复用 zbc 1.15 param attr-ref blob 通道）

#### Scenario: 无默认值参数零开销
- **WHEN** 参数无默认值
- **THEN** 不追加哨兵；z42c/stdlib 多数参数如此 → 相应 attr-ref 列表不变

#### Scenario: 自举字节不动点
- **WHEN** z42c 编译 z42c 自身 / stdlib（源码可含标量默认值，但**不使用** caller 宏）
- **THEN** 当前 z42c 对同一源确定性产出 → **self-host gen1==gen2 逐字节一致（5/5）**

### Requirement: 调用点默认填充——两路统一，跨包修复

#### Scenario: 同包省略可选实参
- **WHEN** 同包调用省略一个带默认值的可选实参
- **THEN** 填入该参数默认值的 `ConstBlob` 物化结果（`BoundConst`）

#### Scenario: 跨包省略可选实参（修复静默塌零 bug）
- **WHEN** 跨 zpkg 调用省略一个带默认值的可选实参
- **THEN** 填入**作者声明的默认值**（从导入符号的 `$Default` ConstBlob 重建），**而非** `default(T)` 零值
- **AND** ImportedSymbolLoader 把 `$Default` 读回导入的 `Z42FuncType` / MethodSymbol；OverloadBinder 跨包分支用它替代 `BoundDefault(T,-1)`

#### Scenario: struct / enum / 数组默认值跨包
- **WHEN** 跨包省略一个 struct / enum / 数组默认值的可选实参
- **THEN** 从 `ConstBlob` 忠实重建该常量并填入（现有机制此处塌零 / 塌错，本 PR 修）

### Requirement: caller-context 编译期宏（D3）

#### Scenario: `caller_member!()` 作参数默认值
- **WHEN** 参数默认值是 `caller_member!()`（如 `void Log(string msg, string who = caller_member!())`）
- **THEN** 编译器持久化哨兵 `$Caller:member`（非 ConstBlob，值依赖调用点）
- **AND** 每个省略该实参的调用点，编译器注入**包含该调用点的成员名**字面量（同包 + 跨包一致）

#### Scenario: `caller_line!()` / `caller_file!()` / `module_path!()`
- **WHEN** 参数默认值分别是 `caller_line!()` / `caller_file!()` / `module_path!()`
- **THEN** 调用点分别注入：调用点**行号**（int）/ 调用点**源文件路径**（string）/ 调用点所在**命名空间**（string）
- **AND** 各以哨兵 `$Caller:line` / `$Caller:file` / `$Caller:module` 持久化

#### Scenario: caller 宏只能作参数默认值
- **WHEN** `caller_member!()` 等出现在参数默认值以外的位置（如函数体内 `let x = caller_member!()`）
- **THEN** 报编译错误（v1 限定作用域 = 参数默认值，对齐 C# `[CallerMemberName]`）

#### Scenario: 显式实参覆盖 caller 宏
- **WHEN** 调用点显式传入该实参（如 `Log("hi", "explicit")`）
- **THEN** 用显式实参，不注入 caller 上下文（对齐 C# 语义）

#### Scenario: z42c / stdlib 源不使用 caller 宏（support-先行）
- **WHEN** 本 PR 落地
- **THEN** z42c / stdlib / xtask 源码**不使用** `caller_*!()` 新语法（仅加 support）→ 上一 nightly 的 z42c
  能编当前源 → 单 PR、不触发两-nightly 纪律（同 PR3c `#suppress`）

### Requirement: 反射迁移到 ConstBlob

#### Scenario: `ParameterInfo.DefaultValue` 反射
- **WHEN** 反射读一个带默认值参数的 `DefaultValue`
- **THEN** 从 `$Default` ConstBlob 解码为对应 Value（比旧标量-only 投影更全：现可含 enum/struct/数组）
- **AND** Rust 反射改读 param attr-ref 的 `$Default` 哨兵解码，替代旧 `param_defaults` 标量元组
- **AND** caller 宏参数的 `DefaultValue` 反射为「无固定值」（值依赖调用点）→ 反射不暴露具体值（约定 Deferred 细化）

## IR Mapping
- **零格式-bump**：不新增任何 zbc/zpkg 段或 flag 位。默认值状态借哨兵 `IrAttrRef` 走既有 **param** attr-ref
  blob 通道（zbc 1.15，SIGS 每参 attr-ref 块）。
- **`ConstBlob` 编码**（放 `$Default` 哨兵的 FactoryFunc 串槽，自描述递归编码）：null / bool / int(宽度) /
  float(bits) / char / string / enum(类型名+底值) / struct(类型名+逐字段) / array(元素类型+元素序)。编码细节见 design.md。
- **caller 哨兵**：`$Caller:member` / `$Caller:line` / `$Caller:file` / `$Caller:module`（FactoryFunc 空；值调用点注入）。
- **SIGS `default_kind` 字节**：退休——一律写 0（物理删除需格式 bump，留 follow-up）。

## Pipeline Steps
- [ ] Lexer — 新增 `!` 后的宏调用识别路径（或复用既有 `Bang` + `(` 后缀；caller 宏名在语义层白名单认，词法零改动优先）
- [ ] Parser / AST — `caller_member!()` 等表达式宏节点（新 `MacroCallExpr` 或复用 CallExpr + 宏名判定）；限定只在参数默认值位合法
- [ ] TypeChecker / Fold — `_foldDefault` 扩到 struct/enum/数组/命名常量 → `ConstBlob`；非常量 → 新诊断；caller 宏 → 哨兵
- [ ] IR Codegen — 每参写 `$Default` / `$Caller:*` 哨兵到 param attr-ref；`default_kind` 写 0
- [ ] 持久化 — 复用既有 param attr-ref blob（零格式-bump），无 writer/reader 格式改动
- [ ] 跨包 — ImportedSymbolLoader 读 `$Default` / `$Caller:*` → Z42FuncType / MethodSymbol；OverloadBinder 跨包分支用它
- [ ] 调用点注入 — OverloadBinder 同包 + 跨包统一从 ConstBlob / caller 哨兵物化省略实参
- [ ] 反射 — Rust reflection.rs 改读 `$Default` ConstBlob，替代 `param_defaults` 标量元组
- [ ] CompilerFingerprint++（codegen 改变、无格式 bump）
- [ ] VM interp — 无执行语义变化（默认填充在编译期完成；caller 注入是编译期字面量）

## Deferred（本 PR 不做）
- **物理删除 SIGS `default_kind` 字节**（格式 bump）——待两代自举回归修复后 follow-up；本 PR 仅令其 vestigial 写 0
- caller 宏在参数默认值以外位置（函数体内「当前成员」语义）
- Rust `[track_caller]` 多层调用链传播（design D3 明列 Deferred）
- 命名参数默认值的 ConstBlob（若涉及）
- 反射对 caller 宏参数 DefaultValue 的精确约定（当前反射为「无固定值」）
