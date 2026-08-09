# Design: 去虚化扩到 sealed override + 泛型 sealed

> follow-up of `add-sealed-devirt`（#142，本地）+ `extend-sealed-devirt-imported`（#147，跨包）。
> 复用其 `_devirtQualifiable` / `_depHasFunction` / `ResolveSealedTarget` 基链 walk 骨架。

## 背景：去虚化的充分条件

去虚化 = emit 时把 `VCall`（运行期按 `type_desc.vtable` 派发）降级为直接 `Call @FQ`，解锁 `IrInline`。
**唯一安全前提：目标函数编译期唯一**——即对 receiver 静态类型 `S` 的任意运行期类型 `R`（`R ≤ S`），
`R` 上该方法的最派生实现都是同一个函数。#142 只用了一个充分条件：`S` 是 sealed 类 → `R == S` → 唯一。
本 change 补两个同样充分的条件。

## Decision 1：sealed override 的目标唯一性证明

设调用 `recv.M()`，receiver 静态类型 `S`（`Z42ClassType`，非泛型可限定）。沿 `S` 的基链向上找到
**declClass** = 最近声明 key `M` 的可限定类（`ResolveSealedTarget` 既有 walk）。

- declClass 由「从 S 向 base 方向 walk」得到 → declClass 是 S 的自身或某**超类** → `S ≤ declClass`。
- 运行期 `R ≤ S`（R 是 S 的子类型）。故 `R ≤ S ≤ declClass`。
- declClass 上 `M` 标 `sealed`（`sealed override`）→ 语义强制（#140 E0427-29）**declClass 之下无任何类能
  override M** → 对任意 `R ≤ declClass`，R 的最派生 M 实现 = declClass.M。**唯一** ∎

故门控 = declClass 处 `ms.IsSealed`。两点必须成立才安全，缺一即回落：
1. **declClass 必须是最派生声明**（≤ S）——walk 从 S 起、命中即停（本地 `LocalClasses` 命中即声明；
   imported 靠 `_depHasFunction` FQ 校验排除 TSIG 展平的继承方法，命中真声明才停）。这保证 R 看到的 M
   就是 declClass.M 或其**继承**（不可能是更派生的 override，因 sealed 禁止）。
2. **declClass.M 必须 sealed**——否则某 `R < declClass` 可 override M → 不唯一。

> **反例（必须回落）**：`class Mid : Base { override int M(){} }`（非 sealed override）。`Mid m` 可持有
> `Leaf : Mid { override int M(){} }` 实例 → `m.M()` 该调 Leaf.M。declClass=Mid，`ms.IsSealed=false` →
> 门控失败 → 回落 VCall。✓

### 与「整类 sealed」的统一

`classSealed`（S 自身 sealed）是另一充分条件：`R == S`，无论 M 在哪声明目标都唯一。二者取**或**：
`sealedHere = classSealed || ms.IsSealed`。整类 sealed 时门控恒真 → 逐字节复现 #142/#147 行为（回归零风险）。
`classSealed` 沿 walk **不随基类变化**（它描述的是 receiver 类 S，不是 declClass）——继承场景（sealed 类
继承基类方法）里 walk 到非 sealed 基类仍命中，正确。

## Decision 2：入口从「SealedReceiverClass」泛化为「DevirtReceiverClass」

`SealedReceiverClass` 的 `if (!ct.IsSealed) return null` 删除 → 接纳非 sealed 类。安全性不受影响：
非 sealed 类进入后，`ResolveSealedTarget` 的 `classSealed=false` 门控只在方法 sealed 时才去虚化，否则 `""`。
即**放宽入口、把安全判定下沉到声明点**。代价：非 sealed 类的每个虚调用多一次基链解析（编译期，O(继承深度)，可忽略）。

## Decision 3：泛型 sealed 的 `$N` arity-mangle 目标名

泛型是**类型擦除 + arity-mangle**（非单态化）：`sealed class Box<T>` 发射为单份函数 `<ns>.Box$1.Get`
（`IrGen._classIrShortName`：`c.Name + "$" + TypeParams.Count`）。去虚化 `Box<int>` receiver：

- receiver 静态类型是 `Z42InstantiatedType`（`Box<int>`），`.Def` = `Box` 的 `Z42ClassType`（`GenericParamCount=1`，
  `IsSealed`）。`DevirtReceiverClass` 需**解包 `Z42InstantiatedType` → `.Def`**（当前 `as Z42ClassType` 得 null）。
- 删 `GenericParamCount>0 → 回落`，但目标名从 `QualifyClass(ct.Name())`（="Box"）换成
  **`QualifyClass(ct.Name()) + "$" + GenericParamCount`**（="<ns>.Box$1"），逐字节匹配 IrGen。
- `_devirtQualifiable` / `LocalClasses` / `ImportedClassNs` 的**键**：泛型类以 `Name$N` 注册还是 `Name`？
  → 实现期需核对（IrGen 注释「泛型类以 Name$N 键注册」提示可能是 `Name$N`）。以此决定 `_devirtQualifiable`
  查的是 `ct.Name()` 还是 mangle 名。**不确定即回落**，靠自举不动点 + e2e 兜底。

> ② 的风险全在「目标名逐字节一致」——差一字节 = 运行期 `undefined function` 或自举 gen1≠gen2。故 ② 单独一 commit，
> ① 先落稳。

## 正确性铁律（沿用 #142/#147）

- **不确定即回落 `""` → VCall，永不 miscall。** 每个 return "" 分支都是安全的（虚派发永远正确）。
- **`Opt.Devirt` 门控**：所有新路径受 `DevirtEnabled()` 保护，`--no-opt` 对拍验证纯优化等价。
- **自举不动点**：z42c 自身源码大量 `sealed`/`sealed override`（如 `EmitContext` 等 `public sealed class`）→
  本 change 改变 z42c 自编译输出 → 当次 gen1≠gen2（D7 破一代）→ warm 重建自愈至 gen1==gen2。
