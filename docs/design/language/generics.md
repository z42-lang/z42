# z42 泛型设计

> **Status**: L3-G1/G2/G2.5/G3a/G3d/G4 ✅ ｜ 泛型函数 + 泛型类 + 约束体系 + 跨 zpkg 元数据传播；关联类型 / 协变逆变 / 反射见 Deferred
>
> **约束体系的真实校验范围**（2026-09-06 更新）：同包七项已校验，**跨包七项亦已校验**
> （add-associated-types PR-1 接通 zbc 约束 bundle 全链路）；`Self`（仅接口）与关联类型
> **同包已实现**，关联类型**跨包尚未校验**、嵌套约束未实现。以
> [book/src/language/generic-constraints.md](../../book/src/language/generic-constraints.md) 为准。

> **方法级类型参数（2026-08-21 add-generic-methods M1）**：`Foo<T>()` 直接调用 + 方法体 `typeof(T)`/`new T()`/`default(T)` 具化为调用点类型——载体是 `Frame.method_type_args`（与类级实例 `type_args` 对称）。实现原理、决策、`<` 歧义消解见 **[book/src/language/generic-methods.md](../../book/src/language/generic-methods.md)**（SoT）。

> L3 核心特性。本文档定义泛型的语法、约束体系、编译策略和 VM 运行时支持。

---

## 设计目标

1. **Rust 级约束表达力** — trait bounds（`+` 组合）、关联类型（`Output=T`）、自引用约束
2. **C# 级代码共享** — 一份字节码服务所有实例化，零代码膨胀
3. **完整反射支持** — `typeof(T)`、`is`/`as` 运行时可用，TypeDesc 携带 type_args
4. **大型工程友好** — 编译时间和代码体积不随泛型实例化数量爆炸

---

## 方案对比与选型

### 三种主流策略

| 维度 | 单态化（Rust/C++） | 类型擦除（Java） | 代码共享 + 具化（C#） |
|------|-------------------|-----------------|---------------------|
| **代码膨胀** | 严重（N 类型 × M 泛型） | 无 | 引用类型无，值类型有 |
| **运行时性能** | 最优（零开销） | 装箱/拆箱 | 接近单态化 |
| **反射** | 不支持 | 部分丢失 | 完整支持 |
| **编译速度** | 慢 | 快 | 中等 |
| **约束表达力** | trait bounds（极强） | `<? extends T>`（弱） | `where T : I`（中等） |

### z42 选型：代码共享 + Rust 约束

| 决策 | 选择 | 理由 |
|------|------|------|
| **字节码策略** | C# 代码共享 | Value 枚举天然统一所有类型，一份代码 |
| **运行时类型** | 具化（Reified） | TypeDesc 携带 type_args，支持反射 |
| **约束语法** | Rust trait bounds | `where T: A + B`，比 C# 更灵活 |
| **关联类型** | Rust 风格 | `where T: Add<Output=T>`，C# 不支持 |
| **值类型特化** | 不做 | z42 Value 统一为 I64/F64，特化收益极小 |

### 为什么不选纯 Rust 单态化

z42 的 `Value` 枚举（`I64 | F64 | Bool | Str | Object | ...`）在 VM 层面已经是统一表示。即使单态化生成了 `List_int` 和 `List_string` 两份代码，VM 执行时仍然通过 Value tag 做 dispatch——**单态化不消除 Value dispatch 开销，但引入代码膨胀**。

### 为什么不选 Java 类型擦除

Java 的擦除导致运行时类型信息完全丢失（`List<int>` 和 `List<string>` 不可区分），无法支持 `typeof(T)`、`is T`、`as T` 等操作。z42 选择 C# 的具化模型保留完整类型信息。

---

## 语法设计

### 泛型函数

```z42
T Identity<T>(T x) {
    return x;
}

T Max<T>(T a, T b) where T: IComparable<T> {
    return a.CompareTo(b) > 0 ? a : b;
}
```

### 泛型类

```z42
class Stack<T> {
    T[] items;
    int count;

    Stack(int capacity) {
        this.items = new T[capacity];
        this.count = 0;
    }

    void Push(T item) {
        items[count] = item;
        count = count + 1;
    }

    T Pop() {
        count = count - 1;
        return items[count];
    }
}
```

### 泛型接口

```z42
interface IComparable<T> {
    int CompareTo(T other);
}

interface IEnumerable<T> {
    IEnumerator<T> GetEnumerator();
}
```

---

## 约束体系

> **约束的语义与校验范围的 SoT 已上浮到
> [book/src/language/generic-constraints.md](../../book/src/language/generic-constraints.md)**
> （change `complete-where-constraints`，2026-09-05）。本节只保留**选型意图**；
> 「哪些真的会被校验、边界在哪」一律以 book 页为准。

### 基础约束（与 C# 对齐）

| 约束 | 语法 | 含义 | 编译期校验 |
|------|------|------|-----------|
| 接口约束 | `where T: ISomething` | T 必须实现该接口（含接口继承链） | ✅ |
| 基类约束 | `where T: BaseClass` | T 必须继承自该类 | ✅ |
| 引用类型 | `where T: class` | T 必须是引用类型 | ✅ |
| 值类型 | `where T: struct` | T 必须是值类型 | ✅ |
| 构造器 | `where T: new()` | T 必须可零实参构造（**无显式 ctor 也算**） | ✅ |
| enum | `where T: enum` | T 必须是 enum 类型 | ✅ |

> ⚠️ **上表的 ✅ 只覆盖同包**。跨包泛型实例化的约束**一条都不校验**（连基类 / `class` /
> `struct` 也不），详见 book 页「已知限制」。

### Rust 风格增强

| 约束 | 语法 | C# 能否做到 | **z42 实现状态** |
|------|------|------------|-----------------|
| **多约束组合** | `where T: ISerializable + ICloneable` | C# 用 `,` 分隔，z42 用 `+`（Rust 风格） | ✅ 已实现 |
| **自引用约束** | `where T: IComparable<T>` | ✅ C# 支持 | ⚠️ 可写，但只比**裸名**、不校验类型实参。**已有更好写法**：接口内用 `Self`（add-associated-types PR-2），约束侧就不必写类型实参 |
| **交叉类型参数** | `where K: IHash, V: ICloneable` | ✅ C# 支持 | ✅ 已实现（每型参一条 `where`） |
| **关联类型** | `where T: IAdd<Output=T>` | ❌ C# 不支持，Rust 支持 | ⚠️ **同包已实现**（add-associated-types PR-3：`type Item;` + `IFoo<Item = T>`）；**跨包不校验**（需 bit7 + 双格式 bump，Deferred `assoc-type-crosspkg`） |
| **嵌套约束** | `where T: IIterator<Item=U>, U: IDisplay` | ❌ C# 不支持，Rust 支持 | ❌ **未实现** |

> 沿革：本节此前把关联类型 / 嵌套约束按**已实现**描述（含语法示例），实为**设计意图**；
> 2026-09-05 按实况订正为「未实现」。2026-09-06 `add-associated-types` PR-3 把**同包**关联类型
> 真正落地，本表随之再更新一次。语义与边界的 SoT 是
> [book/src/language/generic-constraints.md](../../book/src/language/generic-constraints.md)，
> 本文件只留选型与对比。

### 关联类型

```z42
interface IAdd<Rhs> {
    type Output;
    Output Add(Rhs other);
}

// 使用
T Sum<T>(List<T> items) where T: IAdd<T, Output=T> {
    var acc = items[0];
    for (int i = 1; i < items.Count; i++) {
        acc = acc.Add(items[i]);
    }
    return acc;
}
```

### 委托/函数约束（add-generic-func-constraint, 2026-05-11）

`where T: <function-type>` 允许把"T 是可调用的"作为约束传给 body —— body 内可直接 `t(args)` 调用，dispatch 走既有 CallIndirect（与闭包同一 IR 路径，**零新指令**）。

**两种等价语法形态：**

```z42
// 命名形式（沿用 stdlib delegate 名）
R Apply<T, R>(T f, int x) where T: Func<int, R> { return f(x); }
void Run<T>(T h, int x)  where T: Action<int>   { h(x); }
bool Test<T>(T p, int x) where T: Predicate<int> { return p(x); }

// 字面量形式（结构性、不依赖 stdlib）
R Apply<T, R>(T f, int x) where T: (int) -> R   { return f(x); }
void RunVoid<T>(T h)      where T: () -> void   { h(); }
```

两种形态在 TypeChecker 内都解析为 `Z42FuncType`，存入 `GenericConstraintBundle.FuncSignature` 字段。

**匹配语义 —— 结构性 + variance**（复用 `Z42FuncType.AssignableTo`）：

- 参数**逆变**：约束 `Func<Cat, int>` 接受 `Func<Animal, int>`（更宽参数 OK）
- 返回**协变**：约束 `Func<int, Animal>` 接受 `Func<int, Cat>`（更窄返回 OK）

**v1 限制（E0423 InvalidFuncConstraint）：**

- func 约束不能与其他约束（class / interface / `new()` / `struct` / `enum` 等）组合
- 同一 type 参数不能有多个 func 约束
- 反例：`where T: Func<int,int> + IDisposable` → E0423

**调用站点约束检查（E0422 GenericFuncConstraintViolation）：**

```z42
Apply<Func<string,int>, int>(s => int.Parse(s), 5)  // E0422: param 'string' 不能逆变匹配 'int'
Apply<Func<int,int>, int>(n => n * 2, 5)            // ok
```

**zbc 编码**：constraint bundle flag bit `0x40` + `param_count:u8 + per-param strIdx + return strIdx`（zbc 1.4+）。VM `verify_constraints` 校验签名内引用的 class/interface 类型存在（type-params + Std.* + primitives 放行）。

**实施记录**：`docs/spec/archive/2026-05-11-add-generic-func-constraint/`。

### 约束检查时机

| 阶段 | 职责 |
|------|------|
| **TypeChecker（编译时）** | 验证所有约束满足；类型参数传播；关联类型解析 |
| **IrGen（编译时）** | 生成共享字节码；type_params 元数据写入 IrFunction |
| **VM（运行时）** | 不检查约束（编译时已保证）；仅通过 TypeDesc.type_args 支持反射 |

---

## 编译策略

### TypeChecker 扩展

```
// 泛型函数调用 Max<int>(a, b)
1. 解析 T=int
2. 检查 int 是否满足 where T: IComparable<T>
3. 将函数体内所有 T 替换为 int 进行类型检查
4. 报告类型检查结果（不生成多份代码）
```

### IrGen 输出（代码共享）

```
// 源码
T Max<T>(T a, T b) where T: IComparable<T> { ... }
var r = Max<int>(3, 5);

// 生成的 IR（一份代码，不分 int/string）
.func @Max  params:2  ret:object  mode:Interp
  .type_params  T
  .constraints  T: IComparable<T>
  .block entry
    ; a = %0, b = %1
    %2 = v_call  %0.CompareTo  %1    ; T.CompareTo(T) — 通过 vtable 分发
    %3 = const.i64  0
    %4 = gt  %2, %3
    br.cond  %4  then  else
  .block then
    ret  %0
  .block else
    ret  %1

// call site
%5 = const.i64  3
%6 = const.i64  5
%7 = call  @Max  %5, %6              ; 无需 @Max_int
```

### zbc 二进制扩展

SIGS section 扩展：

```
每个函数签名追加：
  type_param_count: u8
  Per type_param:
    name_idx: u32  (STRS pool)
```

TypeDesc 扩展（类实例化时）：

```
TypeDesc {
    name: "Stack",
    type_params: ["T"],           // 泛型定义的参数名
    type_args: [],                // 非实例化时为空
    // ... 现有字段 ...
}

// 实例化后（运行时创建）
TypeDesc {
    name: "Stack<int>",
    type_params: ["T"],
    type_args: ["int"],           // 具体类型参数
    // fields/vtable 与 Stack 共享
}
```

---

## 运行时支持

### 反射

```z42
var stack = new Stack<int>(10);
var type = stack.GetType();
// type.Name == "Stack<int>"
// type.TypeArgs == ["int"]

if (stack is Stack<int>) {
    // true — 运行时类型信息完整
}
```

### TypeDesc 扩展

```rust
pub struct TypeDesc {
    pub name: String,
    pub base_name: Option<String>,
    pub fields: Vec<FieldSlot>,
    pub field_index: HashMap<String, usize>,
    pub vtable: Vec<(String, String)>,
    pub vtable_index: HashMap<String, usize>,

    // === L3 泛型新增 ===
    /// 泛型参数名：["T"]、["K", "V"]
    pub type_params: Vec<String>,
    /// 实例化时的具体类型：["int"]、["string", "int"]
    /// 定义时为空，实例化时填充。
    pub type_args: Vec<String>,
    /// 关联类型映射：{"Output" => "int"}
    pub assoc_types: HashMap<String, String>,
}
```

### 实例化机制

当 VM 遇到 `new Stack<int>(10)` 时：

1. 查找 `Stack` 的 TypeDesc（泛型定义）
2. 创建 `Stack<int>` 的实例化 TypeDesc（clone + 填充 type_args）
3. 缓存实例化 TypeDesc（相同 type_args 共享）
4. ScriptObject 引用实例化后的 TypeDesc

```rust
/// 泛型实例化缓存：(generic_name, type_args) → TypeDesc
type_instantiation_cache: HashMap<(String, Vec<String>), Arc<TypeDesc>>
```

---

## 实施路线

| 子阶段 | 内容 | 涉及模块 | 状态 |
|--------|------|---------|:----:|
| **L3-G1** | 泛型函数 + 泛型类（无约束） | Parser → TypeChecker → IrGen → VM | ✅ |
| **L3-G2** | 接口约束（`where T: I + J`） | Parser + TypeChecker + stdlib | ✅ |
| **L3-G3** | 关联类型 + zbc 格式扩展 + VM 运行时约束校验 + 反射 | 接口声明 + TypeChecker + VM loader | 📋 |
| **L3-G4** | 泛型标准库（`List<T>`、`Dict<K,V>` 原生化 + primitive 接口实现） | stdlib 重构 | 📋 |

## L3-G2 落地细节（2026-04-22）

### 语法：`where` 子句

放在签名尾部、`{` / `=>` 前：

```z42
T Max<T>(T a, T b) where T: IComparable<T> { ... }

class Sorted<T> where T: IComparable<T> + IEquatable<T> { ... }

void Copy<K, V>(K k, V v) where K: IHashable, V: ICloneable { ... }
```

- `+` 组合单参数上的多约束（Rust 风格）
- `,` 分隔不同类型参数的约束项（C# 熟悉感）

### 语义

- `Z42GenericParamType(Name, Constraints)` 携带约束接口列表
- 泛型体内 `t.Method()` 在 constraint 接口的方法表中查找，dispatch 为 VCall
- 调用点（泛型函数 / `new Class<T>(...)`）编译期校验类型参数实现所有约束
- 未约束的 T 上任何方法调用直接报错（E0402）
- 自由函数调用时 T 从实参推断后做约束校验；返回类型也按推断做 T → 具体类型替换（`Max<T>(T, T) → T` 调用时返回类型替换为推断出的 T）

### 限制（本阶段）

- primitive 类型（int/string/...）**未**实现 interface，`Max<int>(1, 2)` 暂不可用（L3-G4 配合 stdlib 泛型化同步放开）
- 约束不写入 zbc 二进制（仅编译期使用），VM 不做运行时校验（**L3-G3 必须补齐**）
- 其他约束范式排期见 L3-G2.5 子迭代（见下）

## L3-G2.5 基类约束（2026-04-22 增量）

在 L3-G2 interface 约束基础上补齐基类约束：

```z42
class Animal { public int legs; }
class Dog : Animal { ... }

void Introduce<T>(T pet) where T: Animal {
    Console.WriteLine(pet.legs);   // 基类字段
    pet.Describe();                // 基类方法（VCall）
}

// 组合基类 + 接口
void F<T>(T x) where T: Animal + IDisplay { ... }
```

### 语义约定

- **基类唯一**：单继承模型，`where T: A + B` 其中 A B 都是类 → 报错
- **基类必首位**：`where T: IFoo + Animal` → 报错（遵循 C# 惯例，简化解析）
- **成员查找顺序**：先基类字段/方法，后接口方法（声明顺序）
- **调用点校验**：typeArg 须等于或继承自基类；复用 `SymbolTable.IsSubclassOf`（O(1) 祖先集）
- **VM 行为**：不区分约束类型 — 方法调用一律 VCall，按 T 实际 TypeDesc.vtable 分发

### 引用 / 值类型约束（L3-G2.5 refvalue，2026-04-22）

```z42
T Default<T>() where T: class { return null; }                // T 限引用类型
void LogValue<T>(T v) where T: struct { Console.WriteLine(v); } // T 限值类型
```

**实现要点**：
- `GenericConstraint.Kinds: [Flags] GenericConstraintKind`（Class / Struct），与类型约束 `Constraints: List<TypeExpr>` 并列
- `GenericConstraintBundle.RequiresClass / RequiresStruct` 传到 ValidateGenericConstraints
- `class` 满足：`Z42ClassType && !IsStruct` 或 `IsReferenceType(t) == true`（含 interface / array / option / string）
- `struct` 满足：`Z42ClassType && IsStruct` 或 `Z42PrimType && !IsReferenceType`（int/bool/float/double/char）
- 互斥：`where T: class + struct` → 编译期报错
- 组合：`where T: class + IFoo` / `where T: struct + IBar` / `where T: BaseClass + class` 都允许

### 源码级泛型容器清理（L3-G4g，2026-04-22）

基于 L3-G4f 基础进行一次容器实现清理：

- **`Std.Collections.ArrayList<T>`** 补齐 `Contains` / `IndexOf`（通过 `where T: IEquatable<T>`）
- **`Std.Collections.HashMap<K,V>`** 新上线 — 开放寻址 + 线性探测，支持 indexer / Set / Get / ContainsKey / Remove / Clear / Count
- **Stub 文件** (`List.z42` / `Dictionary.z42` / `HashSet.z42`) 清理，指向实际实现

**修复的基础设施 gap**：

1. **libsDirs 走查向上搜索**（`PackageCompiler.BuildLibsDirs`）：stdlib 包互编时
   能看到兄弟包的 zpkg（之前 `projectDir/artifacts/build/libs/release` 不存在即完全丢依赖）
2. **TSIG 不再导出 imported 类**（`ExportedTypeExtractor.ExtractClasses`）：以前
   `sem.Classes` 包含本地+导入，TSIG 会重新导出导入的类，导致消费方把
   `new HashMap<...>()` 路由到错误命名空间（如 `Std.Text.HashMap`）
3. **Z42ArrayType `T[]` 赋值跨约束态**：字段存的 `T[]`（无约束）与方法体内
   active 约束下的 `T[]` 之前记录相等性失败；现在按元素名忽略约束差异
4. **IEquatable<T> 增加 GetHashCode**：配对 Equals 对应的哈希能力，让
   `HashMap<K,V>` 可以 `key.GetHashCode()` + `key.Equals(other)` 同时依赖 constraint

**发现的 z42 语义限制**（记入 L3-G4h）：

- **pseudo-class List/Dictionary 仍并存**：真替换要先有 foreach iterator 协议（L3-G4h）

### 短路求值（L3-G4h step 1，2026-04-22）

`&&` / `||` 在 IR 层 desugar 为 `BrCond` 控制流块，右侧仅在左侧未定结果时求值：

- `a && b` → `if a then b else false`
- `a || b` → `if a then true else b`

保留 `AndInstr` / `OrInstr` 仅服务于位运算 `&` / `|`（bool 操作数不再使用）。
`HashMap.FindSlot` 随即回归自然写法 `occupied[s] && !keys[s].Equals(k)`。
golden test `short_circuit` 覆盖：左真/左假 RHS 副作用观察、null-guard 惯用法、链式 &&/||、优先级混用。

### 构造器约束（L3-G2.5 ctor，2026-04-23）

`where T: new()` — 要求类型实参有无参构造器。语法复用 `+` AND 分隔器：

```z42
class Factory<T> where T: class + new() {
    T Create() { return null; }  // body 内 `new T()` 待 L3-R 实现
}

void Main() {
    var f1 = new Factory<Widget>();   // ✅ Widget() 无参 ctor 可见
    var f2 = new Factory<int>();      // ✅ primitive 默认构造
    var f3 = new Factory<NeedsArg>(); // ❌ NeedsArg 只有 NeedsArg(int)
    var f4 = new Factory<IShape>();   // ❌ interface 不可实例化
}
```

**实现范围**：
- 编译期**校验**完整（`TypeChecker.HasNoArgConstructor`）
- 实际 **`new T()` 泛型 body 实例化未实现** —— 依赖 L3-R 的运行时 type_args 传递机制
  （code-sharing IR 下 T 被擦除，无法在 body 知道具体 class name）
- zbc / TSIG flags bit `0x10` 承载 `RequiresConstructor`；与现有 class/struct/base/tp-ref
  共享 flags 字节，所有 flag 可组合

**设计决策记录（2026-04-23 写入）**：

- **约束合取使用 `+`**（Rust 风格）而不是 `,`（C# 风格）：`where T: A + B, U: C + D` 一条
  `where` 同时覆盖多参数 + 多约束；`,` 只用于切换参数。C# 的 `where T: A, B where U: C`
  repeated-where 更冗长，视觉易与 `Dictionary<K, V>` 冲突
- **不支持 OR 约束 `T: A | B`**：主流语言都没有（C# / Java / Rust / Swift / Scala），
  核心困难是函数 body 只能调用 A ∩ B 的方法交集，实用价值低；替代方案（共同基接口 /
  方法重载 / 和类型 ADT）更清晰。z42 遵循主流约定，不做 OR 约束

### enum 约束（L3-G2.5 enum，2026-04-23）

`where T: enum` — 要求类型实参是 z42 原生 enum 类型。用于泛化 flags、解析器、
序列化工具（`Parse<T: enum>(string) -> T`、`AllValues<T: enum>() -> T[]` 等场景）。

```z42
enum Color { Red = 10, Green = 20, Blue = 30 }

T Identity<T>(T x) where T: enum {
    return x;   // body 内 T 被擦除为 i64，enum 反射式操作待 L3-R
}

void Main() {
    var c = Identity(Color.Green);   // ✅ Color 是 enum
    // Identity(42);                  // ❌ int 不是 enum
    // Identity<IShape>(...);         // ❌ interface 不是 enum
}
```

**实现范围**：
- 语义层新增 `Z42EnumType : Z42Type`（`SymbolCollector` / `SymbolTable` 的
  `ResolveType` 在 enum 名查到时发射 `Z42EnumType` 而非 fallback 到 `Z42PrimType`）
- 编译期校验完整（`TypeChecker.IsEnumArg`）；`Z42EnumType` 满足 `struct` 约束
  （enum 是值类型），不满足 `class` 约束
- body 内 `T.Values` / `T.Parse` / flags 位运算等反射式操作**依赖 L3-R**
  运行时 type_args 传递；本迭代只做约束校验
- zbc / TSIG flags bit `0x20` 承载 `RequiresEnum`

**互斥规则**：
- `class + enum` 被拒（enum 是值类型）
- `struct + enum` 允许但冗余（enum 已隐含 struct 语义）
- `enum + new()` 允许（enum 天然 default-constructible）
- `enum + IXxx<...>` 允许；enum 暂不能 implements interface（待 L3-R）

### extern impl — 追溯接口实现（L3，Change 1，2026-04-23）

**动机**：在类型定义之外声明接口实现，支持组织性分离 + stdlib 模块化扩展（下一阶段接通 extern 方法 + TSIG 后，z42.numerics 可为 z42.core 的 `int` 追加 `INumber<int>`）。

**Change 1 语法**：

```z42
interface IGreet { string Hello(); }

class Robot { string name; Robot(string n) { this.name = n; } }

// impl 块在 class body 之外声明接口实现
impl IGreet for Robot {
    public string Hello() { return "beep from " + this.name; }
}

string Greet<T>(T t) where T: IGreet { return t.Hello(); }
```

**等价性**：`impl Trait for Type { ... }` 等价于 class 头部 `: Trait` + body 方法。SymbolCollector 把 trait 合并到 target 的 `InterfaceTypes`，方法合并到 `Methods`。

**Change 1 限制**：
- Target 接受 user class / struct + primitive struct（int/double/bool/char）+ 导入的 class
- Trait 必须是 interface（本地或导入）
- 孤儿规则宽松：允许 impl 出现，不做跨 zpkg 严格检查
- **不含** TSIG Impls 字段；impl 仅在当前 CU 生效，下游消费者看不到（L3-Impl2 补全跨包传播）

**永久禁止：impl 块内 `extern` 方法**（Decision 2026-04-26）：

`extern` 关键字的语义是"VM intrinsic / host FFI 绑定"，是类型本身的一部分（与
类型同生命周期）。`int.op_Add` 的 native 绑定属于 [Int.z42](../../src/libraries/z42.core/src/Int.z42)
的 struct body，**不应该被任何外部包通过 impl 块追加**。

理由：
1. **语义边界清晰**：extern = 类型与 VM ABI 的契约，与类型定义不可分割
2. **impl 的动机已被脚本 body 覆盖**：组织性分离 + 跨包扩展接口 → 包装现有 extern + 用接口暴露即可：
   ```z42
   // z42.numerics（未来包）：给 int 加 INumber<int>
   impl INumber<int> for int {
       public static int op_Add(int a, int b) { return a + b; }  // + 走 Int.z42 已有 extern
   }
   ```
3. **避免复杂度爆炸**：允许 extern in impl 会让孤儿规则 + 跨包传播额外处理"native binding 谁注册 / 重复注册冲突"
4. **真要给 primitive 加 VM intrinsic**：直接在 z42.core 对应类型 body 加 `[Native]` extern method，stdlib 内部调整即可

Parser 在 `impl` 块见到 `extern` 修饰符直接报错（`TopLevelParser.ParseImplDecl`）。

**已落地（L3-Impl2，2026-04-26 cross-zpkg-impl-propagation）**：
- zpkg 加 `IMPL` section（zbc v0.7 → v0.8），承载本 CU 所有
  `impl Trait for Type` 声明（仅签名，方法 body 仍在 MODS）
- 消费者 `ImportedSymbolLoader` 增加 Phase 3 合并：把 impl 方法 TryAdd
  到 imported `Z42ClassType.Methods`，trait 加进 `ClassInterfaces[target]`
- 详细机制见 `docs/design/compiler/compiler-architecture.md` "跨 zpkg impl 块传播"

**后续迭代规划**：
- **+孤儿规则收紧**：Rust 风完整规则 — impl 必须与 Trait OR Target 同 zpkg

**诊断**：新错误码 `E0413 InvalidImpl`（target 非 class/struct、trait 非 interface、签名不匹配、漏方法、重复方法）。

### 静态抽象接口成员（L3 static-abstract，2026-04-24）

**目标**：C# 11 对齐。接口声明 `static abstract / static virtual / static` 三档
静态成员；实现者类型用 `static override` 提供具体实现；泛型代码 `where T : I<T>`
以 `a + b` 或 `x.op_Add(y)` 形式调用，编译器+VM 值驱动派发到实现者的 static 方法。

**接口端三档（D1'）**：

```z42
public interface INumber<T> where T : INumber<T> {
    // 抽象：实现者必须 static override
    static abstract T op_Add(T a, T b);

    // 虚：带默认 body，实现者可选 override（iter 2+ 完整落地）
    static virtual T Double(T x) { return x; }

    // 具体（sealed by default）：实现者不可 override
    static T VersionTag(T x) { return x; }
}
```

**实现者端 `static override`**：

```z42
public struct int : INumber<int> {
    public static override int op_Add(int a, int b) { return a + b; }
    // ...
}
```

**三档 Kind 编码**（`Z42StaticMember.Kind`）：

| 档 | 关键字 | Body | 实现者要求 |
|---|------|:---:|-------|
| 1 | `static abstract` | ❌ | **必须** `static override`（否则 E0412） |
| 2 | `static virtual` | ✅ | 可选 `static override`（省略时继承默认） |
| 3 | `static`（无 virtual/abstract） | ✅ | 不可 override（E0412 sealed） |

**关键实现决策：值驱动派发 = VCall**（非新 IR 指令）

对 `a + b` where a,b: `T where T: INumber<T>`：

1. **TypeChecker**（`TryLookupInstanceOperator` in `TypeChecker.Exprs.cs`）：
   - Path A：`iface.Methods[op_Add]` 1 参（历史实例形式）
   - Path B：`iface.StaticMembers[op_Add]` 2 参（本迭代新增）
   - 两者统一生成 `BoundCall(Virtual, left, iface, op_Add, [right])`

2. **IrGen**：BoundCall(Virtual) → `VCallInstr(obj=left, method=op_Add, args=[right])`
   （复用现有虚派发通路）

3. **VM interp / JIT**：
   - `primitive_class_name(obj)` 解析原语类 → 调 `Std.Int32.op_Add(a, b)`
   - 对象走 vtable → 找不到则 fallback `{class}.{method}` 直接拼接
   - VCall 调用约定 `(obj, ...extras)` 与 2 参静态 op 的 `(a, b)` 签名天然匹配

**没有新 IR 指令 / 没有 VM 改动** — 极简设计。

**TSIG 跨 zpkg 支持**（`ExportedTypeExtractor` + `ImportedSymbolLoader`）：

- 接口静态成员串行化为 `ExportedMethodDef { IsStatic=true, IsAbstract, IsVirtual, ... }`
- `Z42GenericParamType` 序列化为其 Name（"T"）；消费端 `tpSet` 恢复为泛型参
  （避免 Z42UnknownType 污染签名）
- 类级 TypeParams 也参与 ResolveTypeName 的 tpSet，确保 `List<T>.Add(T)` 等
  泛型类方法签名跨 zpkg 完整往返

**SymbolCollector 第五遍验证**（`VerifyStaticOverrides` in `SymbolCollector.Classes.cs`）：

| 场景 | 诊断 |
|------|-----|
| 抽象档无 `static override` | E0412 InterfaceMismatch（漏缺） |
| `static override` 无目标 | E0412（拼写防护，`NoOverrideTarget`） |
| override Concrete 档 | E0412（sealed，`CannotOverrideSealed`） |
| 同名 static 但无 override 关键字 | E0412（隐藏接口成员） |

**宽容模式**：当 class 实现的所有接口都未在 `_interfaces` 中（同包 sibling 未编译 /
TSIG 缺失），`VerifyStaticOverrides` 跳过 "no target" 诊断，避免误报。

**iter 1 Scope**：
- ✅ 5 个二元算术 operator 静态抽象（op_Add/Sub/Mul/Div/Mod）
- ✅ 泛型 `a + b` 和 `x.op_Add(y)` 两种形式
- ✅ stdlib INumber + int/long/float/double 迁移
- ✅ Golden test `static_abstract_operator` + 原 test 87 更新
- ⏸ 静态属性 `static abstract T Zero { get; }` — iter 2
- ⏸ 类型级访问 `T.Zero` / `T.Parse(s)` — iter 2（需 L3-R runtime type_args）
- ⏸ 一元 / 比较运算符 — iter 3
- ⏸ `sealed override` / `new` — iter 2+

**相关**：`docs/design/language/static-abstract-interface.md`（完整设计书），
`docs/spec/archive/2026-04-24-add-static-abstract-interface/`

### Operator 重载（L3 operator-overload，2026-04-24）

**目标**：C# 风 `operator` 关键字，支持二元算术 `+ - * / %` 的运算符重载。
包括异构算子（`Vec2 * int`）。静态方法形式；编译器 desugar `a + b` 为
`Type.op_Add(a, b)` 静态调用。

**语法（C# 对齐）**：

```z42
public struct Vec2 {
    int x; int y;
    public static Vec2 operator +(Vec2 a, Vec2 b) { return new Vec2(a.x+b.x, a.y+b.y); }
    public static Vec2 operator *(Vec2 v, int s) { return new Vec2(v.x*s, v.y*s); }
}

var c = a + b;    // desugars to Vec2.op_Add(a, b)
var d = v * 10;   // Vec2.op_Multiply(v, 10) — heterogeneous OK
```

**运算符 → 方法映射**：

| 运算符 | 方法名 | 与 INumber 对齐 |
|--------|--------|:---:|
| `+` | `op_Add` | ✅ |
| `-` | `op_Subtract` | ✅ |
| `*` | `op_Multiply` | ✅ |
| `/` | `op_Divide` | ✅ |
| `%` | `op_Modulo` | ✅ |

方法名与 `INumber<T>` 的实例方法名相同（非 C# IL 的 `op_Addition`），确保两套机制
互不冲突且可共存。

**Desugar 优先级**（`TryBindOperatorCall` in `TypeChecker.Exprs.cs`）：
1. Primitive 双方（int + int 等）→ **早退，走 BinaryTypeTable / AddInstr 快路径**
2. 静态 `op_Add(L, R)` on left.Type 或 right.Type（签名匹配）→ Static call；
   2026-04-24 起也覆盖 generic T 的静态抽象接口派发（VCall 值驱动）
3. 用户类（非 INumber）的实例 `left.op_Add(R)` 方法 → Virtual call
4. 全未命中 → 原 "requires numeric operand" 错误

**Scope（本迭代）**：
- 5 个二元算术运算符
- 静态 operator 方法（C# 规则）
- 类型签名匹配（含异构）

**后续迭代规划**：
- 比较运算符 `<` / `<=` / `>` / `>=`（走 IComparable）
- 相等运算符 `==` / `!=`（走 IEquatable）
- 一元运算符 `-x` / `!x` / `~x`
- 复合赋值 `+=` 等（纯语法糖）

### INumber 数值约束（L3-G2.5 INumber 迭代 1，2026-04-23 — **已废弃，2026-04-24**）

> ⚠️ **DEPRECATED**：本小节描述 INumber 实例方法形式（`T op_Add(T other)`），
> 已被 "静态抽象接口成员"（本文档前面小节，2026-04-24）全面替代。stdlib INumber
> + int/long/float/double 全部迁移到 `static abstract T op_Add(T a, T b)` 形式；
> `TryLookupInstanceOperator` 的泛型参 Path A（1 参实例查询）已移除。
> 本节文字保留作为历史脉络；**新代码不得再按本节模式实现**。

---

**目标**：泛型数值算法（`Sum<T>`、向量、矩阵、通用算术）通过
`where T: INumber<T>` 约束泛化；primitive（int / long / float / double）
原生参与。

**设计原则对齐**：完全遵循 **Script-First**（philosophy.md §8）:
primitive 的 `op_Add` 等方法**用纯脚本 body 实现**，body 内的 `+`/`-`/...
编译为 IR AddInstr 等指令。**零新 VM builtin**，**零 codegen 特化**。

```z42
// z42.core/src/INumber.z42
public interface INumber<T> {
    T op_Add(T other);
    T op_Subtract(T other);
    T op_Multiply(T other);
    T op_Divide(T other);
    T op_Modulo(T other);
}

// z42.core/src/Int.z42
public struct int : IComparable<int>, IEquatable<int>, INumber<int> {
    ...
    public int op_Add(int other)      { return this + other; }
    public int op_Subtract(int other) { return this - other; }
    ...
}
```

用户类：

```z42
struct Vec2 : INumber<Vec2> {
    int x; int y;
    public Vec2 op_Add(Vec2 other) { return new Vec2(this.x + other.x, this.y + other.y); }
    ...
}

T Sum3<T>(T a, T b, T c) where T: INumber<T> {
    return a.op_Add(b).op_Add(c);
}
```

**选址决策**（为什么 INumber 放 z42.core 而非 z42.numerics）：
- `struct int : INumber<int>` 在 z42.core 声明，所以 INumber 必须 z42.core
  可见
- 反向（INumber 在 z42.numerics）会让 z42.core 反向依赖 z42.numerics，循环
- INumber 接口本身仅 ~10 行，不构成 z42.core 体积问题
- 未来若有更高级数值抽象（IAdditive / IVector 等）再放 z42.numerics，
  不强依赖 INumber

**TypeChecker 两个小修复**（本次一并完成）：

1. `CollectInterfaces`：设置 `_activeTypeParams` 让接口方法里的 `T` 解析为
   `Z42GenericParamType`（此前对返回位置的 T 会 fallback 为 `Z42PrimType("T")`；
   IComparable/IEquatable 的返回类型都是具体的 int/bool，未触发此 bug）
2. `BindClassMethods` / `BindImplMethods`：primitive struct 的方法体内 `this`
   绑定为 `Z42PrimType`（调 `TypeRegistry.GetZ42Type(className)` 优先），
   让 `this + other` 在 primitive struct 里类型检查通过
3. TSIG `ExportedInterfaceDef` 新增 `TypeParams` 字段，跨文件 / 跨 zpkg
   消费时 `RebuildInterfaceType` 根据 TypeParams 把方法签名里的 T 还原为
   `Z42GenericParamType`（此前还原成 PrimType 导致对比失败）

**Scope 限制**（留给后续迭代）：
- bool / char 不实现 INumber（数值语义不适用）
- 未来测量到性能问题可加 IrGen 特化（primitive 的 `op_Add` 直接替换为 AddInstr，
  省掉函数调用开销）；当前纯 script 完全工作
- operator `+` → `op_Add` 自动 desugar（`Sum` 用户可写 `a + b` 而非 `a.op_Add(b)`）—
  独立迭代，见后续 L3 规划

### primitive-as-struct（L3-G4b 重构，2026-04-23）

**设计目标**：消除 `PrimitiveImplementsInterface`（C# 编译器内）和
`primitive_method_builtin`（Rust VM 内）两张硬编码桥接表 — 让 `int` / `double` /
`bool` / `char` / `string` 通过 **stdlib 的 struct 声明** 来声明接口实现
（参考 C# BCL 模型：`System.Int32` 是一个 struct，带 `IComparable<int>` 等接口）。

#### stdlib 声明

```z42
// z42.core/src/Int.z42
namespace Std;

public struct int : IComparable<int>, IEquatable<int> {
    [Native("__int_parse")]      public static extern int Parse(string s);
    [Native("__int_compare_to")] public extern int CompareTo(int other);
    [Native("__int_equals")]     public extern bool Equals(int other);
    [Native("__int_hash_code")]  public extern int GetHashCode();
    [Native("__int_to_string")]  public extern string ToString();
}
```

同样方式为 `double` / `bool` / `char` 声明 struct。`string` 继续使用 uppercase
`class String` 但追加 `IComparable<string>` / `IEquatable<string>`（规范化映射）。

#### 三个支撑层改动

| 层 | 原硬编码 | 现在 |
|----|---------|------|
| **Parser** | 不允许 primitive 关键字作 struct 名 | `ExpectTypeDeclName` 接受 `int`/`double`/... 作为声明名 |
| **TypeChecker** | `PrimitiveImplementsInterface` switch 表（20+ 条） | 数据驱动：查 `SymbolTable.ClassInterfaces[canonical]`（通过 `TypeRegistry.StdlibClassName` 把 keyword `int / i32` 归一为 BCL 名 `Int32`） |
| **VM** | `primitive_method_builtin` 方法-内置名映射（17 条） | `primitive_class_name` 仅 6 条变体→类名映射；实际方法派发走 `module.func_index["Std.Int32.CompareTo"]` → stdlib 生成的 extern stub 调 `__int32_compare_to` builtin |

#### 类型身份保守

**`Z42PrimType("int")` 仍然是 int 在类型系统里的身份** —— 不切换为 `Z42ClassType`。
避免 `IsAssignableTo` / `IsReferenceType` / 全局 `== Z42Type.Int` 比较面临
大面积审计。仅 "接口实现查询" 和 "方法派发查询" 两条路径改走数据驱动。

#### TSIG 扩展

新增 `ImportedSymbols.ClassInterfaces: Dictionary<string, List<string>>`
承载 "class X 声明了哪些接口" 的导出数据。`ExportedTypeExtractor` 从
`SemanticModel.ClassInterfaces` 读取；消费者 `SymbolCollector.MergeImported`
填充 `_classInterfaces` 以供 `PrimitiveImplementsInterface` 查询。

#### struct 现在可实现接口

C# 对齐：删除了 `struct X cannot implement interfaces` 硬性禁令。struct 可以
`: IComparable<T>` 等，既支持 primitive-as-struct，也允许用户 value type 表达
协议一致性（如 2D 坐标 `struct Point : IEquatable<Point>`）。

#### 未来：INumber 等新接口如何加

只需在 stdlib 写 `struct int : ..., INumber<int>` 多加 5 个 extern 方法即可。
**零编译器 / VM 改动** —— primitive 新接口支持从"改硬编码表"变成"纯 stdlib 声明"。

### 跨参数约束链校验（L3-G2.5 chain，2026-04-23）

接口约束的 TypeArgs 现在参与校验而不仅仅是名字匹配：

- `Z42InterfaceType` 新增可空 `TypeArgs` 字段；`SymbolTable.ResolveGenericType`
  遇到带参接口引用时构造带 TypeArgs 的新实例
- `ValidateGenericConstraints` 在检查前用当前 call-site 的 type-arg map
  替换接口 TypeArgs 里的 type-param 引用（`IEquatable<U>` + U=string → `IEquatable<string>`）
- Primitive 类型只满足自指 `I<Self>` —— `int` 实现 `IEquatable<int>` 但不实现
  `IEquatable<string>`；Z42GenericParamType 按 name + args 双重匹配

```z42
class Foo<T, U> where T: IEquatable<U>, U: IEquatable<U> { ... }

void Main() {
    var ok  = new Foo<int, int>(0, 0);        // ✅ int 满足 IEquatable<int>
    var bad = new Foo<int, string>(0, "x");   // ❌ int 不满足 IEquatable<string>
}
```

### 类侧 TypeArg 跟踪（L3-G2.5 chain class-ifaces，2026-04-23）

Class 声明的接口列表升级后：

- AST `ClassDecl.Interfaces` 从 `List<string>` 改为 `List<TypeExpr>`（parser 用
  `TypeParser.Parse` 代替 skip-generic-params）
- `SymbolTable.ClassInterfaces` 从 `Dictionary<string, HashSet<string>>` 改为
  `Dictionary<string, List<Z42InterfaceType>>`，每个条目携带完整 TypeArgs
- `ClassSatisfiesInterface` 走类继承链，对齐 name + args（Instantiated 类通过
  `BuildSubstitutionMap` 做 typeparam 替换）
- `TypeArgEquals` 辅助：class / prim / generic-param 按**名字**比对（避免 record
  结构相等在 collection phase stub vs full 间错配）

```z42
interface IEquatable<T> { bool Equals(T other); }
class IntEq : IEquatable<int> { ... }          // 存 [int]
class Pair<U> : IEquatable<U> { ... }          // 存 [U]

class Foo<T> where T: IEquatable<int> { ... }

new Foo<IntEq>()         // ✅ IntEq 的 IEquatable<int> 精确匹配
new Foo<Pair<int>>()     // ✅ U→int 替换后 IEquatable<int> 匹配
new Foo<Pair<string>>()  // ❌ 实际是 IEquatable<string>
```

**留待独立迭代**：TSIG 当前只按名字序列化 imported 类的 interface
（`ExportedClassDef.Interfaces: List<string>`）；跨 zpkg 消费 `class Foo: IEquatable<int>`
时 args 丢失。本地类 arg-aware 检查 100% 生效，imported 类走 lenient 路径。

### 跨 zpkg 约束消费（L3-G3d，2026-04-23）

stdlib 泛型类的 `where` 子句现在随 zpkg TSIG 传播给消费方，`TypeChecker` 编译期
（而非 VM 加载期）即可拒绝非法类型实参：

- **新 TSIG 字段**：`ExportedClassDef.TypeParamConstraints` / `ExportedFuncDef.TypeParamConstraints`
  （`List<ExportedTypeParamConstraint>?`），接口 / 基类 / tp-ref 以名字存储；
  forward-compat guard 保证旧 zpkg 不读失败
- **序列化布局**（writer / reader 对称）：`u8 count`，每条
  `{u32 tpName, u8 flags, u8 ifaceCount, u32* ifaces, [u32 base]?, [u32 tpRef]?}`
- **Consumer rehydration**：`ImportedSymbolLoader` 携带原始列表；`TypeChecker` 在
  Pass 0.5 后 `MergeImportedConstraints` 将名字解析回 `Z42InterfaceType` /
  `Z42ClassType` 产出 `GenericConstraintBundle`；本地 decl 同名 win
- **覆盖面**：`new ImportedGeneric<T>()` 与 `ImportedGenericFunc<T>(...)` 现在都
  会触发 `ValidateGenericConstraints`

```z42
// 消费端零改动；stdlib List.z42 的 where 会被读到消费方 TypeChecker
class NonEq { int x; NonEq() { this.x = 0; } }

void Main() {
    var bad  = new List<NonEq>();  // ❌ compile error: NonEq 不实现 IEquatable / IComparable
    var good = new List<int>();    // ✅ int 通过 VM primitive 协议满足 IEquatable + IComparable
}
```

**测试**：`TsigConstraintsTests` —— zpkg 二进制 round-trip、负向拒绝、正向通过三个
单元测试验证端到端。

**留到 G2.5 chain**：约束接口的 TypeArgs 当前仍按名字匹配（比如 `T: IEquatable<U>`
只校验"T 必须实现 IEquatable"不检查 U 是否对齐）—— 跨参数链接的完整校验随后独立迭代。

### Pseudo-class List/Dictionary 正式退场（L3-G4h step 3，2026-04-22）

`List<T>` / `Dictionary<K,V>` 从编译器 pseudo-class 快路径迁移到纯源码实现：

- **新源码类**：`Std.Collections.List<T>`（无约束——stdlib-structure-batch 2026-09-03 去掉了原
  `where T: IEquatable<T> + IComparable<T>`，对齐 C#）、`Std.Collections.Dictionary<K,V> where K: IEquatable<K>`。旧的中间产物
  `ArrayList<T>` / `HashMap<K,V>` 源文件**已删除**，其能力合并到 List/Dictionary。
- **Count 统一为 `public int Count` 字段**（直接字段读），替代原来的 `Count()` 方法。
  foreach 协议同时支持 `Count` 字段和 `Count()` 方法，自动适配。
- **新增 `List.Sort` / `Reverse` / `Remove`**：Sort 使用插入排序（约束 `T: IComparable<T>`）；
  Remove 通过 IndexOf + RemoveAt 组合；Reverse 原地反转。
- **编译器清理**：
  - `SymbolTable.ResolveGenericType` 删除 `List`/`Dictionary` pseudo-class 映射
  - `FunctionEmitterExprs.EmitBoundNew` 删除 `__list_new` / `__dict_new` 分支
  - `FunctionEmitterCalls`/`TypeChecker.Calls` 的 `IsBuiltinCollectionType` 收窄到
    `Array` / `StringBuilder`（StringBuilder 仍走 builtin，Array 走 `__list_*`）
  - `ResolveBuiltinMethod` 仅保留 StringBuilder 方法映射；`__list_*` / `__dict_*`
    builtin 不再被编译器发射（VM 侧保留实现，等未来彻底删除）
- **VM 无感知**：`new List<int>()` 现在实例化 `Std.Collections.List` 对象，`Add` /
  `Contains` / `Sort` 等走 Instance/VCall 正常分发；原 `__list_*` / `__dict_*` 仍然
  存在，但没有编译器发射路径。
- **测试迁移**：
  - `stdlib_arraylist` 改名语义：源代码改用 `List<T>`（文件名保留作为迭代标识）
  - `stdlib_hashmap`：改用 `Dictionary<K,V>`
  - `foreach_user_class`：ArrayList 替换为 List
  - `list` / `dict` / `list_operations`：零改动直接跑通 —— `new List<int>()`
    与 `.Count` 字段读与旧 pseudo-class API 等价

### foreach 鸭子协议（L3-G4h step 2，2026-04-22）

`foreach` 不再仅限于数组 / Option —— 任何满足以下协议的**类**（含泛型实例化类型）
都可直接被 `foreach` 迭代：

- `int Count()` —— 返回元素个数
- `T get_Item(int i)` —— 按索引取元素（通常由 indexer 语法声明）

codegen 展开为：

```
i = 0
len = coll.Count()
while (i < len) {
    e = coll.get_Item(i)
    // body
    i = i + 1
}
```

Pseudo-class `List<T>` / `Dictionary<K,V>` 保留在数组 / Map fast path（L3-G4h
step 3 会统一迁移）。`TypeChecker.ElemTypeOf` 做类型参数替换，确保 `var e` 推断为
正确的元素类型（`Z42InstantiatedType` 下 `T` → 具体类型）。

golden test `foreach_user_class` 覆盖自定义 `Ring<T>` + 标准库 `ArrayList<T>` +
break/continue 控制流。

### 源码级泛型容器（L3-G4f，2026-04-22）

`Std.Collections.ArrayList<T>` 上线，纯 z42 源码实现：
- `T[] items` 动态数组 + `int count / capacity`
- `Add` / `Insert` / `RemoveAt` / `Clear` / `Count` / `IsEmpty` / `Grow`
- `public T this[int index] { get; set; }` 索引器

```z42
var xs = new ArrayList<int>();
xs.Add(1); xs.Add(2); xs.Add(3);
xs[0] = 99;
int n = xs[2];
xs.RemoveAt(1);
```

**策略**：ArrayList 与 pseudo-class `List<T>` 并存。用户可选用源码版本（可扩展 / subclass /
添加方法）或继续用 pseudo-class（foreach 友好、零编译器路径开销）。真正移除
pseudo-class 需要先做 foreach iterator 协议（后续迭代）。

**当前限制**（已记录为 L3-G4f 后续项）：
- **Contains / IndexOf 暂缺**：需要 `where T: IEquatable<T>` 约束。跨 `Std.Collections`
  → `Std` 命名空间的约束解析当前有 gap（qualified name `Std.IEquatable<T>` parser 不接受；
  短名 `IEquatable` 在跨模块作用域里失败）
- **HashMap<K,V> 未交付**：依赖同样的 IEquatable/IHash 约束；先等命名空间解析修好

### 索引器语法（L3-G4e，2026-04-22）

类可声明索引器，使 `obj[i]` / `obj[i] = v` 语法可用：

```z42
class MyList<T> {
    T[] items;
    public T this[int i] {
        get { return this.items[i]; }
        set { this.items[i] = value; }
    }
}

var xs = new MyList<int>();
int a = xs[0];      // → get_Item(0)
xs[1] = 99;          // → set_Item(1, 99)
```

**实现要点**：
- Parser 识别 `<vis>? <type> this [params] { get {..} set {..} }`，desugar 为两个
  FunctionDecl：`get_Item(params) → T` 和 `set_Item(params, T value) → void`（`value` 隐式参数名）
- 至少要有 `get` 或 `set` 之一；支持单独 getter / 单独 setter
- TypeChecker `IndexExpr` 分支：target 为 class / InstantiatedType 且有 `get_Item` → BoundCall VCall；
  array / string 仍走既有 ArrayGet
- 赋值 LHS = `IndexExpr` + class 接收者 + `set_Item` → BoundCall VCall（value 作最后一个参数）
- 泛型类透过 L3-G4a SubstituteTypeParams 替换返回/参数类型
- **零 IR 改动**（复用 VCallInstr）；**零 VM 改动**

**限制**：
- 不支持 auto-indexer（`{ get; set; }` 无 body）— 必须显式写 body
- 不支持 `ref this[...]`
- `set` 的隐式参数名固定为 `value`

### stdlib 导出泛型类（L3-G4d，2026-04-22）

让 user 代码能直接 `new Stack<int>()` 指向 stdlib 的 `Std.Collections.Stack<T>`。

**实现要点**：
- **TSIG 格式扩展**：`ExportedClassDef` 新增 `TypeParams: List<string>?`；ZpkgReader/Writer 在 class 条目尾部增加 `tp_count + name_idx[]`（向前兼容：reader 在 section 剩余空间不足时按 0 处理）
- **ExportedTypeExtractor / ImportedSymbolLoader**：跨 zpkg 保留 TypeParams，imported 泛型类在消费方重建时保持泛型性质
- **SymbolCollector 冲突裁决**：local 同名覆盖 imported — 发现 `_classes` 已有同名且属于 `_importedClassNames`，移除 imported 记录，继续注册 local（不报 duplicate）
- **IrGen QualifyClassName**：local 优先 — `Classes.ContainsKey(n) && !ImportedClassNames.Contains(n)` → 用本模块命名空间；否则 imported → 用 `ImportedClassNamespaces` 映射
- **FunctionEmitterCalls DepIndex 守卫**：仅在接收者非本地类时查 DepIndex（否则 local class 方法会被同名 stdlib 方法劫持）
- **VM ObjNew lazy-load**：type_registry 未命中时调 `lazy_loader::try_lookup_type` 触发 zpkg 按命名空间加载；同样对 ctor 函数用 `try_lookup_function` 兜底

**能力**：
```z42
using-like behavior (no using keyword yet, auto-resolved):
var s = new Stack<int>();   // → Std.Collections.Stack<int>
s.Push(1); s.Pop();

class Stack { ... }          // user 定义同名会覆盖
var local = new Stack();    // → local 版本，stdlib 不生效
```

**限制（L3-G4e/f 继续）**：
- 索引器语法 `T this[int]` 未实现 → `List<T>` / `Dictionary<K,V>` pseudo-class 暂不替换
- qualified `new Std.Collections.Stack<int>()` 语法未支持（L3 后期）
- `using` 导入未支持

### User-level 容器源码实现（L3-G4c，2026-04-22）

验证纯 z42 源码能完整实现泛型容器，无需编译器特判。

`src/libraries/z42.collections/tests/generic_list/` 展示用户编写的 `class MyList<T>`：
- `T[]` 动态数组字段 + 扩容（`Grow`）
- `Add` / `Get` / `Set` / `RemoveAt` / `Count`
- 同时支持 `MyList<int>` 和 `MyList<string>`（primitive T 由 L3-G4a/G4b 打通）

**已确认的能力**：
- 泛型类 + 泛型方法 + 泛型 `T[]` 数组（L3-G1/G4a）
- 实例化类型替换（L3-G4a）— `list.Get(0)` 返回具体类型
- Primitive T 和约束（L3-G4b）

**剩余工作（L3-G4d）—— stdlib 导出的泛型类**：
- 名称解析：当前 `new Stack<int>()` 若 stdlib 和 user 都定义了 `Stack` 会冲突
- 需要 SymbolCollector 识别 imported 泛型类并正确路由 ObjNew 到 `Std.Collections.Stack`
- VM VCall 对导入类的 type_desc.name 应为命名空间限定全名

`src/libraries/z42.collections/src/Stack.z42` 和 `Queue.z42` 里以注释形式保留了
参考实现，待 L3-G4d 放开导出路径后即可启用。

### Primitive 类型实现 interface（L3-G4b，2026-04-22）

让 `Max<int>(3, 5)` 真正可用 — primitive 类型（int/double/bool/char/string）
编译期视为实现了 `IComparable<T>` / `IEquatable<T>`；运行时 VCall 分派到
corelib builtin。

```z42
T Max<T>(T a, T b) where T: IComparable<T> {
    return a.CompareTo(b) > 0 ? a : b;
}

Max(3, 5);         // int — OK（L3-G2 只支持 user class，现在 primitive 也行）
Max("a", "b");     // string
Max(1.5, 2.5);     // double
```

**实现要点**：
- TypeChecker `PrimitiveImplementsInterface(primName, ifaceName)` 硬编码表：
  - 整型 / 浮点 / string / char 满足 `IComparable` + `IEquatable`
  - `bool` 只满足 `IEquatable`
- `TypeSatisfiesInterface` 对 `Z42PrimType` 查表；编译期透明
- VM `primitive_method_builtin(Value, method)` 映射到 corelib builtin（interp + JIT 共用）：
  - int  → `__int_compare_to` / `__int_equals` / `__int_hash_code` / `__int_to_string`
  - double/bool/char/string 同构
- 无 IR / zbc 格式改动；VCall 指令不变，dispatch 在 VM 内部分支

### 实例化类型替换（L3-G4a，2026-04-22）

泛型类实例化后，成员访问 / 方法调用的类型需按 type args 替换。

```z42
class Box<T> {
    T value;
    T Get() { return this.value; }
}

var b = new Box<int>(42);
int n = b.Get() + 1;    // Get() 返回 int（而非未替换 T）
int v = b.value;        // 字段 value 为 int
```

**实现要点**：
- `Z42InstantiatedType(Definition, TypeArgs)` 承载实例化形式
- `ResolveGenericType` 当 TypeArgs 数量匹配 TypeParams 时返回 Z42InstantiatedType（否则回退到裸 ClassType 保持 L3-G1 行为）
- `TypeChecker.SubstituteTypeParams(Z42Type, map)` 递归替换 Z42GenericParamType — 覆盖 Array / Option / Func / 嵌套 Instantiated
- BindMemberExpr / BindCall 识别 Z42InstantiatedType 接收者，用 `BuildSubstitutionMap` + Substitute 得到替换后的字段/方法签名
- `IsAssignableTo` / `IsReferenceType` 处理新类型（同 Definition 且 TypeArgs 相等即可赋）
- zbc / IR / VM 无改动（代码共享不变，IR 层仍是单一未实例化形式）

### 裸类型参数约束（L3-G2.5 bare-tp，2026-04-22）

```z42
class Container<T, U> where U: T {
    U child;
    Container(U c) { this.child = c; }
}

// 调用点校验：Dog 必须是 Animal 的子类型
var c = new Container<Animal, Dog>(new Dog());   // ✅
var x = new Container<Animal, Vehicle>(...);      // ❌ E0402
```

**实现要点**：
- `GenericConstraintBundle.TypeParamConstraint: string?` 存另一 type-param 名
- `ResolveWhereConstraints` 优先识别 NamedType ∈ active type params（早于 class/interface 分派）
- 体内成员查找：`SymbolTable.LookupEffectiveConstraints` 做"一跳"合并 — U 查找命中不了走 T 的 bundle
- 调用点 `ValidateGenericConstraints`：拿到 typeArgs 映射后，比较 `typeArg[U]` 与 `typeArg[T]` 的子类型关系（IsSubclassOf；非 class 退回相等）
- zbc 版本 0.5 → 0.6；bundle flag bit3 + 条件 `type_param_name_idx`
- Rust VM `verify_constraints` 对裸 type-param 引用跳过（本地即解）

**限制**：
- 一跳策略：`U: V, V: T` 的两跳不支持（实际场景少）
- primitive / interface 作 typeArg 时只认相等性（不做 primitive 子类型）

### 其他约束范式排期

见 `docs/roadmap.md` L3-G2.5 表：

- `new()` — 依赖 VM 运行时类型参数（L3-R 批次）
- `notnull` — 后续小迭代（与 z42 可空性语义协同设计）

### L3-G2.5 委托/函数约束（add-generic-func-constraint，2026-05-11）

✅ 已完成。详见上方 §约束体系 "委托/函数约束" 段。语法 `where T: Func<int, R>` 或 `where T: (int) -> R`；body 内 `t(args)` 走 CallIndirect；零新 IR / VM 指令。zbc bump 1.3 → 1.4，zpkg 0.4 → 0.5。错误码 E0422 / E0423。

### L3-G3a 已完成（2026-04-22）

- zbc 版本 0.4 → 0.5：SIGS / TYPE section 每个 type_param 追加约束布局
  - `flags: u8`（bit0 RequiresClass / bit1 RequiresStruct / bit2 HasBaseClass）
  - `[if bit2] base_class_name_idx: u32`
  - `interface_count: u8 + interface_name_idx[] × u32`
- C# IR: `IrFunction.TypeParamConstraints` / `IrClassDesc.TypeParamConstraints` 与 `TypeParams` 按索引对齐
- Rust VM: `Function.type_param_constraints` / `TypeDesc.type_param_constraints` 读取并保留
- Rust loader: 加载后运行 `verify_constraints`，对未知 class/interface 引用返回 `InvalidConstraintReference`；对 `Std.*` 前缀和 `I<Upper>...` 接口名放行（分别由 lazy loader 和 L3-G3b 反射补齐）
- ZpkgReader: SIGS 扫描同步跳过新字段

### L3-G3 剩余子阶段

1. **L3-G3b 反射接口**：`type.Constraints` / `t is IComparable<T>` 运行时依据元数据判断
2. **L3-G3c 关联类型**：`type Output; Output=T`
3. **L3-G3d 跨 zpkg**：TSIG section 扩展携带约束，消费方 TypeChecker 做跨模块校验
4. **运行时 Call/ObjNew 约束校验**：代码共享下 type_args 不可得；等 L3-G3b 反射机制出来后再决定如何拿 type_args 并 enforce

### L3-G1 详细 pipeline

```
Parser:     解析 <T> 类型参数列表、where 子句
AST:        FunctionDecl/ClassDecl 新增 TypeParams 字段
TypeCheck:  类型参数作用域；泛型实例化时替换 T → 具体类型
IrGen:      生成共享代码 + .type_params 元数据
ZbcWriter:  SIGS section 写入 type_param_count + names
VM loader:  读取 type_params → TypeDesc
VM interp:  ObjNew 时创建实例化 TypeDesc（填充 type_args）
```

---

## 约束与 Trait 的关系

> z42 features.md 规划：L3 引入 `Trait` 替代 `interface` 作为零开销抽象。

泛型约束天然与 Trait 结合：

```z42
// Trait 定义（L3 后期）
trait Display {
    string Format();
}

// 泛型约束使用 Trait
void Print<T>(T item) where T: Display {
    Console.WriteLine(item.Format());
}
```

**当前阶段（L3-G1/G2）先用 interface 作为约束目标**，Trait 系统独立实现后可无缝替换。

---

## 与现有 pseudo-class 的迁移路径

当前 `List<T>` 和 `Dictionary<K,V>` 是 pseudo-class（编译器特殊处理 + VM builtin）。泛型实现后：

| 阶段 | 状态 |
|------|------|
| L3-G1/G2 | pseudo-class 保持不变；泛型仅用于用户定义的类型 |
| L3-G4 | `List<T>` → 真正的泛型类（替代 pseudo-class） |
| L3-G4 | `Dictionary<K,V>` → 真正的泛型类 |
| L3-G4 | Queue/Stack/HashSet → 新增泛型集合 |
| L3-G4 | 标准库接口类改成泛型的(IComparable, IEquatable等) |

---

## Class arity overloading（2026-05-07）

由 [`docs/spec/archive/2026-05-07-add-class-arity-overloading/`](../../spec/archive/2026-05-07-add-class-arity-overloading/) 落地（D-8b-0）。修复 `class Foo` + `class Foo<R>` 同源名冲突的结构性 type-system gap，与 delegate 的 `Action$N` 命名约定对齐。

### 设计：shadow-only mangling

[`Z42ClassType`](../../src/compiler/z42.Semantics/TypeCheck/Z42Type.cs) 增 `IrName` 派生属性 + `HasArityMangle` 标志：

| 场景 | Registry key | `IrName` | `HasArityMangle` |
|------|-------------|---------|-----------------|
| 单独非泛型 `class Foo` | `Foo` | `Foo` | false |
| 单独泛型 `class List<T>` | `List` | `List` | false |
| 共存 `class Foo` | `Foo` | `Foo` | false |
| 共存 `class Foo<R>` | `Foo$1` | `Foo$1` | **true** |
| 共存 `class Pair<A, B>` | `Pair$2` | `Pair$2` | **true** |

**关键性质**：仅冲突时才走 mangling 路径。stdlib 现有泛型类（List<T> / Dictionary<K,V> / MulticastAction<T> ...）无非泛型同名兄弟 → 全部保持 bare key → **零 zpkg 改动**、零 VM 改动。

### Pre-pass 检测

`SymbolCollector.Classes.cs::CollectClasses` 先 group `cu.Classes` by source name；同源名两个以上时，仅 generic 兄弟（arity > 0）需要 mangling。非泛型永远占 bare 槽位。同 arity 重复仍走 E0408 duplicate path。

### 类型解析路由

```
NamedType("Foo")            → _classes["Foo"]   (always bare)
GenericType("Foo", [T..])   → _classes["Foo$N"] first, fallback _classes["Foo"]
```

非冲突情况下 generic 类住在 bare key，fallback 命中；冲突情况下 mangled 槽位先命中。

### 用户面不变

- `Z42ClassType.Name` user-facing 永远 bare（诊断 / 错误 / `typeof` 不泄漏 `$N`）
- IR / VM type_registry 自动跟随 IrName（mangled 仅在冲突时存在）
- `IrName == Name` 当 HasArityMangle=false → 兼容所有既有 IR 消费路径

### 实施触点（C# 编译器侧）

- [`SymbolCollector.Classes.cs`](../../src/compiler/z42.Semantics/TypeCheck/SymbolCollector.Classes.cs) `K(cls)` / `KeyFor(cls)` helpers + 2-pass pre-pass + 5 个 pass 用 KeyFor
- [`SymbolCollector.cs`](../../src/compiler/z42.Semantics/TypeCheck/SymbolCollector.cs) `ResolveType` GenericType — `Name$N` shadow lookup
- [`SymbolTable.cs`](../../src/compiler/z42.Semantics/TypeCheck/SymbolTable.cs) 镜像 ResolveType
- [`TypeChecker.cs::BindClassMethods`](../../src/compiler/z42.Semantics/TypeCheck/TypeChecker.cs) IrName-aware classKey
- [`TypeChecker.Exprs.cs::case NewExpr`](../../src/compiler/z42.Semantics/TypeCheck/TypeChecker.Exprs.cs) qualName 用 resolved class IrName
- [`TypeChecker.Exprs.Members.cs::ResolveCtorName`](../../src/compiler/z42.Semantics/TypeCheck/TypeChecker.Exprs.Members.cs) ctor 名查找用 cls.Name (bare) 而非 className (可能 mangled)
- [`IrGen.cs`](../../src/compiler/z42.Semantics/Codegen/IrGen.cs) `ClassIrShortName` helper + EmitClassDesc / EmitMethod / EmitImplicitCtor / `_funcParams` 注册全用 IrName

### 限制 / 后续

- **跨 zpkg generic base class**：`class Foo<R> : Bar<int>` 当前不支持（z42 BaseClass 只接 NamedType 字符串），与本变更正交
- **方法层 generic-vs-non-generic 同名**：`class Foo { void m(); void m<T>(); }` 由 method arity overload 已支持，本变更不动
- **D-8b-1 解锁**：stdlib `MulticastException<R>` 现可与现有 `MulticastException` 共存
- **D-8b-3 Phase 2 解锁**：generic type-param `default(R)` 解析现可走 `Z42InstantiatedType.Definition.IrName` 路径
---

## Deferred / Future Work

> 索引也存于 [docs/roadmap.md](../../roadmap.md) "Deferred Backlog Index"。

### D-4: 协变 / 逆变（`<in T, out R>` 等）

- **来源**：[docs/spec/archive/2026-05-02-add-delegate-type/](../../spec/archive/2026-05-02-add-delegate-type/)
- **关联设计文档**：[`delegates-events.md`](delegates-events.md) §12 明确"推迟到 L3 后期"
- **触发原因**：协变 / 逆变涉及泛型 type-arg 关系约束，z42 当前 generic 系统未做这类规则，加进来牵扯 ImportedSymbols / RebuildFuncType / 子类型规则全链路。
- **前置依赖**：L3 后期完整 type-system 规划；与 `generics.md` / `static-abstract-interface.md` 协同。
- **触发条件**：用户大量遇到 `Func<Animal>` ↔ `Func<Dog>` 子类型替换问题。
- **当前 workaround**：依赖具体类型，或手动 wrapper 转换。
