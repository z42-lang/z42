# Design: 让泛型实例化成为运行期真正的类型

> 待 User 过 6.5 gate。下面每条「实测」都是在 main 上跑出来的，不是推断。

## 支点：一句不变式

> **实例化的声明形状必须是真相** —— 描述符、基类链、方法签名、静态字段键、`is`/`as` 的目标名，
> 全部按实参代换；擦除名只保留为**回落**，不作为身份。

前序三个 change 兑现的是这条不变式的另外两面：

| 面 | 谁做的 |
|---|---|
| **布局**（字段偏移 / 引用位图） | #774 |
| **体**（操作它的泛型代码） | #820 / #825（S1 a–f） |
| **声明形状** | ← 本 change |

## D1：五处改动与它们的咬合关系

```
          ┌─────────────────────────────────────────┐
          │  实例化取得独立身份（有完整描述符）        │
          └───────────────┬─────────────────────────┘
                          │ 一旦成立，下面三条**必须同时**成立，否则静默错
      ┌───────────────────┼────────────────────────┐
      ▼                   ▼                        ▼
 is/as 携实参        基类链可识别              静态字段分槽
 （否则 x is G<int>  （否则 is G 断 / 字段     （否则两个实例化
   静默变 false）      合并出两份同名槽）        共享一个槽）
```

⚠️ **这张图是实测出来的，不是设计出来的**：我先只做了「身份」一格，立刻撞上
`x is GBox<int>` 静默变 false（`_bindIsExpr` 扔掉实参 + 运行期按名比）和
「base 记 `Std.Object` 则 `is GBox` 断 / 记 `GBox` 则字段合并出两份」的二选一。

## D2：描述符要合成到什么程度

`_instClassDesc`（`ClassDescBuilder.z42`）今天：

- `il.FieldCount <= 0` 直接 `return null` ⇒ **普通泛型 class 根本不发描述符**
- 基类硬编码 `"Std.Object"`、无接口、无静态字段

要补的：基类链、接口、静态字段。字段本身**已经是代换后的**
（`ObjectLayoutOf(instName).FieldTypeNames`），这正是 B 所需要的。

⭐ **vtable 不用合成**（实测）：运行期 `build_type_registry` 从 `own_methods` + 基链 merge
出来，不在 TYPE 段。#774 当初把它列进「完整描述符」的负担里，那一条是**高估**。

## D3：A（返回值 lifetime）的机制

特化体把返回值拷进**自己帧**的 arena 再返回句柄：

```
fn @Demo.G<P2>.Get(1) -> T {
  %1 = struct_alloc Demo.P2 [16B]     ← 自己的帧
  ...逐字段拷贝...
  ret %1                              ← 帧一弹即悬垂
}
```

z42 本有 **sret** 约定（调用方预留返回槽），但两侧的判据都看**未代换**的返回类型 `T`：

| | 判据 | 对 `G<P2>.Get` |
|---|---|---|
| callee | `FunctionEmitter._blobStructNameT(返回类型)` | `T` 既非 ClassType 也非 InstantiatedType ⇒ `""` ⇒ 不走 sret |
| caller | `CallEmitter` 的 `_isBlobStruct(c.Type())` | `c.Type()` 也是未代换的 `T` ⇒ 否 |

两侧都判否 ⇒ 表面自洽，代价是返回已死帧的句柄。

🔬 **实测原型**：只让 callee 认出 `T→P2`（走 sret）后，症状立刻变成
`takes 2 physical argument(s), the call passes 1`。
⇒ **「lifetime unsound」与「签名解析不到」是同一条 bug 的两副面孔**，取决于哪一侧先判出具体
类型。**只修一侧＝换个地方错**，这是本 change 必须整体做的直接证据。

修法：`MemberResolver._substGenericSig(sig, recv)` **已存在**（`MemberResolver.Subst.z42:48`），
今天只用在 `MemberResolver.z42:211`（接口成员）与 `ConstructTyper.z42:282`（构造器）。
把它铺到一般实例方法调用那条路 ⇒ `g.Get()` 的静态类型成为 `P2` ⇒ 两侧自然都走 sret。

## D4：B 的机制

```z42
class DInt : GBox<int> { }
```

`ClassDescBuilder.z42:151-162`（`fix-generic-base-name`）把基类名**显式剥成裸名** `GBox`，
注释写明理由：写 `Ns.Bag<T>` 进元数据会让 VM 建 vtable 时找不到类。
于是运行期从 `GBox` 合并字段，`V` 的 `type_tag` 是擦除的 `T` ⇒ `ObjNew` 取默认值给 `null`。

> User 的取证「实参在运行期元数据里根本不存在」= **就是这一步丢的**，故只能在编译期发描述符
> 那一侧修，运行期无解。

身份成立后这条剥名的理由消失（`GBox<int>` 会有描述符），但**必须与 D1 的另外两格同时落地**。

## D5：判据必须单一出口

本线（#774 → #825）已经**四次**栽在「同一判据散在多处」：

| 次 | 分叉 |
|---|---|
| #774 教训 6 | 成员 IR 名的 owner 前缀散在 9 处，只改方法那处 ⇒ 属性漏 |
| S1-a | 身份路径有 `_isConcreteTypeArg` 闸门，布局路径没有 ⇒ 伪实例化硬崩 |
| S1-e | 闸门 `LocalClasses` 是包级、登记表按 CU ⇒ 跨文件失效 |
| S4-C | 身份用 `IsStructType`、登记用 `IsBlobStruct` ⇒ 单字段泛型 struct 落进夹缝 |

⇒ 本 change 的硬性要求：**「实例化的形状从哪来」只能有一个出口**，`is`/`as`、描述符、
签名代换、静态字段键**都调它**。

## 验证纪律

- ⚠️ **`xtask test all` 在本地 ≠ CI**：本地那条只含 e2e + stdlib + compiler，**不含
  `test lines` / `walkers` / `docs` / `diagcodes` 等**。要么逐个点名跑，要么承认只有 CI 是权威。
- ⚠️ **`xtask test` 全绿 ≠ JIT 验过**：golden 只跑 interp，必须显式 `--mode jit`。
- ⚠️ **单文件用例对「按 CU / 按包」的机制没有判别力** —— S1-e 就是单文件全绿之后才发现跨文件全失效。
- **阴性对照只变一个变量**：基线不同就不是对照（我在 S1 期间拿新 main 的 src 比由它构建的
  nightly，得出过错误结论）。
- 自举字节不动点 `gen1 == gen2`；静态字段换键还需 `xtask test bootstrap`。
