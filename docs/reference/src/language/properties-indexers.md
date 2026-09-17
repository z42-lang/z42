# 属性与索引器

> 对齐：2026-09-15

属性（property）与索引器（indexer）让字段式 / 下标式语法背后跑用户逻辑，语义与 C# 一致：

| 访问器 | 声明 | 使用 | 编译成 |
|--------|------|------|--------|
| **属性** | `T Name { get; set; }`（auto）/ `T Name { get { ... } }` 或 `T Name => e;`（计算） | `obj.Name` / `obj.Name = v` | `get_Name()` / `set_Name(v)`（auto 另合成后备字段 `__prop_Name`；计算 getter 无后备字段） |
| **索引器** | `T this[P...] { get {...} set {...} }` | `obj[i]` / `obj[i] = v` | `get_Item(...)` / `set_Item(..., v)` |

两者都在编译期降解成普通实例方法（镜像 C# 的 `get_X`/`set_X`、`get_Item`/`set_Item`），
因此天然支持虚派发与跨 zpkg 调用。

---

## 属性（property）

### 自动属性（auto-property）

**自动属性**：访问器只写 `get;` / `set;`（分号结尾），编译器合成后备字段与访问器方法。

```z42
class Person {
    public string Name { get; set; }     // 读写
    public int Age { get; private set; } // 见下：`private` 当前被忽略
    public bool Active { get; }          // 只读（只有 getter）
}
```

- **访问器级可见性修饰符：解析但当前忽略**。`get` / `set` 前可各自写可见性修饰符（如 `private set`），
  parser 接受并记入 AST，但**语义层没有任何读取点**——两个访问器统一继承**属性级**可见性。
  上例的 `Age` 实际是 `get` / `set` 都 public，不是「读公开、写私有」。需要「外部只读」时，
  当前的做法是写 `{ get; }` 只读属性 + 私有字段 / 方法提供写入路径。
- **只读属性**：只写 `{ get; }`。**只能在本类构造函数内经 `this` 赋值**（对标 C# CS0200 的只读
  自动属性），其它位置赋值报 **E0452**；计算属性（`get { ... }`）无存储，**任何位置**都不可赋值。
- **初始化器**：`T Name { get; set; } = expr;` 给后备字段一个初值。语义对标 C#：
  - **直写后备字段，不经 setter**（派生类 override 的 setter 不参与；get-only 也能初始化）；
  - 与字段初始化器**按声明序交错**执行，先于用户 ctor 体；无 ctor 的合成构造同样生效；
  - `: this(..)` 委托 ctor 不重复执行（与字段初始化器同规则）；
  - 只有 auto 属性有存储可初始化——计算属性写初始化器报 **E0452**。

```z42
public int Count { get; set; } = 0;
```

- **默认可见性是 `private`**：属性 / 索引器不写修饰符时与字段 / 方法同规则（见
  [访问权限控制](access-control.md)）。

### 计算属性 getter（`get { ... }`）

getter 可写**块体** `get { <stmts>; return <expr>; }`，在字段 / 其它成员之上**计算**派生值，语义与
C# 计算属性一致。计算 getter **不合成后备字段**——每次读取都执行 getter 函数体（无存储）。

```z42
public class Box {
    public int n;
    public int Doubled { get { return this.n * 2; } }       // 派生自字段
    public bool Big     { get { return this.n > 10; } }      // 布尔派生
    public int Plus     { get { return this.Doubled + 1; } } // 引用另一计算属性
}
// b.Doubled 每次按当前 n 重算；无 __prop_Doubled 后备字段。
```

- **get-only**：只支持计算 `get { ... }` / `get => e;`；`set { ... }`（计算 setter）尚未支持。
- **auto vs 计算的区分**：`get;`（分号）= auto 属性（合成后备字段）；`get { ... }`（块体）=
  计算属性（无后备字段，getter 是真实函数体）。
- getter 体内可访问 `this`、本类字段、其它属性（`this.Doubled` 派发到 `get_Doubled`）。

### 表达式体属性（`=> e;`）

只读计算属性可写成表达式体，语义与 C# 一致：

```z42
public class Box {
    public int n;
    public int Doubled => this.n * 2;          // ≡ { get { return this.n * 2; } }
    public bool Big { get => this.n > 10; }    // 访问器级表达式体，同上
}
public class Tri : Shape {
    public override string Name => "Tri";      // 实现抽象 / 接口属性
}
```

- `T P => e;` 与 `T P { get => e; }` 与 `T P { get { return e; } }` **完全等价**（parser 层脱糖，
  下游无差别）。
- 与表达式体**方法** `int F() => 1;` 靠成员名后的下一个 token 区分：`(` → 方法，`=>` → 属性。
- 非 void 访问器脱糖成 `{ return e; }`；void 访问器（索引器 `set`）脱糖成 `{ e; }`。

### 静态属性

`static` 修饰的属性全形态可用，语义对标 C#：

```z42
class Config {
    public static int Level { get; set; } = 2;     // auto，读写 + 初始化器
    public static string Name => "cfg";             // 计算（也可 `{ get { .. } }` / `{ get => e; }`）
    public static int Seed { get; }                 // get-only：仅本类静态 ctor 内可写
    static Config() { Seed = 42; }

    public static int Next() { Level++; return Level + Seed; }   // 类内裸名 ≡ Config.Level
}
Config.Level += 3;                                  // 限定名读 / 写 / 复合 / 自增
```

- **后备**：静态 auto 属性合成**静态**字段 `__prop_X`，访问器是 0 参 / 1 参静态函数。
- **赋值边界（E0452）**：get-only auto 仅**本类静态 ctor** 内可写（实例 ctor 不行——那是实例初始化）；
  计算属性任何位置不可写；复合赋值 / `++` 同样受约束。
- **静态计算 getter 体是静态上下文**：`this` 与实例字段不可用。
- **初始化器**：无静态 ctor 时并入编译单元的静态初始化；有静态 ctor 注入其体首（与静态字段初始化器
  同一机制，见 [静态构造函数](static-constructors.md)）。
- **跨包**可读写；导入类拿不到后备名，给只读静态属性赋值的 E0452 统一是「it has no setter」措辞。

### extern 属性（`[Native(...)]`，仅 `{ get; }`）

`extern` 属性把一个属性读取直接绑到 VM intrinsic 上，**没有后备字段**，形态固定为只读：

```z42
public class String {
    [Native("__str_length")]
    public extern int Length { get; }       // 读 s.Length → 调 __str_length
}
```

- 降解成 `extern int get_Length();`，由 `[Native("...")]` 指定的 intrinsic 实现。
- ⚠️ **extern 属性只支持 `{ get; }`**。写了 `set;` 语义层照常登记 `set_X` 符号（类型检查因此**不报错**），
  但发射端只有 getter 分支——**不会产出任何 setter**，写入在运行期找不到该函数。
  即 extern 属性上的 `set;` 是**静默不生效**的，不要写。
- 主要用于 stdlib 把 VM 能力包装成属性面（如 `String.Length`、`Type.Visibility`）。

### 接口属性

```z42
interface IShape {
    int Area { get; }
    string Name { get; set; }
}
```

接口属性只声明访问器、无体，降解为方法签名（无后备字段）：

- `{ get; }` → 要求实现类提供 `get_Area`（如 `IEnumerator<T>.Current` → `get_Current`）。
- **`{ get; set; }` 的接口属性同时要求实现类提供 `get_X` 和 `set_X`**——少一个即未实现该接口。
- 实现类既可用 auto / 计算属性满足（合成的正是这两个方法），也可**手写** `get_X` / `set_X` 方法，
  两者等价。

### 降解与命名约定

自动属性 `T Name { get; set; }` 在类上合成（镜像 C# `SynthesizeClassAutoProp`）：

- 私有后备字段 `__prop_Name`（源名 `Name` 不是真实字段，只在类型检查时视作字段）；
- `T get_Name()`（有 `get` 时）、`void set_Name(T value)`（有 `set` 时）。

使用点 `obj.Name` / `obj.Name = v` **会被编译为 `get_Name()` / `set_Name(v)` 实例虚调用**——
包括**类内裸名 `Name`**，它与 `this.Name` 完全同义。只读属性在 ctor 内的写入（无 `set_Name`）
落到后备字段。**任何情况下都不会按源名 `Name` 读写字段**（那是不存在的字段）。

计算属性、表达式体属性同样派发 `get_Name()`；只是没有 `__prop_Name` 后备字段，读取每次都跑
getter 体。

> 绑定期与发射期怎么分工、这条不变量历史上的四个漏口，见
> 「源名 ↔ 后备字段名」的落差属编译器实现细节，本手册不展开。

---

## 索引器（indexer）

索引器让类型支持 `obj[i]` 下标语法。与属性不同，**索引器支持完整的自定义 `get`/`set` 体**。

### 声明语法

```z42
public class Matrix {
    int[] data;
    int cols;

    public Matrix(int rows, int cols) {
        this.cols = cols;
        this.data = new int[rows * cols];
    }

    // this 后接方括号参数列表 + get/set 访问器体
    public int this[int r, int c] {
        get { return this.data[r * this.cols + c]; }
        set { this.data[r * this.cols + c] = value; }
    }
}
```

- 参数个数任意（单维 `this[int i]`、多维 `this[int r, int c]`、更多）。
- 键类型任意（`int` / `string` / 用户类型 …）；`this[string k]` 即字典式索引。
- **表达式体**：只读索引器可写 `T this[int i] => e;`；访问器也可写 `get => e;` /
  `set => this.d[i] = value;`（set 脱糖成 `{ e; }`）。
- `get` 体返回索引器类型；`set` 体内用 `value` 引用被写入的值。
- 泛型类可声明泛型返回类型的索引器（如 `T this[int i]`）。

### 降解：get_Item / set_Item

| 声明 | 合成方法 | 参数 |
|------|---------|------|
| `T this[P0, ... Pn] { get; }` | `T get_Item(P0, ... Pn)` | N 个下标 |
| `T this[P0, ... Pn] { set; }` | `void set_Item(P0, ... Pn, T value)` | N 个下标 + value |

符号收集、体绑定、codegen、跨包导出都按这两个方法名处理，因此手写 `get_Item` / `set_Item`
与声明索引器等价。

### 使用侧派发

下标表达式按接收者的**静态类型**路由：

```
obj[a, b]        （读，obj 是含 get_Item 的类）  → get_Item(a, b)        实例虚调用
obj[a, b] = v    （写，obj 是含 set_Item 的类）  → set_Item(a, b, v)     实例虚调用
arr[i]           （arr 是数组）                  → 原生数组下标（非索引器）
```

多维使用侧 `obj[a, b]`（逗号分隔多下标）下标个数与索引器声明的参数个数天然匹配。

### 接口索引器

接口可声明索引器（accessor-only，无体）；经**接口静态类型**的下标访问派发到实现类的
`get_Item` / `set_Item`：

```z42
interface IBox {
    int this[int i] { get; }          // 接口索引器：只声明访问器、无体
}
IBox b = new ArrBox(...);
int v = b[0];                         // → b.get_Item(0)，虚派发到实现类
```

返回位的 `Self` 换成接口自身（与接口方法返回 `Self` 同口径）；类型参数在运行期经依赖索引派发。

### 约束与边界

- **一个类一个索引器**：`get_Item` / `set_Item` 按名唯一，不支持同类多个 `this[...]` 重载
  （按键类型 / 元数区分的索引器重载尚未实现）。
- **数组是单维**：z42 数组是单维 jagged，`arr[i]` 走原生下标；多维下标 `arr[i, j]` 报
  **E0402**——多维请用 jagged `arr[i][j]`，多维数组 `int[,]` 未支持。
- **非数组、非索引器类型下标** → **E0402**（`index on non-array`）。

---

## 属性 vs 索引器 一览

| 维度 | 属性 | 索引器 |
|------|------|--------|
| 访问语法 | `obj.Name` | `obj[i]` / `obj[a, b]` |
| 命名 | 每个属性独立名 `X` | 固定 `Item`（一类唯一） |
| 参数 | 无 | 1..N 个下标 |
| 访问器体 | auto（`get;`/`set;`）；计算 getter `get {...}` / `get => e;` / `T X => e;`（get-only，无计算 set） | 自定义 `get`/`set`，块体或 `=> e;`；只读可 `T this[..] => e;` |
| 后备字段 | auto 合成 `__prop_X`；计算 getter 无；extern 无 | 无（体自行管理存储） |
| 降解成 | `get_X` / `set_X` | `get_Item` / `set_Item` |
| 访问器级可见性 | 解析但忽略（统一属性级） | 解析但忽略（统一索引器级） |
| extern（`[Native]`） | 仅 `{ get; }` | 不支持 |

---

## 相关文档

- [访问权限控制](access-control.md)——默认可见性、修饰符四级
- [静态构造函数](static-constructors.md)——静态属性初始化器的执行时机
- [错误码全量表](../appendix/error-codes.md)——E0402 / E0452 等
- 测试：`src/tests/classes/auto_property.z42`、`src/tests/classes/static_properties.z42`、
  `src/tests/classes/property_initializers.z42`、`src/tests/types/computed_property.z42`、
  `src/tests/types/expression_bodied_members.z42`、`src/tests/classes/indexer_basic.z42`、
  `src/tests/classes/indexer_multidim.z42`、`src/tests/interfaces/interface_indexer.z42`、
  `src/tests/cross-zpkg/static_property_cross_pkg`
