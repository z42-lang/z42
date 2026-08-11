# 属性与索引器（成员访问器）

> 对齐日期：2026-08-11 · 索引器多维使用侧：change `add-multidim-indexer`（2026-08-11）

成员访问器让字段式 / 下标式语法背后跑用户逻辑，语义与 C# 一致：

| 访问器 | 声明 | 使用 | lower 成 |
|--------|------|------|---------|
| **属性（property）** | `T Name { get; set; }` | `obj.Name` / `obj.Name = v` | `get_Name()` / `set_Name(v)` + 后备字段 `__prop_Name` |
| **索引器（indexer）** | `T this[P...] { get {...} set {...} }` | `obj[i]` / `obj[i] = v` | `get_Item(...)` / `set_Item(..., v)` |

两者都在编译期 lower 成普通实例方法（镜像 C# 的 `get_X`/`set_X`、`get_Item`/`set_Item`），
因此天然支持虚派发与跨 zpkg 调用。

---

## 属性（property）

### 自动属性（auto-property）

z42 属性目前是**自动属性**：访问器只写 `get;` / `set;`（分号结尾），编译器合成后备字段与
访问器方法。**不支持自定义 `get {...}` 访问器体**（延后特性）。

```z42
class Person {
    public string Name { get; set; }     // 读写
    public int Age { get; private set; } // 读公开、写私有（per-accessor 可见性）
    public bool Active { get; }          // 只读（只有 getter）
}
```

- **per-accessor 可见性**：`get` / `set` 前可各自加可见性修饰（如 `private set`）。
- **只读属性**：只写 `{ get; }`。
- **初始化器**：`T Name { get; set; } = expr;` 给后备字段一个初值。

```z42
public int Count { get; set; } = 0;
```

### 机制：后备字段 + get_X / set_X

自动属性 `T Name { get; set; }` 在类上合成（镜像 C# `SynthesizeClassAutoProp`）：

- 私有后备字段 `__prop_Name`（源名 `Name` 不作真实字段，仅供类型检查视作字段）。
- `T get_Name()`（有 `get` 时）、`void set_Name(T value)`（有 `set` 时）。
- 使用点 `obj.Name` / `obj.Name = v` 由 `MemberResolver` 绑定为 `get_Name()` / `set_Name(v)`
  实例虚调用（导入类只有 `__prop_Name` + `get_/set_`，没有裸 `Name` 字段）。
- **接口属性** `T Name { get; }` → 要求实现类提供 `get_Name`（如 `IEnumerator<T>.Current`
  → `get_Current`）。

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
- `get` 体返回索引器类型；`set` 体内用 `value` 引用被写入的值。
- 泛型类可声明泛型返回类型的索引器（如 `T this[int i]`）。

### 机制：get_Item / set_Item

| 声明 | 合成方法 | 参数 |
|------|---------|------|
| `T this[P0, ... Pn] { get; }` | `T get_Item(P0, ... Pn)` | N 个下标 |
| `T this[P0, ... Pn] { set; }` | `void set_Item(P0, ... Pn, T value)` | N 个下标 + value |

符号收集、体绑定、codegen、跨包导出都按这两个方法名处理。

### 使用侧派发

`ExprTyper` 按接收者类型路由 `IndexExpr`：

```
obj[a, b]        （读，obj 是含 get_Item 的类）  → get_Item(a, b)        实例虚调用
obj[a, b] = v    （写，obj 是含 set_Item 的类）  → set_Item(a, b, v)     实例虚调用
arr[i]           （arr 是数组）                  → BoundIndex（原生数组下标，非索引器）
```

对应 IR：

```
%r = vcall %obj.get_Item(%a, %b)          // obj[a, b]
     vcall %obj.set_Item(%a, %b, %v)      // obj[a, b] = v
```

多维使用侧 `obj[a, b]`（逗号分隔多下标）由 `ExprParser` 的后缀 `[` 分支循环解析下标存入
`IndexExpr.Indices`；下标个数与索引器声明的参数个数天然匹配。

### 约束与边界

- **一个类一个索引器**：`get_Item` / `set_Item` 按名唯一，不支持同类多个 `this[...]` 重载
  （按键类型/元数区分的索引器重载尚未实现）。
- **数组是单维**：z42 数组是单维 jagged，`arr[i]` 走原生下标；多维下标 `arr[i, j]` 报
  `E0402`——多维请用 jagged `arr[i][j]`，多维数组 `int[,]` 未支持。
- **非数组、非索引器类型下标** → `E0402`（`index on non-array`）。

---

## 属性 vs 索引器 一览

| 维度 | 属性 | 索引器 |
|------|------|--------|
| 访问语法 | `obj.Name` | `obj[i]` / `obj[a, b]` |
| 命名 | 每个属性独立名 `X` | 固定 `Item`（一类唯一） |
| 参数 | 无 | 1..N 个下标 |
| 访问器体 | 仅 auto（`get;`/`set;`） | 支持自定义 `get {...}` / `set {...}` |
| 后备字段 | 合成 `__prop_X` | 无（体自行管理存储） |
| lower 成 | `get_X` / `set_X` | `get_Item` / `set_Item` |

---

## 相关文档

- 索引器多维使用侧引入：change `add-multidim-indexer`（`docs/spec/archive/2026-08-11-add-multidim-indexer`）
- 编译器错误码：[错误码体系](../compiler/error-codes.md)
- 示例：`examples/indexer.z42`（单维 string 键 + 多维矩阵）、`examples/oop.z42`（接口属性）
- 测试：`src/tests/classes/auto_property.z42`（属性）、`indexer_basic.z42`（单维泛型索引器）、
  `indexer_multidim.z42`（多维索引器）
