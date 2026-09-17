# 泛型的实现

> **页型**: 机制页 ｜ **状态**: ✅ 已实现（代码共享 + 运行期类型实参）
> **代码**: `src/compiler/z42c.semantics/`（TypeChecker / IrGen / SymbolCollector）·
> `src/libraries/z42.ir/`（zbc TYPE/SIGS 约束布局）· `src/runtime/src/corelib/reflection/generics.rs`
> **相关**: [泛型类型实参推断](generic-inference.md) · [架构总览](architecture.md) ·
> [源代码编译流程](source-compile.md) ｜ **对齐**: 2026-09-17
>
> 用户视角（`where` 能写什么、报什么错、`Self` 怎么用）在参考手册的「泛型约束」与
> 「泛型方法」两页 —— **那里是语义 SoT**，本页不重复规则，只讲实现。
>
> 本页由原 `docs/book/src/language/generics.md`（1521 行）瘦身而来（三书重构批 3b）：
> 删去约 57% —— 与参考手册重复的约束体系、自标 DEPRECATED 的 INumber 实例方法形式、
> 指向已不存在的 C# 编译器的实施触点、以及 roadmap 口径的阶段排期；
> 类型实参推断三节抽成了独立的 [generic-inference.md](generic-inference.md)。

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

## L3-G2 落地细节（2026-04-22）

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
类型同生命周期）。`int.op_Add` 的 native 绑定属于 [Int32.z42](../../../../src/libraries/z42.core/src/Primitives/Int32.z42)
的 struct body，**不应该被任何外部包通过 impl 块追加**。

理由：
1. **语义边界清晰**：extern = 类型与 VM ABI 的契约，与类型定义不可分割
2. **impl 的动机已被脚本 body 覆盖**：组织性分离 + 跨包扩展接口 → 包装现有 extern + 用接口暴露即可：
   ```z42
   // z42.numerics（未来包）：给 int 加 INumber<int>
   impl INumber<int> for int {
       public static int op_Add(int a, int b) { return a + b; }  // + 走 Int32.z42 已有 extern
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

### primitive-as-struct（L3-G4b 重构，2026-04-23）

**设计目标**：消除 `PrimitiveImplementsInterface`（C# 编译器内）和
`primitive_method_builtin`（Rust VM 内）两张硬编码桥接表 — 让 `int` / `double` /
`bool` / `char` / `string` 通过 **stdlib 的 struct 声明** 来声明接口实现
（参考 C# BCL 模型：`System.Int32` 是一个 struct，带 `IComparable<int>` 等接口）。

#### stdlib 声明

```z42
// z42.core/src/Primitives/Int32.z42
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

### L3-G3a 已完成（2026-04-22）

- zbc 版本 0.4 → 0.5：SIGS / TYPE section 每个 type_param 追加约束布局
  - `flags: u8`（bit0 RequiresClass / bit1 RequiresStruct / bit2 HasBaseClass）
  - `[if bit2] base_class_name_idx: u32`
  - `interface_count: u8 + interface_name_idx[] × u32`
- C# IR: `IrFunction.TypeParamConstraints` / `IrClassDesc.TypeParamConstraints` 与 `TypeParams` 按索引对齐
- Rust VM: `Function.type_param_constraints` / `TypeDesc.type_param_constraints` 读取并保留
- Rust loader: 加载后运行 `verify_constraints`。校验按引用**种类**分派（fix-runtime-constraint-unresolved-refs）：
  - **基类引用**（`base_class`）严格——未在 `type_registry`、非 `Std.*` 即返回 `InvalidConstraintReference`（基类是布局/派发关键）。
  - **接口引用**（`interfaces`）与 **func-sig 类型引用** soft-allow 未解析——`verify_constraints` 在惰性加载器建立（`boot_context`）之前跑，约束可能命名一个尚未惰性加载的依赖 zpkg 里的接口（与 `Std.*` 同理），真正解析延后到运行期使用点（解释器触发惰性加载）。
  - 接口自 zbc 1.19 反射起即以 minimal TYPE entry 进 `type_registry`，故同模块/静态合并的接口经 registry 命中即通过；只有真正的惰性依赖接口走 soft 分支。**旧的 `I<Upper>...` 命名启发式已删**——它会硬拒任何非 `IFoo` 命名的接口（`Comparable`/`Iterable`/…）作约束，是个 footgun。
- ZpkgReader: SIGS 扫描同步跳过新字段

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


## Class arity overloading（2026-05-07）

由 [`docs/spec/archive/2026-05-07-add-class-arity-overloading/`](../../../spec/archive/2026-05-07-add-class-arity-overloading/) 落地（D-8b-0）。修复 `class Foo` + `class Foo<R>` 同源名冲突的结构性 type-system gap，与 delegate 的 `Action$N` 命名约定对齐。

### 设计：shadow-only mangling

[`Z42ClassType`](../../../../src/compiler/z42c.semantics/src/Z42Type.z42) 增 `IrName` 派生属性 + `HasArityMangle` 标志：

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

### 限制 / 后续

- **跨 zpkg generic base class**：`class Foo<R> : Bar<int>` 当前不支持（z42 BaseClass 只接 NamedType 字符串），与本变更正交
- **方法层 generic-vs-non-generic 同名**：`class Foo { void m(); void m<T>(); }` 由 method arity overload 已支持，本变更不动
- **D-8b-1 解锁**：stdlib `MulticastException<R>` 现可与现有 `MulticastException` 共存
- **D-8b-3 Phase 2 解锁**：generic type-param `default(R)` 解析现可走 `Z42InstantiatedType.Definition.IrName` 路径
---

## Deferred / Future Work

> 索引也存于 [docs/roadmap.md](../../../roadmap.md) "Deferred Backlog Index"。

### D-4: 协变 / 逆变（`<in T, out R>` 等）

- **来源**：[docs/spec/archive/2026-05-02-add-delegate-type/](../../../spec/archive/2026-05-02-add-delegate-type/)
- **关联设计文档**：[`delegates-events.md`](../runtime/delegates-events.md) §12 明确"推迟到 L3 后期"
- **触发原因**：协变 / 逆变涉及泛型 type-arg 关系约束，z42 当前 generic 系统未做这类规则，加进来牵扯 ImportedSymbols / RebuildFuncType / 子类型规则全链路。
- **前置依赖**：L3 后期完整 type-system 规划；与 `generics.md` / `static-abstract-interface.md` 协同。
- **触发条件**：用户大量遇到 `Func<Animal>` ↔ `Func<Dog>` 子类型替换问题。
- **当前 workaround**：依赖具体类型，或手动 wrapper 转换。

