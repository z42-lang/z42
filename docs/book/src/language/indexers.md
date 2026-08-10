# 索引器（indexer）

> 对齐日期：2026-08-11 · 引入：早期特性；多维使用侧由 change `add-multidim-indexer`（2026-08-11）补齐

索引器让用户自定义类型支持 `obj[i]` 下标语法，语义与 C# 一致：一个类可声明一个
`this[...]` 成员，读 `obj[i]` 与写 `obj[i] = v` 分别派发到合成的访问器方法。

## 声明语法

```z42
public class Matrix {
    int[] data;
    int cols;

    public Matrix(int rows, int cols) {
        this.cols = cols;
        this.data = new int[rows * cols];
    }

    // 索引器：this 后接方括号参数列表 + get/set 访问器
    public int this[int r, int c] {
        get { return this.data[r * this.cols + c]; }
        set { this.data[r * this.cols + c] = value; }
    }
}
```

- 参数个数任意（单维 `this[int i]`、多维 `this[int r, int c]`、更多）。
- 键类型任意（`int` / `string` / 用户类型 …）；`this[string k]` 即字典式索引。
- `get` 访问器返回索引器声明的类型；`set` 访问器内用 `value` 引用被写入的值。
- 泛型类可声明泛型返回类型的索引器（如 `T this[int i]`，见 `MyList<T>`）。

## 机制：lower 为 get_Item / set_Item

索引器在编译期 lower 成两个普通实例方法（镜像 C# 的 `get_Item` / `set_Item`）：

| 声明 | 合成方法 | 参数 |
|------|---------|------|
| `T this[P0 p0, ... Pn pn] { get; }` | `T get_Item(P0, ... Pn)` | N 个下标 |
| `T this[P0 p0, ... Pn pn] { set; }` | `void set_Item(P0, ... Pn, T value)` | N 个下标 + value |

符号收集（`SymbolCollector`）、体绑定（`DeclBinder`）、codegen（`IrGen`）、跨包导出
（`ExportedTypeExtractor`）都按这两个方法名处理，因此索引器天然支持跨 zpkg 调用与虚派发。

## 使用侧派发

`ExprTyper` 在绑定 `IndexExpr` 时按接收者类型路由：

```
obj[a, b]        （读，obj 是含 get_Item 的类）  → get_Item(a, b)        实例虚调用
obj[a, b] = v    （写，obj 是含 set_Item 的类）  → set_Item(a, b, v)     实例虚调用
arr[i]           （读/写，arr 是数组）           → BoundIndex（原生数组下标，非索引器）
```

对应 IR：

```
%r = vcall %obj.get_Item(%a, %b)          // obj[a, b]
     vcall %obj.set_Item(%a, %b, %v)      // obj[a, b] = v
```

多维使用侧 `obj[a, b]`（逗号分隔多下标）由 `ExprParser` 的后缀 `[` 分支循环解析下标存入
`IndexExpr.Indices`；下标个数与索引器声明的参数个数天然匹配。

## 约束与边界

- **一个类一个索引器**：`get_Item` / `set_Item` 按名唯一，不支持同类多个 `this[...]` 重载
  （按键类型/元数区分的索引器重载尚未实现）。
- **数组是单维**：z42 数组是单维 jagged，`arr[i]` 走原生下标；多维下标 `arr[i, j]` 报
  `E0402`（`array does not support multi-dimensional index`）——多维请用 jagged `arr[i][j]`，
  多维数组 `int[,]` 未支持。
- **非数组、非索引器类型下标** → `E0402`（`index on non-array`）。

## 相关文档

- 设计/引入：change `add-multidim-indexer`（`docs/spec/archive/…-add-multidim-indexer`）——多维使用侧补齐
- 编译器错误码：[错误码体系](../compiler/error-codes.md)
- 示例：`examples/indexer.z42`（单维 string 键 + 多维矩阵）
- 测试：`src/tests/classes/indexer_basic.z42`（单维泛型）、`indexer_multidim.z42`（多维）
