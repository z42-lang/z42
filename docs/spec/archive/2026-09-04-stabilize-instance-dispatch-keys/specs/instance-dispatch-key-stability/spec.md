# Spec: 实例派发键稳定性约定（overload 键 / 裸规范槽 解耦）

## ADDED Requirements

### Requirement: 方法派发标识拆为「overload 键」与「多态规范槽」两个独立标识

每个方法登记两个标识，各司一职：

```
overloadKey(m) = MangleKey(m.name, m.genericArity, m.paramTypes)      # 全签名纯函数，稳定、区分重载
slotName(m)    = m.name（裸）                                          # 仅「规范槽」方法额外登记

MangleKey(name, gArity, types) =
    name ("$$" gArity)?             # 泛型方法：$$<方法类型参数个数>（0 省略）
         "$" len(types)            # 值参个数
         ("$" TypeKey(types[i]))*  # 逐参类型片段
TypeKey(leaf)      = Canon(leaf.Name())                    # 裸内建叶子 int→i32、剥 nullable
TypeKey(composite) = compositeKeyName(t)                    # array/instantiated/func：构造子用 FQN，内建叶子用 keyword 拼写
type-param 名归一：源 T/U/… → 规范 T0,T1,…（消 alpha-rename 敏感）

规范槽 = 每 (owner, name) 承担多态/协议派发的那个方法：
    virtual origin ∪ 接口/协议方法 ∪ 协议豁免名 ∪ （非虚且同名唯一时）该方法自身
IsProtocolExempt ∈ { ToString, Equals, GetHashCode, GetType, get_Item, set_Item }   # 单一 SoT，编译器与 DepIndex 同集
```

**overload 键取自方法的声明层签名**（类型参数原样、不代入实参）；**规范槽是裸名**（对类型替换不变）。

#### Scenario: 加实例重载不漂移现有方法的键（additive）
- **WHEN** 一个原本唯一的实例方法 `IndexOf(string)`（今日裸 `IndexOf`）新增重载 `IndexOf(char)`
- **THEN** `IndexOf(string)` 的**裸规范槽 `IndexOf` 保留不变**，并新增全键 `IndexOf$1$string`；`IndexOf(char)`
  仅新增全键 `IndexOf$1$char`（非规范槽）；**已编译调用方（含 seed）的裸 `IndexOf` 调用仍命中**，零 rekey

#### Scenario: 已决议的具体重载调用点 emit 全键
- **WHEN** 源写 `s.IndexOf("x")` / `s.IndexOf('x')`（编译期知实参类型、选定具体重载）
- **THEN** 分别 emit 全键 `IndexOf$1$string` / `IndexOf$1$char`；运行期原始/装箱路径**先探全键**命中各自实现

#### Scenario: 多态/协议调用 emit 裸规范槽
- **WHEN** 泛型约束调用 `a.CompareTo(b)`（`where T: IComparable<T>`）或 `a.op_Add(b)`（`where T: INumber<T>`），
  接收者是类型参数、编译期不知具体类型
- **THEN** emit 裸规范槽 `CompareTo` / `op_Add`；运行期按接收者真实类型（对象走 vtable 裸槽、原始走裸候选）
  落到该类型登记在规范槽下的实现——base 声明与原始/派生实现**同槽、对替换不变**

#### Scenario: 泛型-over-原始类型协议方法可达
- **WHEN** `int` 作为 `T: INumber<T>` 参与 `a + b` → VCall `op_Add`；`int` 的 `static override op_Add(i32,i32)`
- **THEN** 不再 `expected object, got I64`——`op_Add` 是 `Std.Int32` 的规范槽裸别名，原始路径裸候选命中

#### Scenario: 泛型方法 arity 进键，不与非泛型同名塌键
- **WHEN** 同类同时有 `Bar()` 与 `Bar<T>()`
- **THEN** 二者键不同（`Bar$0` vs `Bar$$1$0`）→ 都登记不互相覆盖；`Foo<T>(T)` 的键对 alpha-rename（`T`→`U`）不变（归一 `T0`）

#### Scenario: 调用决议——非泛型 ≻ 泛型（tie-break①）
- **WHEN** `Foo(int)` 与 `Foo<T>(T)` 并存，调用 `x.Foo(5)`（无显式类型实参）
- **THEN** 两者都适用且逐参同优时，选**非泛型** `Foo(int)`（C# §12.6.4.3）；emit 其精确键

#### Scenario: 调用决议——显式类型实参只选泛型
- **WHEN** 调用 `x.Foo<int>(5)`
- **THEN** 仅泛型且 genArity==1 的候选参与（非泛型剔除）；TA 代入判适用；emit 泛型声明键 + method_type_args

#### Scenario: 调用决议——无唯一极小 → 报错不静默覆盖
- **WHEN** 候选集在全部 tie-break 后仍无唯一「支配所有」者
- **THEN** 报 E-ambiguous（`OverloadResult` code 2），**禁止**今天「同键塌 `Bar$0` 后者静默覆盖前者」的行为

#### Scenario: composite 键按 FQN 消短名跨-ns 碰撞
- **WHEN** 两个不同命名空间的同短名泛型类型 `A.List<int>` / `B.List<int>` 出现在方法签名
- **THEN** composite 键用 FQN（`A.List<int>` / `B.List<int>`）而非短名 `List<int>`，键不碰撞；跨包 import 侧产出逐字节一致键

#### Scenario: 对象虚重载各占独立 vtable 槽
- **WHEN** 一个类有两个同名虚方法 `virtual F(int)` / `virtual F(string)`（非规范的多虚重载）
- **THEN** vtable 按全键分槽（`derive_simple_method_name` 对非规范虚重载保 `$`），override 按全键匹配，互不覆盖

#### Scenario: 协议豁免名保持裸规范槽
- **WHEN** 方法为 `ToString`/`Equals`/`GetHashCode`/`GetType`/`get_Item`/`set_Item`
- **THEN** 键为裸规范槽（VM `dispatch.rs`/`exec_vcall.rs`/JIT `value.rs` 硬查锚点 + DepIndex 裸名查）；
  编译器 `IsProtocolExempt` 与 DepIndex 协议名单为**同一集**

## MODIFIED Requirements

**Before:** 实例方法键 = 兄弟集的函数（唯一→裸 / 多 arity→`Name$arity` / 同 arity≥2→全签名）；单一字符串键
同时做 overload 决议与多态派发；VM vtable 槽按 `$`-剥离简单名；原始/装箱路径只探裸+`$arity` 候选。

**After:** 实例方法登记**全签名 overload 键**（稳定、加删重载不漂移）**+ 裸规范槽别名**（仅多态承担者）；
调用点 resolved→全键 / 多态→裸槽；VM 原始/装箱路径**增探全键**、vtable 非规范虚重载**保 `$`**、interp 与 JIT
候选列表**同步**；协议名单统一为单一 SoT。静态方法维持既有恒 mangle（`stabilize-dispatch-keys` 已落地）。

## IR Mapping

无新 IR 指令 / 无 wire 布局变化。变化：`CallInstr`/`VCallInstr` 方法名操作数（resolved 调用改发全键）、
zpkg SIGS/导出方法名（新增全键 + 裸规范槽别名条目）、method table 别名条目。zbc minor + zpkg minor 双 bump
（strict-pin，触发两代自举整树重键）。

## Pipeline Steps

- [ ] TypeChecker / MemberCollector（全键 + 裸规范槽标记；泛型 arity + 归一 T名 + FQN composite；协议名单统一）
- [ ] Overload 决议 / InheritanceResolver（override 采纳 origin 规范槽 + 全键；替换不变对齐）
- [ ] IR Codegen / CallEmitter（resolved→全键 emit；多态→裸槽 emit；DepIndex/ExportedType/TSIG 登记双标识）
- [ ] VM 元数据加载（method table 全键+裸别名；vtable 非规范虚重载保 `$`）
- [ ] VM 派发（原始/装箱路径增探全键+裸回落；interp 与 JIT 镜像）
- [ ] 格式版本（zbc minor + zpkg minor 双 bump + reader 常量 + fixtures 版本-patch）
