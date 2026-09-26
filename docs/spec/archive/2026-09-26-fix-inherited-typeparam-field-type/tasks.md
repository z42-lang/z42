# Tasks: 继承来的型参字段按闭合基类代换类型

> 状态：🟢 已完成 | 创建：2026-09-26 | 完成：2026-09-26
> 分支/worktree：`fix-inherited-typeparam-field-type` @ `wt-inhtparam` | 基于：origin/main `4b0d7a018` (#840)
> 类型：`fix`（**lang/编译器** —— 符号层类型代换；无格式变化）
> 授权：User「那请你继续修复」

## 现象（修前，实测）

```z42
class GBox<T>    { public T V;   public GBox() { } }
class DInt : GBox<int> { public DInt() { } }

DInt d = new DInt();
d.V + 1        // E0402: operator `+` requires numeric operand, got `T`
if (db.V) { }  // E0402: `if` condition must be `bool`, got `T`
int y = d.V;   // ⚠️ **静默通过** —— `T` 对 `int` 可赋，错类型一路流下去
```

**两档表现，第二档更糟**：报错档拒掉了合法代码；静默档连诊断都没有。

## 根因：基类的「声明形态」没人留

`BaseName` 刻意只存**裸名**（`GBox`）—— 它是 `Classes` 的**查找键**，每个消费方都拿它当键用
（见 `fix-generic-base-name` 在 `StubCollector` 留的注释：存 `"Bag<T>"` 会让整条 base 链静默截断）。
但沿基类链上溯做**型参代换**时，需要知道派生类把哪些实参喂给了基类 —— 只有裸名的话，
`DInt : GBox<int>` 上溯拿到的是**未实例化**的 `GBox` 定义，它的 `T` 永远换不掉。

`_passInheritFields` 因此把基类的 `FieldSymbol` **原样搬**进派生类，`FieldType` 还是 `T`。

⭐ **这与接口侧是同一个 bug，只是那边早修了**：`Z42InterfaceType.BaseRefs`（#792）存的正是
父接口的声明形态（`IMid<int>`），抬头注释把理由写得一模一样：「成员访问则拿到裸 `T`」。
**类侧一直缺这一块。** ⇒ 本变更照它的形态补。

## 修法（三处，都照接口侧的既有形态）

1. `Z42ClassType.BaseRef`（`TypeExpr`）—— 基类的声明形态，与 `BaseName` 平行，无实参时为 null。
2. `StubCollector`：设 `hasBase` 的同处留 `c.Bases[b]`（AST 节点，**不就地解析** —— 本 pass 只建
   stub，被引用类型可能还没登记；同 `_addIfaceBases` 的理由）。partial 合并那条路也要填。
3. `InheritanceResolver._passInheritFields`：沿基类链**逐层组合代换**。
   `curInst` = 「把 `cur` 看成从 `ct` 出发实例化出来的样子」，每上一层组合一次
   ⇒ `Deep : DInt : GBox<int>` 任意深度都换得到底。组合手法照 `InterfaceClosure.BaseAt`。

**两条保命细节**（都在代码注释里）：

- **必须产出新的 `FieldSymbol`**：原对象被基类的 `Fields` 表共享，就地改会把基类自己的字段类型
  也改掉 ⇒ `GBox<int>` 与 `GBox<string>` 两个派生类**互相污染**。用例 ⑥ 专钉这条。
- **判「写没写实参」用 AST 的 `ArgCount`，不用 `GenericParamCount`**：后者只说基类是泛型定义，
  说不了这一处写没写（`class D : GBox` 也命中它）。接口侧已经记过这条坑。
- 代换出 `Unknown` 时**原样返回**（形参名不在基类形参表里，如种子旧 TSIG 没带
  `GenericParamNames`）—— 宁可留原类型，也不把它降成 `Unknown`。

## 进度概览

- [x] 1 `Z42ClassType.BaseRef` + 注册端回填
- [x] 2 `_passInheritFields` 逐层组合代换
- [x] 3 用例 + 阴性对照
- [x] 4 边界摸底（含跨包）
- [x] 5 全量 GREEN + 指纹判定 + 文档 + PR

## 3 用例与阴性对照

`src/tests/generics/inherited_typeparam_field.z42`（NEW，interp + jit 双过）覆盖八格：
算术 / `if` 条件 / 赋给具体局部（静默档）/ 多型参 / 两层继承 / 派生类本身泛型 /
🔒 两个实例化不互相污染 / 🔒 非泛型基类不受影响 / 🔒 基类自身直接用。

同时把 `src/tests/types/value_field_zero/` 里**因本缺口注掉的两行**取消注释。

**阴性对照**：把 `_substField` 临时改成恒返回原对象（= 撤回本修复），重建编译器后
两个用例分别报 **7 条 / 2 条 E0402** ⇒ 它们真的钉着这个修复，不是恰好绿。

## 4 边界摸底

| 形态 | 结果 |
|---|---|
| `DInt : GBox<int>` 算术 / `if` / 赋局部 | ✅ 修好 |
| `DPair : Pair<int,string>` 多型参 | ✅ 按名字对位 |
| `Deep : DInt : GBox<int>` 两层 | ✅ 换到底 |
| `Sub<U> : GBox<U>`（派生类本身泛型） | ✅ 基类实参=派生类型参，再由收者实例化 |
| 🔒 `GBox<int>` 与 `GBox<string>` 并存 | ✅ 不互相污染 |
| 🔒 非泛型基类 / 基类自身 | ✅ 不受影响 |
| **跨包**闭合泛型基类 | ⚠️ **编译期通了**，但撞到一条**既有**运行期缺口，见下 |

### ⚠️ 跨包：编译期已通，运行期是另一条既有缺口

写了 `src/tests/cross-zpkg/generic_base_field_crosspkg` 后实测：**不再有 E0402**
（导入侧的型参名 + 本地的声明形态足够代换），但运行期抛

```
Std.MissingSymbolException: base type `Demo.GBase.GBox<int>` of `Demo.GBaseApp.DInt`
could not be resolved; every inherited field and virtual method is absent from the layout
```

⭐ **判定它是不是我引入的 —— 用「避开算术的写法 + 撤回本修复」分辨**：把消费方改成只
`d.V = 41;` + 打印（修前也编得过的静默档），再把 `_substField` 改成恒返回原对象重建，
**同一条错误照样抛** ⇒ **既有缺口，与本变更无关**：跨包派生于闭合泛型基类**从来没在运行期通过**。

⇒ 那条 fixture **不并入本 PR**（不留一条注定红的用例），缺口移交泛型实例化线。
本变更只声明**本包**范围；跨包侧现在是「编得过、运行期一条**明确且响**的 MissingSymbolException」，
比修前的「算术被拒 + 赋值静默」不差 —— 而且那条运行期错误修前同样可达。

## 5 收尾
- [x] 5.1 `xtask test all` **GREEN**（8m12s；指纹 bump 后重跑仍 GREEN 8m47s）
- [x] 5.2 指纹判定：**21 → 23，必须 bump**。判据是实测 A/B（两个二进制、同一份**修前也编得过**的源码
      —— 静默档 `int y = d.V;`）：zbc 942 字节里**差 2 个字节** —— offset 713 的 zbc 类型 tag
      `0x00`→`0x04`(I32)、offset 816 的 REGT `0`→`3`(`IrType.I32`)，即「类型未知」变「具体类型」；
      **IR 文本一字不差**（差异全在元数据）。⇒ 那类源文件哈希不变而发码变，不 bump 会复用旧条目
      （那里 REGT 是 Unknown，优化 / JIT 只能走保守路径）。
      ⚠️ 报错档（`d.V + 1`）不能当 bump 理由 —— 它修前**编不过**，根本不存在缓存条目。
      ⚠️ stdlib 与编译器自身没有「派生于闭合泛型基类」这种形状 ⇒ CI 的 fingerprint 守门测不到
      （只会漏判），按 version-bumping.md 手动 bump。
      ⚠️ **让号实录**：原取 22，在飞的 #843 已先占 22 ⇒ 按 parallel-development.md §4.1 让到 **23**
- [x] 5.3 文档同步：`internals/compiler/generics.md` 新增「继承来的型参**字段**：符号层按闭合基类代换」
      一节，并**明确把它与同页第 2 条（运行期 `type_args`）区分开** —— 两者都叫「继承来的型参」，
      一个在类型检查期、一个在运行期，混读会判错；`reference/language/inheritance.md` 新增
      「继承闭合的泛型基类」一节 + 对照表一行 + **跨包限制**（用户会撞到，给了「改用组合」的出路）
- [x] 5.4 归档（阶段 9，在本 PR 内）→ `archive/2026-09-26-fix-inherited-typeparam-field-type/`
