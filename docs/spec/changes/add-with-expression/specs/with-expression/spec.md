# Spec: `with` 表达式（模式匹配 D）

## 语法

```
WithExpr := <Expr> 'with' '{' WithField ( ',' WithField )* ','? '}'
WithField := <Ident> '=' <Expr>       // 显式覆盖
           | <Ident>                   // 简写：x ≡ x = x
```

`with` 是**后缀表达式**，绑定力 85（同 `switch` 后缀）。可链式：`p with { X=1 } with { Y=2 }`。

## 语义

`target with { F_i = v_i, ... }`：

1. `target` 静态类型须为 **record class**（`IsRecord && !IsStruct`）。
2. 产出一个**新 record**：按主构造器声明序，每个字段——若在覆盖集中 → 用覆盖值 `v_i`；否则 → 拷贝 `target`
   对应字段的原值。
3. `target` 求值**一次**（绑临时），非破坏式——原对象不变。

等价脱糖：`target with { Y = 99 }` ≡ `{ $t = target; new T($t.F0, ..., /*Y*/99, ...) }`（读原字段作主构造器
实参、覆盖项替换）——避 record 字段 readonly/init-only 的赋值限制。

## 约束

- 非 record 类型 → 编译错误（`with requires a record type`）。
- 覆盖字段名非该 record 主构造器字段 → 编译错误（`no field ... to update with with`）。
- 仅 record **class**；struct record → 报错（未支持）。
- `..base` 结构更新 → 报错（未支持）。

## 示例

```z42
[Record] class Point(int X, int Y);
[Record] class Box3(int A, int B, int C);

Point p = new Point(1, 2);
Point q = p with { Y = 99 };              // Point(1, 99)，p 不变
Point r = p with { X = p.X + 100 };       // 覆盖值可为表达式
Box3 b2 = new Box3(1, 2, 3) with { A = 10, C = 30 };   // 多字段，B 保留 2

int Y = 55;
Point s = p with { Y };                    // 简写：Point(1, 55)

// 非法：
// Plain x = plainObj with { F = 1 };       // 非 record
// Point z = p with { W = 1 };               // W 非字段
```
