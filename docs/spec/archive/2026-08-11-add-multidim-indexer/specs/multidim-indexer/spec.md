# Spec: 多维索引器使用侧

## ADDED Requirements

### Requirement: 多维索引器读

#### Scenario: 双下标读派发 get_Item
- **WHEN** 类声明 `int this[int r, int c] { get {...} }`，源码写 `m[r, c]`（读）
- **THEN** 解析为携带 2 个下标的 `IndexExpr`，绑定为 `get_Item(r, c)` 实例虚调用（2 实参）

#### Scenario: 三下标读
- **WHEN** 类声明 `T this[int a, int b, int c]`，源码写 `x[a, b, c]`
- **THEN** 绑定为 `get_Item(a, b, c)`（3 实参）

### Requirement: 多维索引器写

#### Scenario: 双下标写派发 set_Item
- **WHEN** 类声明含 `set`，源码写 `m[r, c] = v`
- **THEN** 绑定为 `set_Item(r, c, v)` 实例虚调用（下标 2 个 + value 共 3 实参）

## MODIFIED Requirements

### Requirement: 索引表达式下标载荷

**Before:** `IndexExpr` 携带单个 `Expr Index`；`ExprParser` 后缀 `[` 只解析一个下标就 expect `]`，
`m[a, b]` 在逗号处报 `E0202: expected ']'`。

**After:** `IndexExpr` 携带 `Expr[] Indices` + `int IndexCount`；后缀 `[` 循环解析逗号分隔下标。
单下标 `v[i]` / `a[i]` / `e["k"]` 行为不变（`IndexCount == 1`）。

## 边界 / 回归

#### Scenario: 单维索引器与数组保持不变
- **WHEN** `v[i]`（单维索引器）/ `a[i]`（数组）/ `e["k"]`（string 键索引器）
- **THEN** 行为与改动前逐字节一致（`get_Item(i)` / 数组 `BoundIndex` / `get_Item("k")`）

#### Scenario: 数组多下标报错
- **WHEN** `a` 是数组类型，源码写 `a[i, j]`
- **THEN** 报诊断（数组是单维，不支持多维下标），不 panic、不误派发

## IR Mapping

- `m[r, c]` 读 → `BoundCall("instance", virtual, recv, Cls, "get_Item", [r, c], 2)` → `vcall %recv.get_Item(r, c)`
- `m[r, c] = v` 写 → `BoundCall("instance", virtual, recv, Cls, "set_Item", [r, c, v], 3)` → `vcall %recv.set_Item(r, c, v)`
- 数组 `a[i]` 读 → `BoundIndex`（不变）

## Pipeline Steps

- [ ] Lexer —— 无改动（`[` `,` `]` 皆既有 token）
- [x] Parser / AST —— `IndexExpr` 载荷 + 多下标解析
- [x] TypeChecker —— 读/写路由传 N 下标
- [ ] IR Codegen —— 复用 `BoundCall`（get_Item/set_Item lowering 已就绪，无改动）
- [ ] VM interp —— 无改动（vcall 既有）
