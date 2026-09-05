# Design: 补全泛型 `where` 约束校验

## 1. 判定规则的唯一真相源 = 运行期

`src/runtime/src/corelib/reflection/generics.rs::validate_type_arg_constraint` 已实现完整七项。
编译期**照抄**，不另立规则：

| 约束 | 运行期判定 | 编译期对应物 | 现状 |
|---|---|---|---|
| `class` | `!is_value`（`CLASS_FLAG_STRUCT` / 基元名单） | `_isClassArg`（排除 `PrimModel.IsScalarValue`） | ✅ 已实现 |
| `struct` | `is_value` | `_isStructArg` | ✅ 已实现 |
| base-class | `constraint_satisfied_by` | `SymbolTable.IsSubclassOf` | ✅ 已实现 |
| 型参引用 `where U : T` | `type_name_assignable` | `_satisfiesParamRef` | ✅ 已实现 |
| **interface** | `constraint_satisfied_by(arg, iface)` 循环 | **`SymbolTable.Implements`（已存在，未被约束路径调用）** | ❌ 本变更 |
| **`enum`** | `td.is_enum()` | **`symbols.EnumTypes.ContainsKey(name)`（已存在）** | ❌ 本变更 |
| **`new()`** | `!abstract && type_has_no_arg_ctor` | **`ct.OverloadsOf(ct.Name())` 找 `ParamCount==0`** | ❌ 本变更 |

关键：**三项缺失约束所需的判定能力，编译器侧全都已经有了**。
`GenericConstraint.z42` 那条「z42c 缺对应类型/信息」的注释**已经过时**：

- `SymbolTable.Interfaces` / `HasInterface` / `GetInterface` 存在；
  `Z42InterfaceType` 是独立类型；`Z42ClassType.InterfaceNames` 记录本类实现的接口裸名。
- `SymbolTable.Implements(cls, iface)` 沿 base 链扫 `InterfaceNames`，已被 `Conversion` /
  `DeclBinder` 真实消费。
- ctor **早已收集**（`MemberCollector`「ctor（G11）也收集为方法（名=类名，ret void）」），
  `Z42ClassType.OverloadsOf` 是现成枚举 API，`ConstructTyper` 已用它做 arity 校验。
- `SymbolTable.EnumTypes` 存在，本地 + 导入两侧都 seed。

⇒ 本变更主体是**接线**，不是造能力。

## 2. 决策 D1：接口匹配只比裸名

`ClassDescBuilder` 存约束接口时存的是 `NamedType.Name`（**裸名**，丢 `<...>`），
`SymbolTable.Implements` 比较的也是裸名。v1 **沿用裸名匹配**，不校验接口的类型实参。

这不是偷懒，它一次性消掉两个难题：

1. **F-bounded 自引用递归**：`interface INumber<T> where T : INumber<T>`（`z42.core/src/Protocols/INumber.z42`）
   若朴素展开约束会无限递归。裸名匹配天然不递归。
2. **泛型接口实参匹配**：`struct Int32 : IEquatable<int>` 满足 `where T : IEquatable<T>` 需要
   把 `T` 绑到 `int` 再比对——一套小型统一算法。裸名匹配绕开。

代价：`IEquatable<string>` 也会满足 `where T : IEquatable<T>`。**与运行期
`constraint_satisfied_by` 的行为一致**（它拿到的 iface 同样是常量池里的裸名），故不引入两边分歧。

→ 登记 Deferred：`where-constraint-future-type-arg-matching`。

## 3. 🔴 前置 bug：两个 reader 对同一段字节的布局理解不一致

`ZpkgReader._skipConstraintBundle`（`src/libraries/z42.ir/src/ZpkgReader.z42`）按**完整**布局跳过，
注释标了格式权威：

```
flags u8 → [bit2 base u32] → [bit3 tpRef u32] → ifaceCount u8 + u32× → [bit6 funcSig: pc u8 + u32×pc + ret u32]
```

而 `ZbcReader.z42` 的 TYPE 约束段**只读 bit3，不读 bit2 的 base u32**，注释自认「z42 writer 子集」。

今天不炸，只因当前 writer 恰好只写 bit3。**但只要任何 writer 置了 bit2，ZbcReader 就漏消费 4 字节
→ `ifaceCount` 读歪 → 整个 TYPE 段错位。**

⇒ **PR-A 先修 reader（读全 bit2/bit6），且必须早于任何「写新 flag 位」的改动（PR-3）。**
这条即使不做 where 也该修——它是格式契约被单侧退化的产物。

## 4. 变更点

### PR-0 — expected-compile-error 测试机制

`scripts/test/xtask_test_dist.z42` 的 sidecar 体系加 `expected_error.txt`：

- dir 模式 `<cat>/<name>/expected_error.txt`；flat 模式 `<cat>/<name>.expected_error.txt`
- 语义：编译**必须失败**，且 stderr/诊断输出**包含** `expected_error.txt` 的每一行（逐行子串匹配，
  不做全文比对——诊断措辞会演进，钉死全文等于给自己上枷锁）
- 与 `expected_output.txt` **互斥**：同时存在 → runner 报配置错误

> 独立价值：E0404 跨包 internal、E0451 static 类实例成员等一批诊断今天全靠手工验证 fixture。

### PR-1 — 接口约束

1. `ConstraintBundle` 加 `InterfaceNames[] / InterfaceCount`（受限写法：typed array + int count）
2. **去掉 `nt.ArgCount == 0` 过滤**；按裸名（去 `<...>`）先查 `symbols.HasInterface` → 接口约束，
   否则 `symbols.HasClass` → base-class 约束（与 `StubCollector` 的裸名约定一致）
3. **合并 `_fillBundle` 与 `_fillBundleM`**——两者今天是逐字重复的两份静默延后逻辑，
   改校验必须同时改两处。**先合并再改**（拆分与功能变更分开提交，见 code-organization）
4. `_checkBundle` / `_checkBundleM` 加接口分支，调 `SymbolTable.Implements`

**接口继承闭包缺口**：`Z42InterfaceType` **没有 base-interface 列表**，`Implements` 因此不走接口
继承链——`interface IDerived : IBase` + `class C : IDerived` 时 `Implements("C","IBase")` 返回
**false** → 误报。传递闭包只在 IR 发射期由 `ClassDescBuilder._expandIfaces` 算，不在符号表里。

> 注意不对称：**导入类型不受此影响**——生产端写进 zbc 的是 `_expandIfaces` 后的**传递闭包**，
> 所以导入类型的接口集比本地类型还全。缺口只伤本地类。

处置：PR-1 内补 `Z42InterfaceType` 的 base 列表 + 让 `Implements` 走闭包。**这是本 PR 的主要
未知量**，若代价超预期则退为「接口继承链暂按已知直接接口判定 + warning 不升 error」。

### PR-2 — `enum` + `new()`

- **`enum` 缺口在 parser**：`TypeParser._parseConstraint` 只特判 `New` / `Class` / `Struct`
  三个 token，`where T : enum` 落进 `_parseType()` 兜底 → `NamedType("enum")` → 被静默吞掉。
  补 `enum` special（同步 `WhereConstraint.Special` 的注释所列取值）
- **`new()` 零回归面**：全仓**没有一条**真实 `new()` 约束（只在 `z42c.syntax/tests/decl/decl_tests.z42`
  的 parser 期望字符串里，不走 typecheck）
- 语义定案（对齐运行期 `generics.rs`）：**无显式 ctor = 隐式默认构造，算满足 `new()`**；
  abstract 类**不**满足

### PR-4 — 诊断质量

- `WhereClause.Span` / `WhereConstraint.Span` **本来就有**，但 ConstraintChecker 全程用 `_noSpan()`
  发诊断（指向 `<sem>` 0:0）。换成真 Span
- 「约束名既不是类也不是接口」（拼写错误如 `where T : IFooo`）从静默改报 **E0443 UndefinedType**

> 新增诊断码用**字面量**发码（如 `"E0452"`），不引用 `DiagnosticCodes` 常量——沿用
> E0449/E0450/E0451 的既定手法，避开 core→semantics 新跨成员符号撞 F2 冷启动 stale-cache。
> 本轮预计只复用既有码（E0402 / E0443），不新增。

## 5. 落地强度：warning 探针（User 裁决）

PR-1 **先发 warning**，跑全仓 + 全 stdlib 拉出新增诊断清单对账，零误报后再翻 error（同 PR 内两个
commit）。理由：

- stdlib 热点全靠**基元 wrapper 归一**——接口挂在 `struct Int32 : IEquatable<int>` 上，实参写别名
  `int`。若 `int → Std.Int32` 归一有偏差，`Dictionary<int,int>` 直接编不过 = **整条自举链断掉**。
  这个风险读代码消不掉，必须实测。
- 去掉 `ArgCount` 过滤会让那 21 条泛型接口约束**一次性全部生效**，影响面需要量。

### 已知会新报错的点（User 已裁决处置）

`src/tests/types/struct_generic_container.z42` 的 `struct P` / `struct Tagged` 未实现
`IEquatable<>` 却做 `Dictionary` key。该用例立意是「blob 值 struct 走泛型边界装箱 + boxed
GetHashCode/Equals」，依赖 object 层 `Equals` 而非接口约束。→ **给两个 struct 补 `: IEquatable<>`**。

### 不构成风险的两条（已排除）

- **`impl IFoo for Bar` 外部 impl**：`InheritanceResolver` 已把 trait 并入 `target.InterfaceNames`，
  `Implements` 天然覆盖（`src/tests/generics/extern_impl_user_class.z42` 的 `Robot`/`IGreet` 安全）
- **跨包接口不可见**：`ImportedSymbolLoader` 的「z42c 不 import 接口进表（会破坏 byte-identical）」
  说的是**接口的定义**不进表；而校验 `T : IFoo` 只需要 T 的接口**名单**，那个是导入的
  （`nct.InterfaceNames = cl.Interfaces`）。**不碰 byte-identical**

## 6. 本轮不做（下一轮）

### PR-3 — ZbcWriter 置 flag 位

置 bit0/1/2/4/5，接活运行期那五个死分支。**依赖 PR-A**（否则 ZbcReader 错位）。
需同步给 `IrConstraintDesc` 加 `RequiresClass/RequiresStruct/BaseClass/RequiresCtor/RequiresEnum`
（现在只有 `Interfaces` + `TypeParamConstraint`）。**zbc on-disk 格式无需 bump**——位早已规约。

### PR-5 — 跨包约束持久化

**今天跨包泛型实例化的 where 约束 100% 不检查**——不只是本变更要补的三项，连已实现的
base-class / `class` / `struct` / 型参引用**也一样不查**。链路是封闭的：

- `symbols.ClassConstraints` 的**唯一**写入点是 `ConstraintChecker.Resolve`，只遍历本包 CU 的 `ClassDecl`
- 导入类型走 `ZpkgReader → TsigReconcile → ImportedSymbolLoader`，全程不碰 `ClassConstraints`
- ⇒ `Check` 第一行 `if (!symbols.HasConstraints(cname)) return;` 直接返回

补它需要：`ExportedClassZ` 加约束字段 + `TsigReconcile` 搬运 + `ImportedSymbolLoader` seed。

**为什么必须单独一轮**：给 `ExportedClassZ` 加字段 = 给 z42.ir（stdlib）加 API，而 z42c 源要用它
——正踩 [bootstrap-seed](../../../../.claude/rules/bootstrap-seed.md) 的**第二根轴（stdlib API 面）**，
必须「support 先行、晚一个 nightly 再 use」。`ImportedSymbolLoader` 里已有前人踩坑记录（新增
`ExportedClassZ` 字段导致 bootstrap 越界 E0401）。

**待定案**：`ClassConstraints` 的键是**裸类名**（`c.Name` / `inst.Def.Name()`），而导入侧是
arity-mangle 键 `Name$N`——两者对不上，需要先定键规则。

## 7. Deferred

| 条目 | 描述 |
|---|---|
| `where-constraint-future-type-arg-matching` | 接口约束的类型实参匹配（v1 只比裸名，见 D1） |
| `where-constraint-future-crosspkg` | 跨包约束持久化（PR-5，卡 nightly 节奏） |
| `where-constraint-future-runtime-flags` | ZbcWriter 置全 flag 位、接活运行期校验（PR-3） |
| `where-constraint-future-inferred-method-args` | 方法级约束只在**显式**写类型实参时校验（`MemberResolver._applyMethodTypeArgs` 的 `TypeArgCount == 0` 早退）；推断调用 `Max(a,b)` 不校验 |
| `where-constraint-future-toplevel-func` | 顶层 `FuncDecl` 的 `where` 不校验（`CheckMethod` 只接 `MethodDecl`） |
| `where-constraint-future-func-constraint` | func 类型约束（`where T: Func<int,R>`）—— E0422/E0423 已定义但从未发出。注意 `CallEmitter` 靠该约束把参数当 func 值走 `CallIndirect`，改动需谨慎 |
