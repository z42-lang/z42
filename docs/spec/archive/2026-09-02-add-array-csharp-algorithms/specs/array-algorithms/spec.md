# Spec: Std.Array C# 静态算法

## ADDED Requirements

### Requirement: 谓词查找

#### Scenario: Exists / TrueForAll
- **WHEN** `Array.Exists<int>(xs, p)`，xs 中存在满足 p 的元素
- **THEN** 返回 true；无满足则 false；空数组 Exists→false、TrueForAll→true（vacuous）

#### Scenario: Find / FindLast
- **WHEN** `Array.Find<T>(xs, p)` 有匹配
- **THEN** 返回第一个匹配元素；`FindLast` 返回最后一个；无匹配返回 `default(T)`

#### Scenario: FindIndex / FindLastIndex
- **WHEN** 有匹配
- **THEN** 返回第一个 / 最后一个匹配下标；无匹配返回 -1

#### Scenario: FindAll
- **WHEN** `Array.FindAll<T>(xs, p)`
- **THEN** 返回按原序、精确长度、仅含匹配元素的新数组；无匹配返回零长数组

### Requirement: 查找

#### Scenario: BinarySearch 命中 / 未命中
- **WHEN** 数组按 `CompareTo` 升序，`Array.BinarySearch<T>(xs, v)`
- **THEN** 命中返回其下标；未命中返回 `~插入点`（`< 0`，`~返回值` = 保持有序的插入位置）

#### Scenario: Contains / LastIndexOf
- **WHEN** `Array.Contains<T>(xs, v)` / `Array.LastIndexOf<T>(xs, v)`
- **THEN** Contains 存在→true 否则 false；LastIndexOf 返回最后一次出现下标，无则 -1

### Requirement: 变换与工具

#### Scenario: ConvertAll
- **WHEN** `Array.ConvertAll<T,U>(xs, f)`
- **THEN** 返回等长新数组，元素为 `f(xs[i])`；空数组返回零长

#### Scenario: ForEach
- **WHEN** `Array.ForEach<T>(xs, a)`
- **THEN** 对每个元素按序调用 `a(xs[i])`

#### Scenario: Clear 正常 / 越界
- **WHEN** `Array.Clear<T>(xs, index, length)` 范围合法
- **THEN** `xs[index..index+length)` 全置 `default(T)`，其余不变
- **WHEN** `index<0 || length<0 || index+length > xs.Length`
- **THEN** 抛 `Exception`

#### Scenario: Resize 增 / 减 / 等长
- **WHEN** `Array.Resize<T>(xs, n)`
- **THEN** 返回长度 n 的新数组：n>原长时前段拷贝、尾部 `default(T)`；n<原长时截断；n==原长时内容相同的新数组
- **AND** 原数组 xs 不被修改（返回新数组语义）

#### Scenario: Empty
- **WHEN** `Array.Empty<T>()`
- **THEN** 返回长度 0 的 `T[]`

## IR Mapping
无（纯脚本，复用既有数组索引 / `array_len` / `array_new` 与委托调用）。

## Pipeline Steps
- [ ] Lexer（无）
- [ ] Parser / AST（无）
- [ ] TypeChecker（无——泛型静态方法既有能力）
- [ ] IR Codegen（无）
- [ ] VM interp（无）
