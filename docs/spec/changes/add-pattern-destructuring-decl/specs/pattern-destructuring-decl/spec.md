# Spec: 解构声明（模式匹配 B）

## 语法

```
DeconstructDeclStmt := <PositionalPattern> '=' <Expr> ';'
PositionalPattern   := <Type> '(' <Pattern> ( ',' <Pattern> )* ')'
```

解析消歧：语句起始为 `Identifier`，跳过类型名前缀后紧跟 `(`，配平括号后随 `=` → 解构声明；否则回落
表达式语句（函数调用后随 `;`）。

## 语义

`T(p_0, ..., p_{n-1}) = e;`：

1. 求值 `e`（静态类型须为 record class `T`，精确匹配）。
2. 按 `T` 主构造器声明序，第 i 个子模式 `p_i` 绑定第 i 个字段 `T.OwnFieldNames[i]` 的值。
3. 子模式绑定的变量在**后续语句**可见（当前块作用域，同普通局部声明）。

## 不可失败（irrefutable）约束

解构声明无失败分支，编译期强制模式不可失败：

- **结构**：子模式仅 `_`（通配）/ 裸标识符（绑定）/ 嵌套 `PositionalPattern`。含常量 / or `|` / 范围 `..=`
  / 关系 `>0` / 类型测试 `T x` → 编译错误（`must be irrefutable`）。
- **类型精确匹配**：每层位置模式的类型须与该位置被解构值的静态类型完全一致（顶层=`e` 静态类型，
  嵌套=父 record 字段类型）→ 编译错误（`static type to match the pattern exactly`）不满足时。
- arity 不符（子模式数 ≠ 字段数）→ 编译错误。

## 限制

- 仅 record **class**（`IsRecord && !IsStruct`）；struct record → 报错（未支持）。
- 仅位置形态；属性形态 `T { F: p } = e`、泛型 record、元组模式为后续特性。

## 示例

```z42
[Record] class Point(int X, int Y);
[Record] class Line(Point A, Point B);

Point p = new Point(3, 4);
Point(x, y) = p;                             // x=3, y=4
Point(_, b) = p;                             // b=4（通配丢弃 X）
Line(Point(ax, ay), Point(bx, by)) = seg;    // 嵌套

// 非法（编译错误）：
// Point(0, y) = p;      // 常量子模式：可失败
// Circle(r) = shapeVal; // 类型不精确匹配（shapeVal: Shape 基类）
```
