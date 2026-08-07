# Design: 去虚化扩到 imported sealed 类

## Architecture

```
 a.M()  a : imported sealed class A（源 ns = Pkg.Ns）
   │
   ▼  SealedReceiverClass(A):  IsSealed ✓ 非泛型 ✓ 且 _devirtQualifiable("A")=in ImportedClassNs ✓ → A
   ▼  ResolveSealedTarget(A, RegKey, argc):
        沿 A→base 链（Symbols.GetClass，imported 类已在表）找最近声明 RegKey 且非 abstract 的类 C，
        要求 C 也 _devirtQualifiable（in LocalClasses 或 ImportedClassNs）+ 非泛型
        → QualifyClass(C.Name()) + "." + RegKey
           （C 本地 → 当前 ns；C imported → ImportedClassNs 里的源 ns = 导出包发射名）
   ▼  CallInstr(dst, target, [recv, ...args], argc+1)  →  IrInline 内联
```

## Decisions

### Decision 1: 走 Symbols + QualifyClass，不走 DepIndex.GetInstance

**问题：** imported 目标名两条路——(A) `Deps.GetInstance(RegKey, argc).QualifiedName`（非虚 imported 调用
已用的证过机制）；(B) 沿 `Symbols` 基链 + `QualifyClass(定义类)+"."+RegKey`（与本地同构造）。

**探索发现：** `DependencyIndex.Instances` 按**bare 方法名**（去 `$N` mangle）+ arity 注册，且同 bare 名跨
多类 → **Ambiguous 排除**（`GetInstance` 返 null）。故 (A) 对 imported 虚方法**覆盖低**（只无歧义裸名命中），
且 bare-key 与解析后 RegKey（全 mangle）匹配面窄。

**决定：** 走 **(B) Symbols + QualifyClass**。
**理由：** 覆盖高（沿真实基链解析定义类，不受 bare 名歧义限制）+ 与 v1 本地路径**同一构造**（`QualifyClass+RegKey`，
只是 imported 时 QualifyClass 经 `ImportedClassNs` 给源 ns）——最小改动、逻辑统一。正确性靠 Decision 3 兜底。

### Decision 2: `_devirtQualifiable` 守卫——in LocalClasses 或 ImportedClassNs

**问题：** v1 的 `LocalClasses.ContainsKey` 守卫作用是「保证 `QualifyClass(name)` 给出与 IrGen 发射一致的 FQ」
（本地同-cu 类 = 当前 ns）。imported 类不在 LocalClasses，但 `QualifyClass` 对它经 `ImportedClassNs` 给源 ns。

**决定：** 新增 `private bool _devirtQualifiable(string name)` = `LocalClasses.ContainsKey(name) ||
(ImportedClassNs != null && ImportedClassNs.ContainsKey(name))`。`SealedReceiverClass` 与 `ResolveSealedTarget`
的守卫都换成它。
**理由：** 这正好覆盖「QualifyClass 能给出正确 FQ」的两种情形（本地当前 ns / imported 源 ns）；**两者皆非**
（如某中间基类既非本地声明又未进 ImportedClassNs → QualifyClass 会误加当前 ns）→ 返回 ""，回落 VCall（安全）。

### Decision 2.5: imported 定义类必须用 Deps 校验 FQ 真实发射——排除 TSIG 展平的继承方法（实现期发现）

**问题（cross-zpkg e2e 暴露的真 bug）：** Decision 1/2 假设「`ct.Methods.ContainsKey(methodKey)` 命中 = ct
声明该方法」。这对**本地**类成立（`SymbolCollector` 只填**声明于本类**的方法），但对 **imported** 类
**不成立**——`ImportedSymbolLoader._fillClass`（ImportedSymbolLoader.z42 行 266-293）把 TSIG 里**已展平的
继承方法**灌进**每个派生类**的 `Methods`（`ExportedClassZ.Methods` 本身就含祖先方法，行 260 注「TSIG 继承
字段展开」，方法同理）。于是 `sealed class Leaf : Tagged {}`（Leaf 不 override `Tag`）在 imported 侧
`Leaf.Methods.ContainsKey("Tag")` = **true** → 走原逻辑构造 `QualifyClass(Leaf)+".Tag"` = `Demo.Sld.Leaf.Tag`
——**一个从未被任何包发射的函数名**（真身是 `Demo.Base.Tagged.Tag`）→ 运行期 `undefined function`。
（`ExportedMethodZ` **不带**声明类字段，加也是 TSIG 格式 bump，越界；故 imported 侧无法从符号区分「声明 vs 继承」。）

**决定：** imported 定义类候选，在返回前用 **`Deps` 校验该 FQ 确为真实发射函数**——`_depHasFunction(fq)` =
`Deps.Statics.ContainsKey(fq)`。依据：`DependencyIndex.AddModule`（DependencyIndex.z42 行 110）把**每个跨包
函数**按其**完整 FQ**（`ns.Cls.Method[$arity]`）注册进 `Statics`。故：
- **命中**（如 `Demo.Sld.Circle.Area`，Circle override 了 Area，demo.sld 真发射）→ 本类确有该函数 → 去虚化。
- **未命中**（如 `Demo.Sld.Leaf.Tag`，Leaf 只继承）→ 本类仅继承 → **不返回，继续沿基链上溯**，直到命中真正
  声明类（`Demo.Base.Tagged.Tag` 在 Deps → 命中 → 去虚化跨包基类实现）。

**为何只对 imported 校验：** 本地类的函数**不进 Deps**（Deps 仅含依赖包），本地路径靠 `LocalClasses.ContainsKey`
分支**先行短路返回**（`Methods` 命中即声明，无需 Deps 校验）——本地路径零改动、零回归。

**为何这是「最近声明」= 正确目标：** 从 receiver 精确 sealed 类型 `startCt` **向上**走，第一个 FQ 命中 Deps 的
类 = 对「精确类型为 startCt 的对象」做动态派发时会命中的最近实现。sealed ⇒ runtime 类型必是 startCt ⇒ 该最近
实现即唯一目标。Deps 存在性校验精确地跳过所有「展平但未声明」的中间层，落到真正发射该函数的类。

### Decision 3: 正确性门——cross-zpkg e2e + `--no-opt devirt` 对拍

imported 目标名构造错 = 运行期 `undefined function` 或静默调错。门：
- **cross-zpkg e2e（主门，就是它抓到 Decision 2.5 的真 bug）**：`sealed_devirt_imported`——demo.base 出
  `Shape`/`Tagged`（各含 virtual），demo.sld 出 `sealed Circle:Shape`（override Area）+ `sealed Leaf:Tagged`
  （**跨包继承不 override** Tag），demo.app 按精确类型调 `c.Area()`=25 / `lf.Tag()`=100（跨包基类实现!）/
  `lf.extra()`=7 / `Shape s=new Circle(3); s.Area()`=9（非 sealed 静态类型→VCall→override）。目标名错即运行崩。
  **实测：修 Decision 2.5 前 `lf.Tag()` 崩 `undefined function Demo.Sld.Leaf.Tag`；修后输出 25/100/7/9 全对。**
- **devirt 确实开火的铁证**：VCall 回落**永不**发射直接 call → 上述 `undefined function Demo.Sld.Leaf.Tag`
  这条错本身就证明 imported sealed 调用确走了去虚化直接 call（否则 VCall 动态派发不会有这条错）。
- **`--no-opt devirt` 对拍的适用范围**：devirt 门控 `Opt.Devirt` 位在 `IrDump.DumpFuncOpt`（单源 IR 文本）
  路径可精确开关（#142 的 `test_sealed_devirt_*` 即此）；但 imported 需跨包 deps，`DumpFuncOpt` **无依赖解析**
  → imported 去虚化**无法**用单源 IR-文本单测覆盖（无 deps-aware 文本 dump，新增即 IrDump 越界）。且
  `build` 子命令路径的 `--no-opt devirt` 逐字节对拍**当前不生效**（多文件 build 的 CLI per-opt 位未贯穿到
  emit，与本 change 正交）→ 故 imported 的对拍以 **cross-zpkg e2e 输出正确性**为准（Decision 3 主门），
  不依赖 build 路径字节对拍。
- **既有覆盖**：本地路径不变（self-host 不动点 + local e2e + #142 `codegen_tests` 仍绿）；stdlib 280 用例里凡按
  imported sealed 精确类型调用的点此后走去虚化 → 若错则 stdlib 红（实测 280/280 绿）。

## Implementation Notes

- `SealedReceiverClass`：`if (!this._devirtQualifiable(ct.Name())) { return null; }`（替 `LocalClasses` 判）。
- `ResolveSealedTarget`：循环内 `if (!this._devirtQualifiable(ct.Name())) { return ""; }`（替 `LocalClasses` 判）；
  非泛型、abstract 跳过、沿 `Symbols` 基链**不变**。`Methods.ContainsKey(methodKey)` 命中后分两支（Decision 2.5）：
  - **本地类**（`LocalClasses.ContainsKey(ct.Name())`）→ 直接 `return QualifyClass+"."+methodKey`（声明即发射，Deps 无本包函数）。
  - **imported 类** → `if (this._depHasFunction(fq)) { TrackImportedClass; return fq; }`；未命中则**不返回**，
    `while` 继续沿基链上溯（跳过 TSIG 展平的继承方法，落到真正声明类）。
- 新增私有助手 `_depHasFunction(fq)` = `this.Deps != null && this.Deps.Statics.ContainsKey(fq)`。
- imported 类的 `Methods` 含 methodKey：`ImportedSymbolLoader._fillClass` 按 TSIG 的 RegKey 填 `ct.Methods`
  （key = TSIG 方法名 = RegKey），且 **TSIG 已展平继承方法** → 派生类 `ContainsKey` 对继承方法亦命中
  （正是 Decision 2.5 要用 Deps 校验排除的情形）。
- 改动**不 bump 格式**（复用 CallInstr，纯 emit 决策放宽；`Deps.Statics` 是既有跨包索引，无新字段）。

## Testing Strategy

- **cross-zpkg e2e**（主门）：`sealed_devirt_imported`——pkgA sealed（声明+继承）× pkgB 调用，结果正确。
- **codegen 单测**：imported sealed receiver → `call @<srcNs>.Cls.M`（非 vcall）；`--no-opt devirt` 对拍。
- **回归**：本地 devirt 用例（#142 的）+ 自举不动点仍绿（本地路径零改动）。
- **GREEN**：完整 `xtask test`（含 self-host + stdlib + cross-zpkg）。
