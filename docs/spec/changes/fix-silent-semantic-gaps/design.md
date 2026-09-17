# Design: 修「编译通过但行为静默错误」的缺口

> Status: **IMPL 进行中**（2026-09-17）。三族根因调查全部完成，User 已确认按本文推进。
> 前置阅读：[proposal.md](proposal.md)（判据与归属核实）· [repro.md](repro.md)（最小复现与实测输出）

---

## 第三族 · 重载决议：适用性判据绕开了符号表

> 调查已完成（34 个探针 + 声明级普查）。**本族建议排在最前面实施**，理由见 §3.6。

### 3.1 根因：一处判据用了不带符号表的可赋性

`OverloadResolver._assignable`（`OverloadResolver.z42:376-391`）是重载适用性的唯一判据：

```z42
internal static bool _assignable(Z42Type arg, Z42Type param) {
    if (OverloadResolver._sameType(arg, param)) { return true; }          // 精确同名
    if (param != null && param.CanonName() == "object") { return true; }  // object 万能口
    if (arg == null) { return false; }
    return arg.IsAssignableTo(param);                                     // ← 无符号表
}
```

而 `Z42ClassType.IsAssignableTo`（`Z42Type.z42:306-323`）**自己的注释就写着继承链被延后了**：

```z42
// 同名（1A-1 不查继承链；subclass→base 赋值在 1C，需类表走 base 链）
if ((other is Z42ClassType) && this._name == other.Name()) { return true; }
```

`Z42InterfaceType.IsAssignableTo`（`:396-399`）只认同名接口 ⇒ class → interface 恒 false；
`Z42InstantiatedType`（`:554-558`）只认同名同实参 ⇒ 实例化泛型 → 基类 / 接口也不行。

**被漏掉的恰好是「需要类表」的那三条**：派生→基、类→接口、实例化泛型→基/接口。
而数值加宽（`PrimModel.CanWiden`）与 `object`（上面第 2 行的短路）**不需要类表**，所以它们过得去
——这就是「数值和 object 行、用户类层次不行」这个反直觉现象的全部原因。

> ⚠️ `_applicable:201` 上方那行注释「候选适用 ⟺ … 含**子类→基类** / prim→object 装箱 / **接口实现**」
> 是**假断言**，判据本体做不到。实施时一并订正。

### 3.2 「单签名能过」不是走了另一套判据，而是压根不走判据

`OverloadBinder.z42:696`：

```z42
if (na == 1) { return byArity[0]; }   // 同 arity 候选只有 1 个 → 直接返回
```

实参类型的错留给后面带符号表的 `CheckArgTypes` → `Conversion.Classify` 去判，所以单签名传子类放行。

**触发条件因此精确地是「同 arity、非 `params` 的候选 ≥ 2」。** 34 个探针的边界结论：

| 因素 | 是否影响 |
|---|---|
| 候选总数（2 个 / 3 个） | ❌ 不影响，同 arity ≥2 即坏 |
| 重载之间的类型关系 | ❌ 不影响 |
| 收者形态（静态 / 实例 / struct / 泛型实例化） | ❌ 全坏 |
| 本地 / 跨包 | ❌ 全坏 |
| **arity 是否相同** | ✅ **唯一因素**（arity 不同 ⇒ na==1 ⇒ 短路 ⇒ 正常） |

最有力的旁证：**`params` 展开形态一直是对的**——`OverloadBinder._resolveParamsOverload:771` 用的
**恰是**带符号表的 `TypeFactsTc._isAssignable(…, symbols)`。于是同一个方法组
`F(params A[])` + `F(string)`：传两个 `D` ✅ 正常选中，传一个 `D` ❌ 落进 arity 过滤 → na==1 短路 →
`E0402: cannot assign D to String`。**一个实参和两个实参结果不同，正是判据没统一的直接代价。**

### 3.3 为什么落 `E0401` 而不是 `E0425`

`Resolve`（`:157-198`）在适用候选数为 0 时直接返回 NoMatch（`:169`），永远到不了择优/歧义分支
⇒ `_resolveOverload` 返回 null ⇒ `MemberResolver` 发 `E0401`。
`E0425` 的 7 个发射点不是死码（有正向断言），只是**这条路进不去**。

### 3.4 本族顺带挖出的两条真静默错误 —— 这改变了本族的优先级

proposal 原本把本族归为「编译期拒绝（用户会立刻发现并绕行）」，优先级排在静默错误之后。
**调查推翻了这个判断**：同一处判据缺陷还造成两条货真价实的静默错误。

#### (a) ctor 按声明顺序静默选错，且诊断指错方向

`ConstructTyper._bindCtorArgs:197` 先取 `_ctorKey(cls, argc)`（**只按 arity 拼键**，primary 拿裸键）；
`Resolve` 与 `ResolveMapped` 都 NoMatch 时，**`ctorName` 保持 arity 兜底那个**：

| 声明序 | `new C(new D())`（`D : A`） | |
|---|---|---|
| `C(A)` 先 / `C(string)` 后 | ✅ 选中 `C(A)` | **碰巧对** |
| `C(B)` 先 / `C(A)` 后 | ❌ `E0402: cannot assign D to B (argument)` | |
| `C(string)` 先 / `C(A)` 后 | ❌ `E0402: cannot assign D to String (argument)` | |

⇒ 行为取决于**声明顺序**；且错误消息指着调用点说「你传错了」，而真相是「我选错了」。

#### (b) `object` 遮蔽更精确的基类形参 —— 唯一「今天能编译、修后会改选」的形状

| 形状 | 今天 | 应当 |
|---|---|---|
| `F(A)` + `F(object)`，传 `D` | 选 **`F(object)`** | `F(A)` |
| `F(IFoo)` + `F(object)`，传实现类 | 选 **`F(object)`** | `F(IFoo)` |
| `F(A)` + `F(object)`，传 `A` | 选 `F(A)` ✅ | — |

这是语义修正（C# 就是这样选），但**会改变字节** ⇒ 是本族唯一需要普查的形状（普查结果见 §3.5）。

### 3.5 修法

#### 方案 A（推荐）：把符号表贯通到判据层，用 `ImplicitRef` 窄口

```z42
internal static bool _assignable(Z42Type arg, Z42Type param, SymbolTable symbols) {
    if (OverloadResolver._sameType(arg, param)) { return true; }
    if (param != null && param.CanonName() == "object") { return true; }
    if (arg == null) { return false; }
    if (arg.IsAssignableTo(param)) { return true; }
    // ↓ 新增：只补需要类表的那几条（派生→基 / 类→接口 / inst→基·接口 / 数组→Array）
    if (symbols != null && Conversion.Classify(arg, param, symbols).Kind == ConvKind.ImplicitRef) { return true; }
    return false;
}
```

`Conversion.z42` 里这条路**本来就是齐的**，而且都带符号表：

| 分支 | 行 | 判据 |
|---|---|---|
| E class→class | `:182-186` | `symbols.IsSubclassOf(from, to)` |
| F class→interface | `:188-191` | `symbols.Implements(from, to)` |
| G inst→interface | `:193-196` | `symbols.Implements(Def, to)` |
| H inst→class | `:198-201` | `symbols.IsSubclassOf(Def, to)` |
| H2 inst→inst | `:208-216` | 同实参 + `IsSubclassOf` |

**为什么用 `ConvKind.ImplicitRef` 窄口而不直接换 `TypeFactsTc._isAssignable`：**
后者 = `Classify(...).ImplicitOk()`，白名单还含 `ImplicitNumeric` / `Boxing` / `UserImplicit`。
换过去会同时把数值门从 `PrimModel.CanWiden` 换成更严的 `_widensLossless`（诊断质量倒退，见 §3.7），
并把用户 `op_Implicit` 拉进重载适用性（语义扩张，属另一件事）。
**窄口 = 只补需要类表的那几条，数值 / object / 装箱 / 泛型擦除的行为逐位不变。**

**改动面**：同文件 9 个内部函数加 `symbols` 形参（`Resolve` `_applicable` `_betterThan` `_betterAtPos`
`Map` `ResolveMapped` `_mappedBetter` `_assignable` `_assignable2`）；生产调用点 **5 处、全都手里有符号表**
（`OverloadBinder:415` 需从 `:630` 把 `env` 带一层进来，`:664` `:706` 与 `ConstructTyper:222` `:233` 直接可用）；
单测调用点 4 处传 `null` 即保持现行为。

#### 🔴 `_assignable2` 必须同一批改，否则把 E0401 换成 E0425

C# 的「更派生的形参更优」在这套实现里是**自动推出来**的（`_betterAtPos:234-238`：`px` 可赋到 `py`
而非反向 ⇒ `px` 更具体），**但前提是 `_assignable2` 也认类层次**：

| 形状 | 只改 `_assignable` | 两个都改 |
|---|---|---|
| `F(A)`+`F(string)` / `D` | ✅ 唯一适用 → 选中 | ✅ |
| `F(A)`+`F(object)` / `D` | ✅（靠 object 短路救回） | ✅ |
| **`F(A)`+`F(M)` / `D`（`D:M:A`）** | ❌ 两者适用但不可比 → **E0425** | ✅ 选 `F(M)` |
| `F(A)`+`F(IFoo)` / `D:A,IFoo` | E0425 | E0425（**与 C# CS0121 一致，正确**） |

只改一半会让「多层继承 + 重载」从「找不到」变成「歧义」——用户体验没变好，还多一种错法。

#### 已否决的两个更小方案

| 方案 | 为什么不用 |
|---|---|
| 只在 `Resolve` 的 NoMatch 分支用带符号表的判据重算一遍 | 留下上表第三行的 E0425 陷阱；且命名实参 / 默认值走的 `Map` / `ResolveMapped` 路径修不到 |
| 直接把 `_assignable` 换成 `TypeFactsTc._isAssignable` | 顺带改数值门与用户转换 ⇒ 诊断倒退 + 语义扩张，影响面不可控 |

### 3.6 影响面：实测自举与 stdlib **零改变**

声明级普查（`src/compiler/**` + `src/libraries/**`，排除 tests/bench）：

| 目标 | 同名重载组 | **同 arity ≥2**（唯一会走适用性判据的形状） | 含非内建形参 | 同 arity ≥2 的 ctor 组 |
|---|---|---|---|---|
| `src/compiler/`（自举编译器） | 4 | **0** | 0 | 0 |
| `src/libraries/`（stdlib） | 60 | 13 | **0** | 0 |

- **编译器自己的源码永远不进适用性判据** ⇒ 自举字节不可能漂。
- stdlib 那 13 组全部只用内建类型：`Assert.Greater/GreaterOrEqual/InRange/Less/LessOrEqual`、
  `String.IndexOf/LastIndexOf/Split/Trim/TrimEnd/TrimStart`、`StringBuilder.Append/Insert`。
  其中 `StringBuilder.Append/1` 与 `Insert/2` 含 `object`，是 §3.4(b) 的候选形状，
  但**同组里没有用户类 / 接口形参** ⇒ 「object 被更精确的基类抢走」无从发生。
  ⇒ **stdlib 的重载选择一位不变。**
- **反向风险**：今天任何「同 arity ≥2 + 子类 / 实现类实参」都是 E0401 ⇒ 改后无论变 Resolved 还是 E0425，
  都是从「错」变到「对或更准的错」。唯一新增歧义形状 `F(Base)`+`F(IFoo)` 传 `D:Base,IFoo` 与
  C# CS0121 一致，且诊断消息已就绪（`OverloadBinder:709`「add an explicit cast to disambiguate」）。

**跨包能做全**：TSIG 里基类名（`ClassExtractor.z42:373` → `ImportedSymbolLoader.z42:191-194`）
与接口表（`:234-237`，且是生产侧 `_expandIfaces` 后的**传递闭包**）都有，实测三层跨包链赋值可通。

⚠️ **唯一隐患是惰性加载**：`IsSubclassOf:348-353` 靠 `GetClass(cur)` 逐级取，链中某一级类没被载进
`r.Classes` 时返 null → 循环中断 → 返 false。**实施时必须加一条「链中断」探针用例**
（引用一个中间基类不在 `using` 世界里的跨包类）。

### 3.7 明确登记为 Deferred：`_widensLossless` 未接入择优

`OverloadResolver.z42:9` 的原话「v1 不做 int→long→double 隐式数值排序（并列即歧义）」**已不成立**：

- `F(long)` + `F(double)` 传 `int` → 选中 `F(long)`（不是歧义）。择优事实上用了
  `PrimModel.CanWiden` 那张表（`PrimModel.z42:154`）。
- 而 `Conversion._widensLossless`（`:326-338`）是**收紧后**的表（`i64→f32/f64` 明确 false）。两张表不一致。
- 后果不止排序：`F(float)` + `F(string)` 传 `long` → 决议选中 `F(float)`，紧接着 `CheckArgTypes`
  用 `Conversion` 判 `Int64→Single` 是 `ExplicitNumeric` → 报 `E0439`。
  ⇒ **决议的门比实参检查的门宽，可以选中一个随后必被拒的候选。**

**本 PR 不接**，理由：接进去会把 `E0439`（"are you missing a cast?"，有指引）降级成
`E0401`（零指引），除非同时改「无适用候选但有 arity 匹配候选时该报什么」的诊断策略——那是另一条设计决策。
且两表不一致**不产生静默错误**，与本 change 的「静默错误优先」判据不同档。

**但要顺手订正 `OverloadResolver.z42:7-9` 与 `:200` 两处过期 / 假断言注释。**

### 3.8 分期建议：**本族单独一个 PR，且排在最前**

1. **它是三族里唯一「自举与 stdlib 实测零影响」的一族**（§3.6 有逐组名单作证）。
   先合它把不动点稳住，给后两族腾出干净的字节基线——后两族一定会扰动字节，混在一起就分不清谁造成的漂移。
2. **不需要 zbc bump、不需要新诊断码、不碰 runtime** ⇒ 与另两族没有任何共享改动面，
   合并只会放大回滚半径（`ref` 族一旦 zbc bump 出问题要整包回退）。
3. **PR 内部分 4 个 commit**：

| commit | 内容 | 预期字节影响 |
|---|---|---|
| 1 | `symbols` 贯通 + `ImplicitRef` 窄口（`_assignable` / `_assignable2` 同改）+ 判据层单测 | 零漂移 |
| 2 | `object` 遮蔽的行为变更确认（§3.4b）+ 端到端用例 | **唯一会改既有选择的一步，独立可回退** |
| 3 | ctor 兜底（§3.4a）——动的是 `ConstructTyper` 的兼容缝，单独隔离 | 需普查 |
| 4 | 订正两处过期 / 假断言注释 + `_widensLossless` 登记 Deferred | 无 |

### 3.9 测试方案

**正例（端到端，`src/tests/inheritance/overload_by_class_hierarchy.z42`）** ——
归属判据「这条断言在描述谁的契约」= 语言的派发规则；同目录已有 `virtual_override_overload_by_type.z42` 先例：

① 子类→基类 ② 实现类→接口 ③ 多层继承 ④ **择优：更派生者胜**（`F(A)`+`F(M)` 传 `D:M:A` → `F(M)`，
守 §3.5 那张表的第三行）⑤ 精确 > 上转 ⑥ 与数值加宽混合 ⑦ **与 `object` 混合**（行为变更的锚）
⑧ 接口继承 ⑨ 三种收者形态各一 ⑩ 命名实参 + 默认值路径（走 `ResolveMapped` / `Map`）
⑪ `params` 正常 / 展开形态回归（今天已对，防改坏）

**跨包（`src/tests/cross-zpkg/`）**：⑫ 基类在依赖包 ⑬ 基类与派生类都在依赖包
⑭ 🔴 **惰性加载链中断**（§3.6 的隐患探针）⑮ 跨包接口实现

**负例（`z42c.semantics/tests/typecheck/`，用 `SemanticDump.FirstErrorCode`）**：
⑯ `F(Base)`+`F(IFoo)` 传 `D:Base,IFoo` → **E0425**（不再是 E0401）
⑰ 不相关类型仍报 E0401 ⑱ 单签名不适用仍报 E0402（防 na==1 短路被改坏）

**判据层单测（`tests/overload/overload_tests.z42`）**：真 `SymbolTable` + `A` / `D:A` 两个类，
直断 `_applicable` 与 `_betterAtPos` 的三态——这是唯一能把「择优方向」钉死的层次。

**门禁提醒**：本缺口在语义层，与 StackAlloc/Inline/Devirt 无关，**不需要** `opt_all` sidecar；
但自举需 gen1/gen2/gen3 字节比对，用 §3.6 预测的「零漂移」当阴性对照。

---

## 第一族 · `ref` 传址

> 调查已完成（`--dump-ir` + 端到端探针）。

### 1.1 根因：把 lvalue「求值成临时寄存器」再取那个临时槽的地址

`--dump-ir` 把机理摊得很清楚：

```
%4 = array_get %2[%3]          ← 先把元素值读进临时寄存器
%5 = load_local_addr %4        ← 再取「临时寄存器」的地址 ⇒ 写回落到 %4
%6 = call @Inc(%5)

%9 = field_get %8.f            ← 同上
%10 = load_local_addr %9

%13 = const.i64 30
%14 = load_local_addr %13      ← 局部：%13 就是 v 的归属寄存器 ⇒ 正确
```

`ExprEmitter.z42:110-123` 对 `BoundRefArg` 一律 `slotReg = this.Emit(ra.Inner)`。
对 `BoundIdent` 走 `_lookupIdent` 返回**局部的归属寄存器** ⇒ 地址正确；
对 `BoundIndex` / `BoundMember` / `BoundStaticGet`，`Emit` 是**一次读取**、返回**新分配的临时寄存器**
⇒ 地址指向临时槽，写回打在临时槽上，函数返回后无人再读。

**`BoundRefArg` 不需要新加形态标签** —— `Inner` 的运行时类型本身就是判据。

### 1.2 lvalue 形态比预期多，其中两种必须拒绝

| 源形态 | 当前发射 | 目标 |
|---|---|---|
| `ref v`（局部 / 形参） | `load_local_addr %slot` | ✅ 已正确 |
| `ref arr[i]` | `array_get` → `load_local_addr` | `LoadElemAddr`（`0xA1`） |
| `ref obj.f` / `ref this.f` / 裸 `ref f` | `field_get` → `load_local_addr` | `LoadFieldAddr`（`0xA2`） |
| `ref w.a[0]`（字段里的数组） | `field_get` + `array_get` → `load_local_addr` | `LoadElemAddr`（arr 来自 field_get 的 reg，天然可用） |
| **`ref C.sf`（静态字段）** | `static_get` → `load_local_addr` | ⛔ **VM 没有 `RefKind::Static`** ⇒ 本 PR 明确**拒绝** |
| `ref p.Q`（属性） | `vcall get_Q()` → `load_local_addr` | ⛔ 无存储 ⇒ 拒绝 |
| `ref q.X`（blob struct 字段） | `struct_fget_prim` → `load_local_addr` | ⛔ blob 在 arena，无对应 `RefKind` ⇒ 拒绝 |

判据可直接复用现成谓词（`AccessEmitter.z42:93-160` 的 `_isBlobStruct` / `_isInlineStructFieldRoot` /
`_isPropWithSetter`），拆解 lvalue 的机械也是现成的（`:141-156` 已把 `BoundIndex` 拆成
`(arr reg, idx reg)`，只需把末端的 `ArraySet` 换成 `LoadElemAddr`）。

### 1.3 写回机制加了新指令后是对的 —— 但有一条必须同 PR 修的潜伏 bug

出口 copy-out（`exec_support.rs:295-302`，四条返回路径都调，含未捕获抛出）
**不按 `RefKind` 分流**，分流在 `store_thru_ref`（`frame.rs:295-353`）内。三条指令的载荷差异：

| 指令 | `RefKind` | 生命周期 |
|---|---|---|
| `0xA0 LoadLocalAddr` | `Stack { frame_idx, slot }` | 按帧栈索引寻址，**依赖目标帧仍在栈上** |
| `0xA1 LoadElemAddr` | `Array { gc_ref, idx }` | **持 GcRef**，与帧无关 |
| `0xA2 LoadFieldAddr` | `Field { gc_ref, field_name }` | 同上 |

好消息（**一行都不用写**）：
- `store_thru_ref` / `deref_ref` 的 Array / Field 两臂**已实现完整**；
- transient arena **已是 GC 根**且 `scan_roots` 显式 visit `RefKind::Array/Field` 的 `gc_ref`
  （`transient_arena.rs:126-140`）；
- Array / Field 的 `RefKind` **不依赖帧栈**，比 `Stack` 更安全；
- JIT 的三道 `Value::Ref` 逃生门是 `matches!(.., Value::Ref{..})`、**RefKind 无关** ⇒ 新形态自动被拦。

#### 🔴 1.3a `RefKind::Field` 写回缺 GC 写屏障 —— 一旦开始发射 `0xA2` 就变成可达路径

`frame.rs:336-351`：

```rust
RefKind::Field { gc_ref, field_name } => {
    let mut obj = gc_ref.borrow_mut();
    ...
    // (No GC write barrier here — parity with the pre-PR-2 store-through-ref path.)
    Some(slot) => { obj.set_field_value(slot, &val); Ok(()) }
```

而正上方 Array 臂有 `write_barrier_array_elem`（2026-09-10 `fix-missing-array-write-barriers` 补的），
`exec_object.rs:399/410/417` 的 `FieldSet` 三处也都打屏障。

**分代 GC 已是默认** ⇒ `ref obj.f` 写回一个 young 引用进 old 对象会**漏记 remembered set** ⇒ 过早回收。
**必须与本族同 PR 修（或先单独作为 bug fix 合掉）**，否则是**拿一个静默丢写换一个静默悬垂**。

#### ⚠️ 1.3b 别名语义偏差（架构 E 固有，非本次引入）

copy-out 是「出口用参数寄存器终值覆盖目标位置」，不是真别名。于是：

```z42
void F(ref int x, int[] a) { a[0] = 99; }   // 调用 F(ref arr[0], arr)
```

出口会把 `x` 的入口值写回 `arr[0]`，**吃掉 99**。且 callee 即使从不写 `x`，出口也**无条件**写一次
（`exec_support.rs:296-301` 无 dirty 位）。这条对 `Stack` 也成立，只是 caller 帧无法被第二条路径改到
所以今天看不见；Array / Field 把它**暴露出来**。是既有设计的已知代价，**必须写进语言文档并用测试钉住**。

#### ⚠️ 1.3c 这三条运行时路径**从未被执行过**

`grep -rn LoadElemAddr src/runtime/` 的全部命中都在实现文件里，**零测试**。
`load_elem_addr` / `load_field_addr` 自 **2026-05-05**（`cb61cc072`）落地以来是**纯死代码**。
「运行时侧实现完整」是**读码结论，不是实测结论** ⇒ PR 里必须补 Rust 手搓字节码单测
（`--lib` 且**必须 debug 跑**——`--release` 会把 `debug_assert!` 编掉，本地全绿 CI 四 OS 全红）。

### 1.4 调用点漏写 `ref` 不报错：修法要先补两处「修饰符信息不存在」

**parser 有两处塌缩，不是一处**：

| 位置 | 现状 |
|---|---|
| 形参侧 `MemberParser.z42:340` | `Ref \|\| Out \|\| In` → `isRef = true` → `Param.IsRef` 单布尔 |
| **调用点侧 `ExprParser.z42:238-248`** | `RefArgExpr(this._parseExpr(0), refTok.Span)` —— **`refTok.Kind` 被丢掉，只留 `.Span`** |

**语义层完全瞎**：`Z42FuncType`（`Z42Type.z42:444-457`）有 `ParamTypes` / `ParamsFrom` /
`ParamDefaults` / `ParamCallers` / `ParamNames`，**没有任何 ref 修饰位**。
唯一能拿到 ref-ness 的路径是 `ms.Decl.Params[i].IsRef`，而 `ms.HasDecl` 只对**本包本地**方法为真
⇒ **跨包方法的 ref-ness 在今天的 z42 里根本不存在**。

**zpkg 里一个字节都没有**：`ExportedParamZ` = `{Name, TypeName, DefaultBlob, CallerKind}`；
SIGS 每参写 `type:u32 + name:u32 + default_kind:u8 + payload`，**没有 flags 字节、也没有预留空槽**。

⇒ 修法：`Param.IsRef` → `RefMod` 枚举（`ParamRefMod.None/Ref/Out/In`，互斥态不可表示）；
`Z42FuncType` 加 `ParamRefMods`；`RefArgExpr` / `BoundRefArg` 加 `Mod`；
校验点放 `OverloadBinder.BindArgsToSignature` 里 `_checkOneArg` 旁（不放 parser——它不知道被调方签名；
不放决议前——修饰符一旦入键就是不同重载，放决议前会让候选过滤与诊断打架）。

**`IsRef` 的真实消费点只有 4 个**（其余 `StructLayout.IsRef(int)` 是同名无关函数）：
`MemberParser.z42:360`（写入）· `:371`（`params` 冲突检查 E0208）· `CtorInheritance.z42:185`（合成 ctor 拷贝）
· 🔴 **`ForwardGenerator.z42:384`** —— 今天 `if (p.IsRef) { sig += "ref "; }`，
对 `out` / `in` 形参**生成错关键字**，是本族修完后**第一批会暴露的红**，必须同 PR 修。

### 1.5 格式影响：zbc 1.42 → 1.43、zpkg 0.47 → 0.48，共 9 步

依据 `version-bumping.md:43`「新 opcode ⇒ 单次 commit 必须同步 5 处」+「zbc minor bump 必须同步 bump zpkg minor」。

**仓库先例完全同型**：`ZbcFormat.z42:58-60` 明写「A-support 只加编解码/执行能力、**不发射** → 暂不 bump
（**bump 在 A-use**）」；zbc 1.31 就是「A-support 已加编解码/执行，同时 z42c **开始 emit** `0xC0–0xC3`」。

9 步：① `ZbcFormat.z42` Minor++ + `ZbcInstr`（写）+ `ZbcReaderInstr`（读，**含 REGT 回填分支，别只改解码**）
② Rust `versions.rs:174` 常量 + **钉值单测** `zbc_reader_tests.rs`（只在 `cargo test --lib` 跑，
`xtask test` 不含——doc 记着 2026-09-13 差点漏掉）③ `formats/zbc.md` changelog
+ **顺带补 opcode 表**（`:234` 只有 `0xA0`，`0xA1`/`0xA2` 从来没进过那张表）
④ regen 6 个 zbc fixture（CI 有硬门 `refresh-format-fixtures`）⑤ 重截 `zbc_tests.z42` 的内嵌 hex
⑥⑦⑧⑨ zpkg 侧 4 步（`ZpkgWriter.z42:36` / `versions.rs:294` / `zpkg.md` / regen 4 个 fixture）。

✅ 已核实那 6 个 zbc fixture 的 `source.z42` **都不含 ref/out/in** ⇒ 字节 delta 只来自 header minor 字段。

### 1.6 自举安全 —— 三条依据

1. **support 是 2026-05-05 `cb61cc072`，不是 08-24**（`072ca0fd1` 只是模块拆分的搬家）
   ⇒ 已发布 4 个多月，远超「support 先行、晚一个 nightly 再 use」的窗口。
2. **格式 bump 维度已由两代自举根治**：`ci-bootstrap` 检测种子 header minor 与源码不等时自动走
   旧 VM 跑 Gen1+Gen2（实测 0.25→0.30 连续 5+ 次真实 bump 全绿）。
3. **「use 新形态」的约束当前已满足**：`grep -rEn "[(,] *(ref|out|in) +[A-Za-z_][A-Za-z0-9_]*[.\[]"`
   在 `src/libraries` `src/compiler` `scripts` `xtask` `src/tests` **零命中** ——
   全仓用 `ref` 的地方只有 `scripts/test/*.z42` 的 13 处，全是**裸局部 / 形参**，走 `0xA0` 老路。

⚠️ **反向纪律**：修完之后也**不要立刻**在 z42c / xtask / stdlib 源码里用 `ref arr[i]` / `ref obj.f`
——那属「use 新语义」，要等含修复的 nightly 发布（轴 ③；6 个自依赖库的预建豁免只覆盖
「新 stdlib API」，**不覆盖「新代码生成语义」**）。

### 1.7 爆炸半径

| 维度 | 结论 |
|---|---|
| committed 字节基线 | **不受影响**。`src/tests/refs/` 4 个用例只有 `source.z42`，`.zbc` 是 gitignored 产物；全仓 committed `.zbc` 只有 7 个且**都不含 ref/out/in** |
| 现有单测 | `codegen_tests.z42:502-510` 断言 `ref c`（局部形态）产出 `load_local_addr` ⇒ 分流后**一字不变** |
| JIT | **净影响为零**。三条取址指令**早已全在** unsupported 表（`unsupported.rs:46-48`）⇒ `ref arr[0]` 的调用方**今天就已不可 JIT**。`analysis.rs:203/234` 的寄存器类型推断也已就位 |
| stdlib | **零暴露**（`z42.core` 与其它库 ref/out/in 各 0 处） |
| ⚠️ **xtask** | 13 处 `ref`，**全部已正确写了 `ref` 关键字** ⇒ 严格化对它们是绿的。但 xtask 是 `ci-bootstrap` 的冷启动路径（种子 stdlib + 种子 z42c 编当前源），**本地不可验** ⇒ B 的规则必须先用探针把这 13 处形状逐个覆盖 |
| 逃逸分析 | `IrEscapeAnalysis._markEscaping` 的**兜底 ④**「全部读操作数逃逸」自动吞下两条新指令，**恰好是必需的保守行为**（`load_elem_addr` 只接 `Value::Array`，栈分配数组是 `StackArray` ⇒ 必须逃逸）。只要 `ReadCount`/`ReadAt` 实现正确就零额外代码 —— 但**必须挂 `opt_all` sidecar 验证**（默认优化集关掉 StackAlloc，等于没测；见「优化门禁空转」教训） |

### 1.8 分期建议

| PR | 内容 | 理由 |
|---|---|---|
| **PR-0**（推荐先行） | `frame.rs:336-351` 补 `RefKind::Field` 写屏障 | 纯运行时 bug fix、不改格式、不改编译器，可立刻合。**阴性对照先做**：不打屏障时 `ref_field_writebarrier` 用例必须判红 |
| **PR-1** | `z42.ir` 两条新指令 + `ExprEmitter` 分流 + 三种形态拒绝 + 格式 bump 9 步 + Rust 单测 | 发新 opcode ⇒ bump ⇒ 9 步同 commit（strict-pin 硬要求）；写/读两侧天生对称，分开必坏 cache。**`MangleKey` 加修饰符位建议搭这趟车**（属「键字符串内容变」，与 zbc 1.38 同型，共用同一次 bump 零额外格式代价） |
| **PR-2** | `Param.RefMod` 三分 + 调用点校验 + `ForwardGenerator` 修正 | 一改四个消费点同时失配；B 的规则依赖 `ParamRefMods` 与调用点 `Mod`。**唯一会碰 xtask 冷启动的 PR，push 后必须盯 CI** |
| PR-3 | lvalue 限制 / 跨修饰符类型严格 / lambda 捕获禁止 / `in` 只读 | 纯诊断，零字节漂移，可再拆 |
| PR-4 | `ref C.sf`：新增 `RefKind::Static` + `0xA3` | 独立运行时语义 + 又一次 bump；PR-1 先报错拦住即可 |
| PR-5 | `out` definite-assignment 分析 | 全新 pass，仓库无先例，最大一块 |
| PR-6 | ref 位进 SIGS wire，让**跨包**调用也被校验 | 又一次 bump；stdlib 零 ref 形参 ⇒ 收益低。**但 cross-zpkg 今天完全无校验，设计文档要写明这个洞** |

## 第二族 · struct 存储与初始化

> 调查已完成（20 个探针实测）。**本族的两条修法各只有约一行，但有一条硬阻断。**

### 2.1 缺口 4（静态 struct 字段）：正确机制已在仓里，只是一条 codegen 路径漏挂

**反证是这条的钥匙**：给类加一个**空的** `static Holder() { }`，同一段代码立刻正确输出。

原因：`DeclBinder.z42:176-186` 按「有无静态 ctor」分流字段初始化器：

| 分支 | 去向 | 结果 |
|---|---|---|
| **有**静态 ctor | 初始化器注入静态 ctor 体首 → 普通 `BoundAssign` → `AccessEmitter._emitStaticStore` | ✅ **装箱，正确** |
| **无**静态 ctor | `model.AddStaticInit` → `FunctionEmitter.EmitStaticInit` | ❌ **漏装箱** |

对称性漏项精确到两行：

```z42
// FunctionEmitter.z42:333  —— 裸 v
this._ctx.Emit(new StaticSetInstr(fq, v));

// AccessEmitter.z42:308   —— 有装箱
TypedReg sv = this._boxIfStaticStruct(val, st.Type());   // add-static-struct-bytecization
```

于是存进静态槽的是 `__static_init__` 帧里的 `Value::StructRef{idx, frame_id}`。arena 是 per-context
**LIFO**（`pop_frame` → `StructArena::truncate`，`struct_arena.rs:73-78`），`__static_init__` 一返回 slot
就被截掉，`frame_id` staleness 守卫在下次 deref 时抛 `value-struct lifetime unsound`。

**根本矛盾确实存在**（arena 帧级 LIFO vs 静态字段模块级），但 `add-static-struct-bytecization`
**早已用「装箱进堆」解决了它**——机制、读路径（`ExprEmitter.z42:155-164` 整读拆箱 /
`AccessEmitter.z42:383-386` 叶子根直取盒）、golden 全部现成。

> ⚠️ 我在调查早期判断这是「机制层面的矛盾、不可小修」——**结论错了**。正确表述是：
> 矛盾真实存在，但解法已实现且在跑，缺的只是 `EmitStaticInit` 这一条路径接上它。

**为什么测试没抓到**：现有 golden `src/tests/types/struct_static_field.z42` 声明的是
`public static Point P;`（**无初始化器**），在 `Main()` 里赋值 ⇒ 只走已装箱的那条路。
**初始化器路径零覆盖。**

**同族第二条**：静态 auto-property 初始化器走同一个 `AddStaticInit`，但崩点不同 ——
`Holder.get_White ... takes 0 physical argument(s), the call passes 1`，是**合成 auto-prop 访问器桩不
sret-aware**，与 §2.2 的第二条缺口同源。

**修法**：`FunctionEmitter.z42:331-334` 发 `StaticSetInstr` 前对 blob struct 值走 `_boxIfStaticStruct`
（现为 `AccessEmitter` private，需按 `ExprEmitter.z42:431` 的既有手法加一层 `internal` 转发）。
判据用 `model.SiInit[i].Type()`。
⛔ **不要用「加个空静态 ctor」当修法** —— 那会把 `beforefieldinit` 语义绑死，
`DeclBinder.z42:182-186` 的注释明确记了为什么这两条路要分开。

### 2.2 缺口 5（struct 自动属性）：🔴 我的原诊断是错的 —— 后备字段**在**布局里

原判断是「后备字段没进 `StructLayout`，offset 保持未初始化值」。**实测否掉了**：

- `MemberCollector.z42:206`：`if (pfHasBacking) { ... ct.AddOwnField("__prop_" + pd.Name, ...); }`，
  而 `StructLayout.BuildFromSymbols` 正是从 `ct.OwnFieldNames` 建布局
- 探针：`struct M { int X {get;set;} int y; }` 里 `this.y = 42` 能正确读回 **42**
  ⇒ `__prop_X` 确实占了 offset 0 的 4 字节，`y` 才拿到 offset 4
- 单 auto-prop 的 struct（FieldCount=1）**不崩**（落到缺口 3 的引用语义）；双 auto-prop（FieldCount=2）才崩

⇒ **`4294967295` 不是「没进布局」，而是「查布局时用了错的名字」。**

#### (a) 写路径缺属性判据（不对称漏项）

`AccessEmitter._emitBlobFieldSet:283`：

```z42
int off = this._structChainOffset(tm.Target) + this._ctx.Gen.Layouts.FieldByteOffset(cont, tm.MemberName);
```

`FieldByteOffset(P, "X")` —— 布局里只有 `__prop_X` ⇒ 返回 **-1** ⇒ 烘焙进
`StructFieldSetPrimInstr` 的立即数，编码成 u32 就是 **4294967295** ⇒ runtime 的
`layout.ref_index(byte_off)` 落空 ⇒ `struct ref leaf at byte offset 4294967295 not in type layout`。

**读侧早已修过**（`fix-struct-property-getter`，`AccessEmitter.z42:179-180` 先查
`sct.Methods.ContainsKey("get_" + m.MemberName)`），而 `AccessEmitter.z42:176-178` 的注释
**精确描述了这个 `4294967295` 症状**——只是描述的是读侧。**写侧的对称判据从未加上。**
同一漏项还在 `_structChainOffset:400-404`、`PatternEmitter.z42:268`、`RecordSynth.z42:318`
（都直接用源名查偏移）。

#### (b) 合成访问器桩体在 struct 上根本不可执行 —— 第二条独立缺口

非 extern auto-property 的 `get_X`/`set_X` 是合成桩（`IrGenMemberEmitter.z42:98-116`），
桩体形态是 `field.get %0 @__prop_X → %1; ret %1`（`IrGen.z42:284-289`）。
`FieldGet`/`FieldSet` 只认堆 `ScriptObject`，而 struct 方法的 `this`（reg0）是 `Value::StructRef`
⇒ 实测 `FieldGet: not an object or known value type, got StructRef { idx: 0 } (field '__prop_X')`。

**所以只修 (a) 之后读 `m.X` 仍会崩** —— 它会被正确路由到 `get_X` 的静态 Call，然后死在桩体里。
**(a) 与 (b) 必须一起修。** (b) 的静态变体就是 §2.1 末尾那条（sret arity mismatch）。

**修法**：(a) 查布局前把成员名经 `_propBackingName`（`AccessEmitter.z42:323-328`，**已存在**）
翻成 `__prop_X`；auto-prop 在 struct 上应**直接读写后备字节**（零 Call、零桩），比路由到 `get_X` 更快。
(b) owner 是 blob struct 时桩体改发 `StructFieldGetPrim/SetPrim(reg0, FieldByteOffset(S,"__prop_X"), tag)`，
返回 blob struct 时 sret-aware —— **这条不能省**，接口派发 / 反射 / 泛型仍会调到桩。

**兜底**（若 (b) 的 sret 改造超出本轮 scope）：只做 (a) 的名字翻译，同时对
「blob struct 的 auto-prop 被经 `get_X`/`set_X` 派发」补一条编译期诊断（新码 `E0470`），
至少把静默错值 / 晦涩 runtime 崩换成清晰诊断。

> 顺带：`src/tests/types/struct_property_getter.z42` 的头注释声称覆盖 auto-property，
> **实测文件里一个 auto-prop 都没有**（只有计算属性）。要补真实用例。

### 2.3 附带缺口：`default(任何值 struct)` 都崩

`ExprEmitter.z42:206-248` 的 `BoundDefault` 按 `ToIrType` 的 tag 分派（Bool/I32/I64/F64/Char），
blob struct 的 tag 是 `IrType.Ref` ⇒ 落到 `:245-247` 的 `ConstNullInstr`；
上层 `Two t = default(Two)` 发 `StructAlloc t` + `StructCopy(t, null)` ⇒
`StructCopy src: expected a struct value (StructRef), got Null`。

**修法约 5 行**：在 `BoundDefault` 分支**最前面**加一条 blob-struct 臂，发
`StructAllocInstr(dst, QualifyClass(name), StructSize(name))`。arena `alloc` 本来就零初始化
（`struct_arena.rs:82-89`：`bytes` 全零、`refs` 全 `Null`），这正是 struct 的默认值定义。
顺带收掉 `Guid.z42:15-20` 的注释与 `Guid.Empty()` 的 workaround。

### 2.4 缺口 3（单字段 struct）已移出本 change，但调查结论要留档

见 [proposal.md](proposal.md) 的「归属核实」——归 `unify-value-types` Phase 4。以下三条留给那边：

**(a) 它比「赋值串味」严重得多 —— 值类型契约整体缺失**（全部实测）：

| 契约 | 单字段 struct 实测 | 两字段对照 |
|---|---|---|
| 赋值复制 | `b=a; b.x=99` → `a.x=99` ✗ | `c.x=1` ✅ |
| 传参 copy-in | `Bump(a)` 后 `a.x=100` ✗ | `a.x=1` ✅ |
| 装箱独立 | `(One)o` 改副本 → 原值也变 ✗ | ✅ |
| `==` 值相等 | `new One(1) == new One(1)` → **false** ✗ | true ✅ |
| 类字段零初始化 | `h.o == null` ✗ | `h.o.x=0` ✅ |
| `GetType()` | **`VCall: function One.GetType not found`** 崩 ✗ | ✅ |

⇒ 「一个类型看起来是值类型、行为处处像引用类型」，且**全是静默给错值**。

**(b) 🔴 硬阻断：泛型静态抽象运算符 × sret。**
`src/tests/operators/static_abstract_operator.z42:13` 的 `struct Money : INumber { long Cents; }` 是
**单字段**，**今天绿恰恰因为它走引用语义**。实测翻成 blob 后：
`T Add<T>(T a, T b) where T: INumber { return a + b; }` 崩在
`Money.op_Add ... takes 3 physical argument(s), the call passes 2`
—— 返回 blob struct 的运算符经**泛型约束派发**时没追加 sret。而**直接** `a + b`（非泛型）是好的。
⇒ **这是一条先决 bug，不修它，放宽判据必然把这个 golden 变红。**

**(c) 一个大幅降险的切分建议**（供 Phase 4 参考）：
把「单**引用**叶子 / 单**嵌套 struct** 叶子」走放宽 blob，「单**基元**叶子」留给 Phase 4 的标量塌缩。
这样 `GCHandle`（单 `i64`）**本轮不动** ⇒ 整体避开那 5 个 Rust builtin 的改造
（`make_gc_handle` 产 `Value::Object`、`extract_gc_handle_slot` 只认 `Value::Object`，
收到 `StructRef` 会**静默返回 slot 0**当「未分配」）⇒ `src/tests/gc/gc_handle.z42` 也不会红。
判据从 `FieldCount >= 2` 变成 `FieldCount >= 2 || (FieldCount == 1 && !单基元叶子)`。

### 2.5 格式与自举：本族全部改动**格式中立**

- **无新指令**：缺口 4 用现成 `__box_struct`（`Builtin` opcode）；`default` 用现成 `StructAlloc`（`0xC0`）；
  缺口 5 用现成 `StructFieldGetPrim/SetPrim`（`0xC2`/`0xC3`）。
- **无 section 字段语义变化**：`StructLayout` 进 zbc 的门**不是** `IsBlobStruct`，而是
  `Kind == "struct"`（`ClassDescBuilder.z42:277-286` 填值、`ZbcWriter.z42:392-403` 按 `Flags & 4` emit、
  `ZbcReader.z42:415-425` 同 gate）。**单字段 struct 的 `StructSize` + 引用位图今天就已写进 TYPE 段**，
  只是编译器不发 blob 指令去用它。
- ⇒ **不需要 zbc / zpkg bump。** 与第一族形成对比（那族因新 opcode 必须 bump）。
- **自举零风险**：`src/compiler/**` 与 `src/libraries/z42c.*` / `z42.ir` 声明的 struct 数量 = **0**
  （命中全在 `tests/` 的源码字符串 fixture 里）。它们消费的 stdlib struct（`ValueTuple*` /
  `KeyValuePair` / 两个 Enumerator）字段数都 ≥2、**今天就已是 blob** ⇒ self-host 不动点应保持。

### 2.6 分期建议

| PR | 内容 | 依赖 | 规模 |
|---|---|---|---|
| **PR-A**（推荐最先） | 缺口 4：`EmitStaticInit` 挂装箱 + sret-aware 静态 auto-prop 桩 + 扩 `struct_static_field.z42`（初始化器 / `static readonly` / 有无静态 ctor 两路） | 无 | **~1 行主修** + 一个桩修复 |
| **PR-B** | `default(blob struct)` → `StructAlloc` | 无 | **~5 行** |
| **PR-C** | 缺口 5：(a) 名字翻译（含 `_structChainOffset` / `PatternEmitter` / `RecordSynth` 三处同漏）+ (b) 桩体 struct-aware | 与 PR-A 的桩修复轻度重叠，建议 sret 部分并入 PR-A | 中 |
| PR-D | 解构赋值 vs 解构声明（`(A,B) = (a,b)` 在表达式体 ctor 里静默声明两个新局部）| 无 | 触 parser ⇒ **要走 DRAFT → User 确认** |

**先修缺口 4 收益/成本比最高**：`public static readonly Color White = ...` 是 C# 里最常见的 struct
惯用法之一（`Color.White` / `Vector3.Zero` / `Guid.Empty`），今天**完全用不了**；而修法是一行，
机制与 golden 全部现成，格式中立、自举零风险、纯增量（今天崩的修好后不崩，今天能跑的一字节不变）。

---

## 附：本 change 调查过程中被推翻的四个前提（我自己的）

1. **「缺口 4 是 arena 与静态字段生命周期的机制矛盾、不可小修」** —— 矛盾真实，但
   `add-static-struct-bytecization` 早已用装箱解决；缺的只是 `EmitStaticInit` 漏挂，**修法一行**。
2. **「缺口 5 是后备字段没进 `StructLayout`」** —— 后备字段**在**布局里（`MemberCollector.z42:206`，
   探针实证 `y` 拿到 offset 4）。真因是**查布局用了源名而非 `__prop_X`**，返回 -1 编码成 u32。
3. **「VM 对三条取址指令的支持是 2026-08-24 进的」** —— 那是模块拆分搬家；真正 support 是
   `cb61cc072`（2026-05-05），等待窗口已过 4 个多月。
4. **「缺口 3 的修法是放宽 `IsBlobStruct`」** —— 与既有设计相反。`unify-value-types` Phase 4 的意图是
   **塌缩成 Scalar**；而且缺口 3 整体已有主，已移出本 change。

**教训**：读码得到的「根因」要用探针实测反证一次再下结论。本 change 四条自我推翻里，
有三条是靠「写一个应当成立的反例，看它成不成立」发现的（空静态 ctor / `y` 的 offset / `git log -S`）。
