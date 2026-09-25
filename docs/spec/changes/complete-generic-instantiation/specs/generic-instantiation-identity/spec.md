# Spec: 泛型实例化的单调化闭包

## MODIFIED Requirements

### Requirement: 操作实例化的泛型代码必须按实例化布局编译

**Before:** 泛型体（泛型自由函数 / 泛型方法 / 泛型类型的成员）**只编一份**，按**擦除布局**
烘焙字节偏移；而 #774 让实例化**类型**拿到自己的布局。同一批字节两种理解 ⇒ **静默错值**。

**After:** 凡是以具体实参操作某实例化的泛型体，都按**该实例化的布局**各特化一份。
布局与擦除布局相同的实参组合不特化（共享擦除体即正确，且产物逐字节不变）。

#### Scenario: 泛型函数读实例化的字段（S1 驱动用例）
- **WHEN** `[Record] struct Loc<A,B>(A Item1, B Item2);`
  `int ReadSecond<T>(Loc<T,int> p) { return p.Item2; }`
  `Loc<P2,int> t = new Loc<P2,int>(a, 7);`（`P2 = { int X; long Y; }`）
- **THEN** `ReadSecond<P2>(t) == 7`（今天读到 **2** —— 按擦除布局在 @8 读，那是 `P2.Y`）

#### Scenario: 泛型函数写实例化的字段
- **WHEN** 泛型函数以 `ref` 或返回值写回某实例化的字段
- **THEN** 写入位置与调用方直接访问同一字段的位置一致

#### Scenario: 泛型方法（实例方法）上的同一形态
- **WHEN** 泛型类型的实例方法以具体实参操作自身的型参字段
- **THEN** 与调用方的直接访问一致

#### Scenario: 闭包传递
- **WHEN** 被特化的泛型体内部又构造/访问另一个实例化（含嵌套 `G<G<A,B>,C>`）
- **THEN** 内层实例化及其相关泛型体同样被特化（不动点），链式访问逐层正确

#### Scenario: 布局相同则不特化（阴性对照）
- **WHEN** 实参组合使实例化布局与擦除布局逐项相同
- **THEN** 不产生特化体，编译产物与改动前**逐字节相同**

#### Scenario: 非泛型代码不受影响（阴性对照）
- **WHEN** 程序不含任何泛型实例化
- **THEN** 编译产物与改动前**逐字节相同**

### Requirement: 跨包实例化的表示一致（S2）

**Before:** 跨包实例化两侧都不特化（都用擦除布局）—— 自洽但值语义错（型参槽按句柄存，
外层复制只浅拷句柄）。

**After:** 泛型定义以**布局无关模板**随包投送；消费方按实例化布局烘焙偏移后发射，
生产方侧以具体实参操作该实例化的泛型体同样特化。

#### Scenario: 元组的值语义
- **WHEN** `(P2,int) t = (a, 7); a.Y = 99L;`
- **THEN** `t.Item1.Y == 2L`（今天读到 99）

#### Scenario: 库内部构造、消费方读取
- **WHEN** `Dictionary<string,int>.Entries()`（体在 z42.core）返回 `KeyValuePair<string,int>[]`，
  消费方遍历求和
- **THEN** 和正确（今天 `dict_iter` 的 `sum3` 读到 **0**，应为 6）

### Requirement: 合成实例化产物的重复到达不是歧义（S2 先决条件，已实施）

同一实例化会被**每个用到它的包**各合成一份描述符与成员。两份是 `(定义, 类型实参)` 的
确定性函数 ⇒ 第二份到达应被静默跳过，而非记为「两个包声明了同一个名字」。

#### Scenario: 库与主程序各用一次同一实例化
- **WHEN** 库 `X` 内部使用某实例化，主程序也使用它
- **THEN** 构造与调用均不抛；stderr 无 `duplicate type` / `duplicate function` 告警

#### Scenario: 真正的用户声明重复仍然报（阴性对照）
- **WHEN** 两个包各声明同一个 FQN 的**非泛型**类型，消费方引用它
- **THEN** `E0601` 照报、运行期歧义行为照旧

## IR Mapping

**S1 不新增任何 IR 指令、不改格式。** 特化体与被特化的泛型体用同一套指令，差别只在
`struct_fget_prim` / `struct_fset_prim` 烘焙的**偏移**与 `struct_alloc` 的**类型名/大小**。

**S2** 需要一段承载**布局无关模板**的新载荷：体内凡 owner 布局依赖型参的 struct 访问，
以**字段名**而非偏移表达，由消费方按实例化布局烘焙。⇒ 格式 bump（zpkg minor），
按 `bootstrap-seed.md` 的「support 先行、晚一个 nightly 再 use」分阶段引入。

## Pipeline Steps

- [ ] Lexer — 无
- [ ] Parser / AST — 无
- [x] TypeChecker — S1：确定泛型调用的具体实参（已有信息，需在发射侧可达）
- [x] IR Codegen — S1：工作表扩成两类工作项 + 特化名单一出口；S2：模板烘焙
- [x] VM interp — S2 先决条件 D4-fix（惰性加载器的重复登记判定）已实施；S1 **零改动**
