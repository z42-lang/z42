# Spec: 跨包泛型约束持久化（PR-1）

> Capability：`generic-constraints`（延续 `complete-where-constraints` 的同名能力）
> 长期规范落点：`docs/book/src/language/generic-constraints.md`

## MODIFIED Requirements

### Requirement: 跨包泛型实例化的 where 约束校验

**Before:** 跨包泛型实例化的 where 约束 **100% 不校验**——`SymbolTable.ClassConstraints` 的唯一
写入点 `ConstraintChecker.Resolve` 只遍历本包 CU 的 `ClassDecl`，导入类型全程不碰，`Check` 第一行
`HasConstraints` 即返回。不只新补的几项，连 base-class / `class` / `struct` / 型参引用也不查。

**After:** 导入类型的 where 约束与本包类型**同口径校验**，七项全部生效。

#### Scenario: 跨包接口约束——满足
- **WHEN** 包 A 定义 `class Box<T> where T : IShow`，包 B `using A` 后写 `new Box<Widget>()`，且 `Widget` 实现 `IShow`
- **THEN** 编译通过，无诊断

#### Scenario: 跨包接口约束——违反
- **WHEN** 同上，但 `Widget` **未**实现 `IShow`
- **THEN** 报 `TypeMismatch`，Span 指向 `Widget` 这个类型实参本身（非整个 new 表达式）

#### Scenario: 跨包 base-class 约束
- **WHEN** 包 A 定义 `class Box<T> where T : Animal`，包 B 用 `Box<Rock>()` 且 `Rock` 不继承 `Animal`
- **THEN** 报 `TypeMismatch`（今天静默通过）

#### Scenario: 跨包 `class` / `struct` 约束
- **WHEN** 包 A 定义 `class Ref<T> where T : class`，包 B 用 `Ref<int>()`
- **THEN** 报 `TypeMismatch`（今天静默通过）

#### Scenario: 跨包 `new()` 约束
- **WHEN** 包 A 定义 `class Factory<T> where T : new()`，包 B 用 `Factory<NoCtor>()`，`NoCtor` 只有带参构造器
- **THEN** 报 `TypeMismatch`（今天静默通过）

#### Scenario: 跨包 `enum` 约束
- **WHEN** 包 A 定义 `class Flags<T> where T : enum`，包 B 用 `Flags<string>()`
- **THEN** 报 `TypeMismatch`（今天静默通过）

#### Scenario: 跨包型参引用约束
- **WHEN** 包 A 定义 `class Pair<T, U> where U : T`，包 B 用 `Pair<Animal, Rock>()`
- **THEN** 报 `TypeMismatch`（今天静默通过）

#### Scenario: 🔴 基元实参不得误报（自举链守护）
- **WHEN** 编译 stdlib，`Dictionary<TKey,TValue> where TKey : IEquatable<TKey>` 被实例化为 `Dictionary<int,int>`
- **THEN** 编译通过，无诊断（基元与其 wrapper 在 `Implements` 上归一一致）
- **注**：这是本 PR 最高风险场景，落地强度按 design D4 先 warning 探针

### Requirement: 约束键规则统一

**Before:** 三套键规则并存——`Classes` 条件 arity-mangle（`Name$N`）、`ClassConstraints` 恒裸名、
查询侧恒裸名。同名多 arity 泛型类的约束在 `ClassConstraints` 里 last-wins 互相覆盖。

**After:** 写入 / 查询 / 导入三处统一经 `SymbolTable.ConstraintKey(Z42ClassType)`，规则与 `Classes` 一致。

#### Scenario: 同名多 arity 泛型类的约束不串味
- **WHEN** 同一包内同时存在 `class Foo<T> where T : IShow` 与 `class Foo<T,U> where T : class`
- **THEN** `Foo<Widget>` 只校验 `IShow`、`Foo<Widget,int>` 只校验 `class`，互不覆盖（今天 last-wins 串味）

### Requirement: 接口声明上的 `where` 子句进入约束模型

**Before:** `ConstraintChecker.Resolve` 只处理 `Kind == "class" || "struct"`，接口的 `where`
子句根本不进约束模型；`ClassDescBuilder._interfaceDesc` 只建全空 bundle。

**After:** 接口的 `where` 子句与类同样被 resolve、持久化、校验。

#### Scenario: 接口约束被校验
- **WHEN** 定义 `interface IBox<T> where T : IShow`，并声明 `class C : IBox<Widget>` 而 `Widget` 未实现 `IShow`
- **THEN** 报 `TypeMismatch`（今天静默通过）

## ADDED Requirements

### Requirement: 约束在 zbc TYPE 段完整往返

#### Scenario: 七类约束写入并读回
- **WHEN** 一个泛型类带 base-class / `class` / `struct` / 型参引用 / `new()` / `enum` / 接口 七类约束，编译成 zpkg 后被另一包导入
- **THEN** `ZbcWriter` 置位 bit0/1/2/3/4/5 + 接口名列表；`ZbcReader` 读回后 `IrConstraintDesc` 的对应承载位与源一致

#### Scenario: 格式不 bump
- **WHEN** 本 PR 落地后重新生成格式 fixture
- **THEN** zbc minor 仍为 38、zpkg minor 仍为 43，`cargo test --test format_fixture_versions` 通过
- **注**：bit0-6 早已规约、三方 reader 已按完整布局消费，置位不改 layout

#### Scenario: 运行期死分支被接活
- **WHEN** 运行期 `validate_type_arg_constraint` 处理一个带 `class` 约束的导入泛型类型
- **THEN** 走到 `requires_class` 分支并做出判定（今天该分支因 writer 从不置 bit0 而是死代码）

## IR Mapping

不新增 IR 指令。约束数据落在 zbc `TYPE` 段的**型参约束 bundle**：

```
tp_count u8
  per 型参:
    name_idx u32
    cflags   u8      ← bit0 class / bit1 struct / bit2 base / bit3 tpRef
                       / bit4 new() / bit5 enum / bit6 funcSig
    [bit2] base_idx  u32
    [bit3] tpRef_idx u32
    iface_count u8
    iface_idx   u32 × n
    [bit6] funcSig 块
```

**本 PR 改的是写端置位与读端承载，不改上述 layout。**

## Pipeline Steps

- [ ] Lexer —— 不涉及
- [ ] Parser / AST —— 不涉及
- [x] TypeChecker —— `ConstraintChecker` 键规则 + 导入约束接入 + 接口 where
- [x] IR Codegen —— `ClassDescBuilder` special 约束不丢弃、base/iface 分开；`ZbcWriter` 置全位
- [x] 元数据往返 —— `ZbcReader` bit2 承载、`TsigReconcile` 搬运、`ExportedClassZ` 字段、`ImportedSymbolLoader` seed
- [x] VM interp —— 无代码改动，但运行期五个死分支因写端置位而**首次被执行**（需 JIT 模式补测）
