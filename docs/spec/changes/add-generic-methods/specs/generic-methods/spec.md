# Spec: 泛型方法端到端（方法级 type_args，M1）

## ADDED Requirements

### Requirement: 调用点显式方法类型实参

泛型方法调用可在方法名后写显式类型实参 `Foo<T1, T2>(args)`，编译器解析并绑定到目标泛型方法。

#### Scenario: 静态泛型方法显式调用
- **WHEN** 定义 `static Std.Type F<T>() { return typeof(T); }` 且调用 `F<int>()`
- **THEN** 解析成功，绑定到 `F`，方法 type_args = `[int]`

#### Scenario: 实例泛型方法显式调用
- **WHEN** 类 `C` 有 `Std.Type G<T>() { return typeof(T); }`，调用 `new C().G<string>()`
- **THEN** 解析成功，运行返回名为 `"string"` 的 `Std.Type`

#### Scenario: 多类型参数
- **WHEN** `static bool H<K, V>()` 调用 `H<int, string>()`
- **THEN** K=int、V=string 分别绑定

#### Scenario: `<` 歧义消解不误伤小于号
- **WHEN** 源码含 `a < b` 或 `x = a < b > c`（比较，非泛型调用）
- **THEN** 仍按比较运算解析，不误判为泛型调用（byte-identical 既有 golden 不漂移）

### Requirement: 方法体内方法级 `typeof(T)` 解析为具体类型

泛型方法体内 `typeof(T)`（T 为方法级类型参数）在运行期求值为**调用点实参的具体类型句柄**。

#### Scenario: typeof(T) = 具体用户类
- **WHEN** `static Std.Type F<T>() { return typeof(T); }`，调用 `F<Point>()`（`Point` 为用户类）
- **THEN** 返回句柄 `.Name == "Point"`，且 `.GetFields()` 可枚举 `Point` 字段（真句柄，非裸名 "T"）

#### Scenario: typeof(T) = 基元
- **WHEN** 调用 `F<int>()`
- **THEN** 返回 `.Name == "int"`

#### Scenario: 与直接 typeof 一致
- **WHEN** `F<Point>()` 的结果与直接 `typeof(Point)` 比较
- **THEN** `.FullName` 相等

### Requirement: 方法体内 `new T()` / `default(T)` 解析

方法级类型参数支持 `new T()`（调用具体类型无参构造）与 `default(T)`（具体类型零值）。

#### Scenario: new T() 构造具体实例
- **WHEN** `static object Make<T>() { return new T(); }`，`T` 有无参构造，调用 `Make<Point>()`
- **THEN** 返回一个 `Point` 实例（`.GetType().Name == "Point"`）

#### Scenario: default(T) 引用类型 → null
- **WHEN** `static object D<T>() { return default(T); }`，调用 `D<Point>()`
- **THEN** 返回 `null`

#### Scenario: default(T) 值类型 → 零值
- **WHEN** 调用 `D<int>()`（方法返回改为 `T`/ 泛型上下文）
- **THEN** 求值为 `0`（复用 `default_value_for` 语义）

### Requirement: 类型实参数量校验

调用点类型实参数量必须与方法声明的类型参数数一致。

#### Scenario: arity 不符报错
- **WHEN** `static void F<T>()` 被调用为 `F<int, string>()`
- **THEN** 编译期诊断错误（类型实参数量不匹配），不产出字节码

### Requirement: zbc / zpkg 格式演进（strict-pin）

承载方法 type_args 的 Call 编码 + 方法级形参解析指令导致格式版本递增。

#### Scenario: 版本 bump 后旧产物不可读
- **WHEN** zbc `Minor` / zpkg `Minor` 递增
- **THEN** 旧版本 zbc/zpkg 被 reader 拒绝（strict-pin，无兼容路径）；golden fixture 重生

#### Scenario: 两代自举吸收格式差
- **WHEN** ci-bootstrap 检测种子 z42c 格式 minor 与源码不等
- **THEN** 走两代自举（旧 VM 跑 gen1/gen2），格式差被吸收，build-and-test 绿

## MODIFIED Requirements

### Requirement: `typeof` 运行期求值

**Before:** `typeof(X)` 中 X 若为泛型类型参数（裸名 `"T"`），运行期 `make_constructed_type("T")` 产出名为 `"T"` 的占位 Type（不解析为具体类型）。

**After:** **方法级**类型参数的 `typeof(T)` 经新指令读 `frame.method_type_args[idx]` → 解析为**调用点实参的具体类型**。（类级 `typeof(T)` 的具体解析不在 M1 范围——M1 只补方法级；类级现状保持。）

## IR Mapping

| z42 语法 | IR 指令 | 运行期 |
|---------|---------|--------|
| `Foo<A,B>(args)` | `Call` / `VCall` 携带 `method_type_args: [A,B]`（解析后具体名）| 建帧填 `frame.method_type_args` |
| 方法体 `typeof(T)` | 方法级形参解析指令（读 frame 槽 idx → 具体 Type）| `make_type_from_name(frame.method_type_args[idx])` |
| 方法体 `new T()` | 同上物化 Type + activator 构造 | 复用 `__activator_create` |
| 方法体 `default(T)` | 方法级形参解析指令（读 frame 槽 idx → 零值）| 复用 `default_value_for` |

## Pipeline Steps

- [x] Lexer（无新 token；`<` 复用）
- [x] Parser / AST（调用点类型实参解析 + `CallExpr.TypeArgs`）
- [x] TypeChecker（绑定方法 type_args + arity/约束校验 + 方法级形参解析）
- [x] IR Codegen（Call 携带 type_args + 方法级形参解析指令 + zbc 编解码 + 版本 bump）
- [x] VM interp（Frame method_type_args 槽 + 建帧填充 + 新指令执行）
- [x] VM jit（镜像 interp）
