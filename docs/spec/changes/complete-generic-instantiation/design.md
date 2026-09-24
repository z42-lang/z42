# Design: 泛型实例化的单调化

> 2026-09-24 重写（原文档是「放宽跨包闸门」，其核心机制 D2 被实测推翻，见 §附录）。
> S1 段为待实施定稿；S2 / S3 是方向记述 + **已实测的障碍清单**，各自开工前回到阶段 5 定稿。

## 支点

承自 #774 的两个正交概念，**不得混用**：

- **身份名**：描述符 / `StructAlloc` / `ObjNew` / 数组元素名。所有具体实例化都有。
- **特化名**：布局查询 / 成员派发。仅布局与定义不同者才有。

本 change 加入第三个，且它是本 change 的全部要点：

- **特化的闭包**：特化一个实例化，就必须特化**所有以该具体实参操作它的泛型体**。
  少特化一处，那一处就按擦除布局读写同一批字节 ⇒ 静默错值。

## 根因（一句话）

**z42 的 IR 把字节偏移烘焙进指令**（`struct_fget_prim %0 @8`），而泛型体只编一份。
于是「一份体」与「多种布局」必然对不上，除非每种布局各编一份体。

## S1：本包闭包

### 今天特化什么、漏了什么

| | 今天 | S1 之后 |
|---|---|---|
| 实例化类型自己的成员（`Loc<P2,int>.Loc` / `.Equals$1` / `.ToString`）| ✅ #774 已做 | 不变 |
| **泛型自由函数 / 泛型方法**（`ReadSecond<T>` 以 `T=P2` 调用）| ❌ **只编一份擦除体** | 按具体实参各特化一份 |
| 泛型体内引用的**别的**实例化 | ❌ | 闭包到不动点 |

### D1：判据

#774 的 `InstDiffersFromDef(instName)` 判的是**类型**：实例化布局是否不同于定义布局。
泛型**体**要的是另一件事：

> 这个体在该替换下**触碰到的任一 struct 布局**，是否不同于擦除布局。

最直接的充分条件：**该体引用到的任一实例化类型 `G<…>` 满足 `InstDiffersFromDef`**。
（体内所有 struct 访问的 owner 要么是非泛型类型——布局与替换无关，要么是某个实例化。）

⇒ 判据可以完全复用既有的 `InstDiffersFromDef`，只是**作用对象**从「被特化的类型」变成
「体内出现的每个实例化」。布局都相同 ⇒ 特化是 no-op ⇒ 跳过即字节不变（自带闸门性质，
与 #774 同源）。

### D2：闭包算法

已有地基：`IrGen.Generate` 尾部的**工作表 + 不动点**（`InstLayouts` → `EmitInstantiation`）。
S1 把工作项从「实例化类型名」扩成两类：

```
工作项 ::= 实例化类型 G<A,B>          （#774 已有）
         | 泛型体实例 f<A>            （S1 新增；f = 自由函数 / 方法，A = 具体实参）

不动点：
  发射一个工作项 ⇒ 扫描其体内出现的实例化与泛型调用 ⇒ 新工作项入表
  直到集合不再增长
```

不动点必然收敛：工作项来自「源码中出现的类型/调用 × 具体实参组合」，而具体实参只能来自
已有工作项的实参集合的子结构，集合有限。
⚠️ 仍需**显式上限 + 超限报错**（防御未来的元数据缺陷把编译器挂死，同
`try_fixup_inheritance` 的 `fixup_cap` 先例）。

### D3：调用点派发

调用点必须拼出与发射端**逐字节相同**的特化名。#774 已把类型侧收敛到单一出口
（`IrGenMemberEmitter._irOwner`）；S1 要给**泛型体**建立同样的单一出口，
并让调用点与发射端**都调它**。

⭐ #774 教训 6：同一判据散在多处 ⇒ 只改一处必漏（当时属性那处漏了，症状是
`MissingSymbolException`）。S1 的泛型方法名同理。

### D4：代码膨胀与去重

每个 `(泛型体, 具体实参)` 一份体。去重靠 D1 的判据本身：布局相同的实参组合不特化、共享
擦除体。这既是正确性闸门也是膨胀闸门。

---

## S2：跨包模板投送（方向 + **已实测的障碍**）

下面每条都是 2026-09-23/24 在「放宽闸门」那版实现里**实际撞到**的，不是预判。
谁做 S2 都会再撞一遍，除非先读这里。

1. 🔴 **`SemanticModel` 只装本 CU 绑定过的体。**
   `IrGenMemberEmitter.EmitMethod` 的 `if (model.HasBody(ownerKey + "." + methKey))` 对导入
   定义**恒假** ⇒ 走 AST 那条会**静默什么都不发**，症状是调用点派发到
   `Std.ValueTuple2<int,string>.ValueTuple2` 而运行期 `MissingSymbolException`。
   ⇒ 模板必须自带可直接发射的体，不能指望消费方的 SemanticModel。

2. 🔴 **三处限定用的是「当前 CU 的 ns」，跨包必错。**
   `IrGenMemberEmitter._irOwner` / `IrGenTypeEmitter` 的 `eqIr`·`rq` / `ClassDescBuilder`
   的 `_instLayoutDesc`·`_instClassDesc` 都用 `IrGen._q`（硬贴当前 ns）；而调用点走
   `EmitContext.QualifyClass`（查 `ImportedClassNs`，得定义所在包）。
   跨包实例化的定义在别的包 ⇒ 两边拼出不同名字。
   ⇒ 需要一个「按定义所在包限定实例化名」的**单一出口**，四处都改调它；
   并且 `QualifyClass` 本身要在 `<` 处切开（整串去查 `ImportedClassNs` 必 miss）。

3. 🔴 **描述符名对不上时是「静默 miss 到兜底布局」。**
   运行期 `resolve_layout` 查不到就回落「只有 size、空引用位图」⇒ **零引用叶子的实例化
   恰好照常工作，有引用叶子的才崩**（`struct ref leaf at byte offset 8 not in type layout`）。
   #774 在本包踩过一次，跨包会原样重演。**「能跑」不能作为名字对齐的证据。**

4. **反造的 AST 没有 `RegKey`。**
   `SymbolCollector.RegisterMethod` 只跑本包声明；缺键时 `OverloadResolver.MethodKeyOf` 抛
   「注册路径漏走 SymbolCollector.RegisterMethod（unify-regkey 不变量）」。
   键**不能重算**（规则是兄弟集相关的，重算等于把规则抄第二遍），要从导入符号取回。

5. **「用户未显式声明才合成」这条守卫对导入定义反向。**
   导入 record 的 `Equals$1` / `ToString` **本来就在 TSIG 里**（那正是生产方合成的同一份），
   守卫会把该发的挡住。

6. ✅ **合成产物跨模块重复已解决**（已合入本分支）：见 §D4-fix。

## S2 的先决条件：D4-fix（已实施）

每个用到 `Std.ValueTuple2<Int32,String>` 的包都会**各合成一份**它的描述符与成员。两份是
`(定义, 类型实参)` 的确定性函数，第二份到达**不是**歧义。不区分的话，歧义函数
**一调用就抛**（`exec_call` 的 use-site 判定），而「库内部用了元组、主程序也用了」就已经
撞上——是常态形状，不是边角。

实测三条路径：

| 路径 | 行为 |
|---|---|
| `struct_alloc` | 不查歧义表，只 `try_lookup_type` 取布局 ⇒ 不受影响 |
| 类型描述符重复 | `registry.rs` warn + `note_ambiguous_type` + first-wins 丢弃第二份 |
| **合成 ctor 等成员函数重复** | 🔴 `note_ambiguous_function` → `exec_call.rs` **调用即抛** |

修法：加载器区分「合成实例化产物」与「用户声明」。判据「名字含 `<`」可靠——泛型定义按
**裸名**发、arity mangle 走 `Name$N`、伪实例化已被 `_isConcreteTypeArg` 排除。
结构不一致时**仍按歧义处理**，不静默取第一份。

编译期 E0601 经核实**不会**误报：`PkgCheckFqn` 对实例化返回**定义**的 FQN。

---

## S3：退役擦除名回落

`vcall_resolve` 的擦除名回落（miss 后剥实参重试）是 #774 为「布局相同的实例化共享定义体」
配的。全量单调化之后，miss 不应再悄悄落到擦除体上——那正是布局分裂的藏身处。

---

## 验证纪律（三阶段共用）

- ⚠️ **`xtask test` 全绿 ≠ JIT 验过**：golden 只跑 interp。改 struct 访问路径**必须**显式
  `xtask test e2e --mode jit`。（`./xtask test e2e jit` 会因位置参数错静默 `rc=2`；
  `Z42_JIT=1` **不是旋钮**，正确的是 `--mode jit` / `Z42_MODE=jit`。）
- **阴性对照只变一个变量**：不命中判据的实例化，编译产物必须**逐字节不变**。
- 自举字节不动点 `gen1 == gen2`。
- ⚠️ **跨包的事一律走 harness 判定**：手工 `z42c build` / 单文件 `--dump-ir` **不加载依赖
  TSIG**（元组会显示成 `Demo.<unknown>`），据此下的结论全是假的。我为此误判过两次。

## 附录：原 D2 为什么不成立

原设计称「反造一个等价 `ClassDecl` 即可完全复用 `EmitInstantiation`」。该假设有两层错：

1. **工程层**：`EmitInstantiation` 依赖 `SemanticModel` 里**已绑定的体**（见 S2 障碍 1）。
2. **根本层**（致命）：即便把成员体发出来了，**操作该实例化的泛型代码**仍是擦除的一份。
   实测 `ReadSecond<P2>((P2,int))` → `2`，`dict_iter` 的 `sum3` → `0`。
   ⇒ 「消费方能否重建成员」根本不是正确的判据，正确的判据是**闭包是否完整**。
