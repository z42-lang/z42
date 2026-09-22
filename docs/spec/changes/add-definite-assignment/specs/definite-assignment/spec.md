# Spec: definite assignment

## ADDED Requirements

### Requirement: 读未赋值的局部变量报 E0407

#### Scenario: 声明后直接读
- **WHEN** `int x; Console.WriteLine(x);`
- **THEN** 报 `E0407`
- **注**：此前**编译通过**，运行期读到 Null

#### Scenario: 赋值后读合法
- **WHEN** `int x; x = 1; Console.WriteLine(x);`
- **THEN** 编译通过

#### Scenario: 带初始化器
- **WHEN** `int x = 0; Console.WriteLine(x);`
- **THEN** 编译通过

#### Scenario: 形参恒为已赋值
- **WHEN** `void F(int a) { Console.WriteLine(a); }`
- **THEN** 编译通过

#### Scenario: 被 `ref` 取址的局部恒为已赋值
- **WHEN** `int v; Inc(ref v); Console.WriteLine(v);`
- **THEN** 编译通过 —— 取址点在声明处回填了 `default(T)`（simplify-ref-parameters）

### Requirement: if/else 的合并规则

#### Scenario: 两支都赋值
- **WHEN** `int x; if (c) { x = 1; } else { x = 2; } print(x);`
- **THEN** 编译通过（交集非空）

#### Scenario: 只有一支赋值
- **WHEN** `int x; if (c) { x = 1; } print(x);`
- **THEN** 报 `E0407` —— 条件假时会落下来

#### Scenario: 另一支必定 return
- **WHEN** `int x; if (c) { x = 1; } else { return; } print(x);`
- **THEN** 编译通过 —— else 必定退出，只有 then 的路径能落到 print

#### Scenario: 另一支必定 throw
- **WHEN** `int x; if (c) { x = 1; } else { throw new Exception("no"); } print(x);`
- **THEN** 编译通过

### Requirement: 循环的合并规则

#### Scenario: while 体内赋值不算
- **WHEN** `int x; while (c) { x = 1; } print(x);`
- **THEN** 报 `E0407` —— 循环可能零次执行

#### Scenario: do-while 体内赋值算
- **WHEN** `int x; do { x = 1; } while (c); print(x);`
- **THEN** 编译通过 —— 体必执行一次

### Requirement: try/catch 的合并规则

#### Scenario: try 里的赋值在 catch 里不算
- **WHEN** `int x; try { risky(); x = 1; } catch (Exception e) { print(x); }`
- **THEN** 报 `E0407` —— 异常可能在 `x = 1` **之前**抛出
- **注**：这条最容易漏，漏了是**漏报**

#### Scenario: try 与 catch 都赋值
- **WHEN** `int x; try { x = 1; } catch (Exception e) { x = 2; } print(x);`
- **THEN** 编译通过

### Requirement: switch 的合并规则

#### Scenario: 各分支都赋值
- **WHEN** 每个 `case`（含 `default`）都给 `x` 赋值后 `print(x)`
- **THEN** 编译通过

#### Scenario: 有分支未赋值
- **WHEN** 某个 `case` 没给 `x` 赋值
- **THEN** 报 `E0407`

### Requirement: 不误报

#### Scenario: 全仓现存代码
- **WHEN** 对 stdlib + 编译器自身 + 全部测试跑本检查
- **THEN** 每一条命中都必须是**真的未初始化读**；任一误报即说明 join 规则漏了一种结构
- **注**：DA 的价值前提就是误报率接近零，「规则太严」不是可接受的解释
