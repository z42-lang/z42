# Spec: struct 泛型容器装箱（P3a）

## ADDED Requirements

### Requirement: blob 值 struct 可作泛型容器键/值/元素（装箱进容器）

struct 流入泛型 type-param（`K`/`V`/`T`）且存进容器堆存储时装箱（`__box_struct`），取出到具体 struct 类型时
拆箱（`AsCast`→arena StructRef）；容器内部按 boxed 对象协议（PR2b）算 hash/equals。

#### Scenario: Dictionary<P,int> 按值键存取
- **WHEN** `var d=new Dictionary<P,int>(); d[new P(1,2)]=5; d[new P(3,4)]=9;`
- **THEN** `d[new P(1,2)]==5` 且 `d[new P(3,4)]==9`（同值键命中，靠装箱 + boxed GetHashCode/Equals）

#### Scenario: Dictionary ContainsKey / 覆盖
- **WHEN** `d.ContainsKey(new P(1,2))` / `d.ContainsKey(new P(9,9))` / `d[new P(1,2)]=7`
- **THEN** `true` / `false` / `d[new P(1,2)]==7`（同值键更新同槽）

#### Scenario: List<P> 增/索引/遍历/查找
- **WHEN** `var l=new List<P>(); l.Add(new P(1,2)); l.Add(new P(3,4));`
- **THEN** `l[0]` 拆箱得值 `P(1,2)`（`l[0].x==1`）；`foreach(P p in l)` 得值 struct；`l.Contains(new P(3,4))`→true

#### Scenario: 取出值独立（值语义）
- **WHEN** `P p = l[0]; p.x = 99;`
- **THEN** 容器内元素不变（`l[0].x==1`）——拆箱是拷贝到当前帧 arena

#### Scenario: 含 string 叶子 struct 作键
- **WHEN** `Dictionary<Tagged,int>`（`Tagged{int n; string label;}`），同内容不同 string 实例的键
- **THEN** 命中同槽（boxed hash 混 string 内容 + Equals string 内容相等）

#### Scenario: 非 struct 泛型不受影响
- **WHEN** `Dictionary<string,int>` / `List<int>` / `Dictionary<object,V>`
- **THEN** 行为不变（type-param 目标对基元/引用类型的装箱由既有 `_emitBox`/`BoxIfNeeded` 决定，blob-struct 判据不误伤）

## MODIFIED Requirements

### Requirement: struct → 泛型 type-param 的传参/存储

**Before:** struct 实参传给 type-param `K`/`T` 不装箱 → 裸 `StructRef` 流入容器 → `key.GetHashCode()` VCall
崩 + 堆数组存帧作用域句柄悬垂。

**After:** `BoxIfNeeded` 对 type-param 目标的 blob struct 装箱；`d[key]=v` indexer-set 亦装箱；容器存
`BoxedStruct`（堆稳定）；取出到具体 struct 类型时拆箱回 arena StructRef。

## IR Mapping

- 装箱：`__box_struct`（Builtin opcode，PR2a）；拆箱：`AsCast`（PR2a `_emitConvert` 臂）。**无新 opcode、无
  zbc/zpkg 格式 bump、容器 ABI 不变**。

## Pipeline Steps

- [ ] Lexer / Parser / AST —— 无
- [x] TypeChecker —— `BoxIfNeeded` type-param 目标 + `UnboxIfNeeded`（新）+ `_bindAssign` set_Item 装箱 + `_bindIndex` get_Item 拆箱
- [x] IR Codegen —— `FunctionEmitter` foreach 拆箱；复用 `_emitBox`/`_emitConvert`
- [x] VM interp / JIT —— 无改动（复用 boxed-vcall 臂 + `unbox_struct`）
