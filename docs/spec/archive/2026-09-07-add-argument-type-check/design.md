# Design: 调用实参类型检查

## Architecture

```
CallExpr
   │
   ├─ MemberResolver._bindCall / _bindMemberCall / _bindInstanceMemberCall
   │     （free / local-fn / static / instance / interface / instantiated / prim / indirect）
   │
   └────► OverloadBinder.FillDeferredArgs(args, rawArgs, argCount, sig, env)   ← 通用汇聚点
                │                                                                （本变更改名
                │  sig != null  ⟺ 签名已知 ⟺ 可检查                              BindArgsToSignature）
                ├─ ① 回填 target-typed new 延迟位（既有）
                └─ ② 逐位 TypeChecker.CheckImplicitConvert(arg, paramType, syms, argSpan, "argument")  ← 新增
                          │
                          └─► Conversion.Classify(from, to, syms).ImplicitOk()
                                （赋值 / return / var-decl 走的是同一条门）
```

### 为什么汇聚点选 `FillDeferredArgs`

全仓 21 处调用它，覆盖 free / local-fn / static / instance / interface / instantiated /
prim-wrapper / indirect 全部路径；每处都在构造 `BoundCall` **之前**、拿着**未经默认值填充与
params 打包的原始位置实参**——正是检查该发生的时刻与形状。

关键性质：**传 `sig=null` 的 11 处，签名确实不可知**（逐一核过）——错误路径（`no method`）、
懒加载 stub 接收者 loose-bind、`Z42ErrorType`/`Z42UnknownType` 接收者、泛型形参接收者查无
Object 方法。因此 `sig != null` 恰好等价于「签名已知」，无需在各站点分别接线，也不会漏站点。

`_withDefaults`（instance / free / prim 三条主路径）第一步就调 `FillDeferredArgs(ms.Signature)`，
故一并覆盖。

## Decisions

### D1: 复用 E0402 / E0439，不新增诊断码

**问题**：实参类型不符该报什么码？

**选项**：
- A — 新增 `E0453 ArgumentTypeMismatch`。
- B — 复用 `CheckImplicitConvert` 既有的 E0402（无转换）/ E0439（有显式转换、缺 cast）。

**决定：B**。传参与赋值/return 在类型系统里是**同一个隐式转换上下文**，C# 也用同一族诊断
（CS1503 是重载专用，但其根仍是 CS0029 转换失败）。`CheckImplicitConvert` 的 `ctx` 参数已为此
预留——它的注释白纸黑字写着覆盖「赋值 / return / **传参**」，只是传参一项从未接上。复用还自动
带上「常量在范围内例外」（`byte b = 48` 同理适用于 `TakeByte(48)`）与「经中间类型转换」提示。

> 新增码反而会制造两套语义相同、消息不同的诊断——违反单一真相来源。

### D2: 五个根因必须在同一变更内修（否则树不 GREEN）

开启检查后 stdlib / z42c **编不过**。这不是"检查制造的错误"，而是既存缺陷第一次被看见。
必须同批修，否则无法达成 GREEN。逐条根因与修法：

> 🔧 **实施期校正（2026-09-07，以实测为准，覆盖本节初稿的三处判断）**：
>
> 1. **跨包路径是 `ZpkgReader` 而非 `ZbcReader`**。初稿把修复链写在 zbc SIGS reader 上——错。
>    `ImportedSymbolLoader` 的输入来自 `DepScan → TsigReconcile.Rebuild(ZpkgInfo)`，型参名读弃点在
>    [`ZpkgReader.z42:220`](../../../../src/libraries/z42.ir/src/ZpkgReader.z42) 的 `c.U32(); // tp 名`。
>    （`SigEntryZ.TypeParamCount` 全仓无消费方，是另一处独立的死字段。）
> 2. **R1 是两半，只修型参身份不够**。对照实验：修完身份后跨包**裸 `T`** 转绿、**`T[]` 全数仍红**
>    ⇒ 还缺 **R1b 递归擦除**（`Conversion` 分支 B 只看顶层是不是 `Z42GenericParamType`，
>    `Byte[]` 与 `T[]` 两侧都是 `Z42ArrayType`，判定恒相等 ⇒ 擦除永不触发）。
> 3. **R5 的根因不是"lambda 推断"，而是 imported 委托类型退化**。给 lambda 接上目标定型后
>    `Thread.Start(() => {…})` 依旧红——因为形参类型 `Action` 本身经 `ImportedSymbolLoader` 退化成了
>    `Z42ClassType("Action")`，`target is Z42FuncType` 根本不成立。本地解析器
>    `SymbolTable.ResolveTypeP:238-254` 早有 Func/Action/Predicate 分支，imported 侧没有——完全对称的缺口。
>
> ⇒ **R1 / R3 / R5 / R7 四条其实同属一族：`ImportedSymbolLoader` 的类型保真度**。跨包读回时把
> 结构化类型降级成"名字对但种类错"的 `Z42ClassType`（型参 → 普通类、委托 → 普通类、限定名 → 名叫
> `"unknown"` 的类）。这解释了为什么它们只在**传参**处暴露：赋值上下文很少跨包构造出这些形状。

| # | 根因 | 命中 | 修在哪 | 性质 |
|---|---|---|---|---|
| **R1a** | **跨包 imported 泛型签名丢失型参身份**：`Array.Copy<T>(T[],T[],int)` 的 `T` 读回成名为 `"T"` 的普通 `Z42ClassType` | （与 R1b 合计 79） | 见下「R1 修复链」 | 🆕 本次发现 |
| **R1b** | **擦除判定不递归**：`Conversion` 分支 B 只看顶层，`Byte[]` vs `T[]` 两侧同为 `Z42ArrayType` ⇒ 擦除永不触发 | 同上 | `Conversion._hasGenericParam`（结构化递归） | 🆕 实施期发现 |
| **R2** | `X[]` / `T[]` → `Array` 不放行（`_classifyBuiltin` 只特判 `object`，没管数组基类 `Array`） | 4 | `Conversion.z42:124` 邻近加分支 | 已知 **bug A** |
| **R3** | 限定类型名固化成字面量 `"unknown"`，读回成 `Z42ClassType.Builtin("unknown")`（一个**名叫 unknown 的类**）而非 `Z42UnknownType` → Absorb 守卫失效 | 4 | **消费端半边**：`_resolve` 把 `"unknown"` 还原为 `Z42UnknownType`。**产出端半边留作独立 change**（见下） | 已知 **bug B3** |
| **R5** | **imported 委托类型退化**：`Action`/`Func<…>` 经 `ImportedSymbolLoader` 成了普通 `Z42ClassType` ⇒ lambda 实参拿不到 `Z42FuncType` 目标，只能落 `_bindLambdaArg` 的 Unknown 返回 | 3 | ① `ImportedSymbolLoader._resolveDelegate`（对齐本地 `SymbolTable.ResolveTypeP:238-254`）② `BindWithTarget` 加 lambda 目标分支 ③ `_bindCall` 延迟 lambda 实参 | 🆕 本次发现 |
| **R6** | enum ↔ 底层整数不可转 | 2 | 🕳 **本变更跳过 enum 位并登记残留洞**；语义归独立 lang change `make-enum-distinct-type` | 已知 **bug D** |
| **R7** | `Func<int>` ≠ `Func<Int32>`；极端形态连 `Action` → `Action` 都判不可赋 | 4 | `Z42Type.z42:304-307` `Z42FuncType.IsAssignableTo` 用 `Dump()` 逐字比、不 `Canon`（`Z42ArrayType` 就 Canon 了） | 已知 **bug C** |

### R1 修复链（已核实，无格式 bump）

型参**名字本就在 SIGS 的 tp 块里，只是被读弃**：

```
ZbcReader.z42:531-541   int tpc = c.U8();          // 个数（add-array-paired-sort 已捕获）
                        while (t < tpc) {
                            c.U32();               // ← 型参名的 pool 索引，读出即丢 ★R1 修这里
                            ...约束 bundle...
                        }
   ↓ SigEntryZ.TypeParams[]（新增，仿 TypeParamCount）
TsigReconcile           → ExportedMethodZ.TypeParams[]（新增；ctor 元数不变，构造后赋值——
                          与 ParamsFrom / IsSealed / TypeParamCount 同款旧种子 ABI 约定）
   ↓
ImportedSymbolLoader    _resolve(r, name, typeParams, tpCount) 的表并入**方法级**型参
   :296/297（自由·静态方法，现传空表 `_resolve/2`）
   :348/349（类方法，现只传类级 tps）
   ↓
Conversion._classifyBuiltin 分支 B「恰一侧泛型形参 → GenericErase」自然生效
```

> **`_resolve` 现有两个重载**：2 参版（`:435-437`）硬传 `new string[0], 0`；4 参版只喂类级
> `cl.TypeParams`。`Array.Copy<T>` 属「类无型参 + 方法级型参」，**两条路都漏**——与实测吻合。
>
> **不改 wire 格式** ⇒ 不 bump zbc/zpkg minor ⇒ **不受
> [bootstrap-seed.md](../../../../.claude/rules/bootstrap-seed.md) 两-nightly support/use 纪律约束**。
> 先例：`add-array-paired-sort` 用同样手法捕获 tp 个数，明确记「SIGS 早已存 tp 块，无格式 bump」。

> **R1 与 R3 同族**：都是 `ImportedSymbolLoader` 的**类型保真度**问题——跨包读回时把结构化类型
> 降级成"名字对但种类错"的 `Z42ClassType`。按 [philosophy.md 根因修复](../../../../.claude/rules/philosophy.md)
> 修**产出端**（loader 忠实还原型参 / 真实类型名），而非在 `Conversion` 加同名兼容分支。
>
> **R2 / R7 已独立坐实为既存 bug**（**赋值**上下文即可复现，与本变更无关）：
> `Array boxed = x;` → `E0402 (var-decl)`；`closure_l3_loops.z42:58` →
> `cannot assign Func<int> to Func<Int32> (assign)`。它们一直存在，只是 `--emit-zbc` 吞掉了。
>
> **R1 只能经传参触达**——泛型方法体内无法凭空造出具体类型的值赋给 `T[]`，所以它在赋值上下文
> 永远不出现。这正是它长期无人发现的原因。

### D3: R5 的修法——lambda 实参走既有的延迟绑定通道

**问题**：lambda 实参在 `_bindCall:345-349` 被**提前**绑定（`_bindExpr`），此时形参类型未知 →
返回类型只能填 `Z42UnknownType`。

**选项**：
- A — 在 `_bindLambdaArg` 里猜：块体无 `return` ⇒ `Action`。**治标**：只解决 void 一种情形，
  带返回值的 lambda 仍拿不到形参类型；且"猜"违反根因修复。
- B — **把 lambda 实参纳入既有的 target-typed 延迟绑定通道**：`_bindCall` 见 lambda 实参时留
  `null` 占位（同 `IsTargetTypedNew`），重载决议后由 `BindArgsToSignature` 按形参类型
  `BindWithTarget` 回填。

**决定：B**。z42c 已为 target-typed `new` 建好整条延迟通道（`args[i]==null` 占位 →
`FillDeferredArgs` 按 sig 回填 → `E0437` 兜歧义），lambda 是同一个问题的另一个实例，**复用而非
新造**。副作用是正向的：lambda 从此拿到真实形参类型，`-> unknown` / `add ?`（JIT 丢类型）一并消失。

**风险**：重载决议对 lambda 实参将无类型可用（同 target-typed new），同 arity ≥2 时会走 E0437
让用户写显式类型。全仓需确认无此类现存调用（tasks.md 3.x 核）。

### D4: 构造器实参 —— 同批接，不留不对称（Q3 已裁决）

`ConstructTyper` 自绑实参、不经本汇聚点；今天只有 **arity** 检查（E0426），无类型检查。
若本变更只接方法/函数而放过构造器，就留下一个"`f(x)` 查、`new C(x)` 不查"的不对称——正是本程序
反复在清理的那类洞。**决定：同批接**，在 `ConstructTyper` 解析出 ctor 的 `MethodSymbol` 后调用
同一个检查函数。

### D6: enum —— 本变更**跳过 enum 位**，语义拆为独立 lang change（Q2 裁决落地方式，2026-09-07 修正）

**问题**：Q2 裁定「enum ↔ 整数要求显式 cast（对标 C#）」。但实施时发现该裁决建立在**错误前提**上——
我在提问时把现场描述成「用户把 `long` 传给 `GCHandleType` 形参」，**实际是
`GCHandle.Alloc(target, GCHandleType.Weak)`，实参是枚举成员引用本身**。

**事实**（实测 + SoT）：

- `MemberResolver.z42:36-46` **刻意**把 `E.Member` 绑成 `BoundLitInt(long)`，注释写明「保 z42
  **enum-as-int 模型**……静态类型仍 long」；book `runtime/struct-value-semantics.md:232` 有对应 SoT。
- `SymbolTable.z42:255` 把 enum **类型名**解析成孤立 `Z42ClassType`。
- 两者在转换格里**无边相连** ⇒ 实测 `Color c = Color.Blue;` **今天在 var-decl 就编不过**
  （与本变更无关，只是 `--emit-zbc` 一直吞了诊断）。即**今天无法产生任何 enum 类型的值**。

**决定**：按 Q2 执行会变成在调用点写 `(GCHandleType)GCHandleType.Weak` —— 那是**拿 cast 掩盖
「enum 成员产不出 enum 类型值」这个 bug**，违反本程序铁律。故：

- **本变更**：`_checkOneArg` 对 enum 位跳过（`_isEnumSide`），代码里写明这是**有意的残留洞**并指向下条。
- **独立 lang change `make-enum-distinct-type`**：实现 Q2 选定的 C# 语义
  （成员定型为 enum 类型 + 双向显式 cast），并**摘掉本变更留的跳过**。它要改写 book 的 enum-as-int SoT
  与 `src/tests/types/enum.z42` 里 4 条成文断言，半径远超实参检查，按 Spec-First 必须自带 proposal/spec/design。

> 下面这节是**初稿**的写法（把 R6 当成本变更内的一条转换规则），已被上面取代，保留作决策留痕。
>
> ~~### D6-初稿: enum ↔ 底层整数要求显式 cast~~

**问题**：`Conversion._classifyBuiltin` 今天对 enum ↔ 整数给出 `None`（"根本无转换"），
导致 `GCHandle.z42` 把 `long` 传给 `GCHandleType` 形参时报 `E0402`。

**选项**：A — 隐式双向放行；B — 要求显式 cast（对标 C#）。

**决定：B**。理由：
1. 与 z42 已确立的 `tighten-implicit-conversions`（窄化 / 有损浮点须显式）**同向**——
   enum→整数丢的是"语义标签"，整数→enum 更是可能落在任何未定义值上，正属该收紧的一类。
2. 保住 enum 的类型区分度；隐式双向等于把 enum 退化成整数别名。

**实现**：`Conversion` 增 `ConvKind.ExplicitEnum`（**不进** `ImplicitOk` 白名单，进 `Exists()`）
→ 隐式上下文落 `CheckImplicitConvert` 的 `r.Exists()` 分支 → 报 **E0439**
（"an explicit conversion exists (are you missing a cast?)"），而非 E0402「无转换」。
这条消息正是用户需要的指引。

**边界**：enum **成员**引用（`GCHandleType.Weak`）不受影响——那是 enum 类型自身，非整数转换。
`GCHandle.z42` 两处调用点加 `(GCHandleType)` / `(long)` 显式 cast。

> ⚠️ 欠债表的 **bug D** 面比这里大（`Color c = Color.Blue` 与 `long n = c` 今天**两个方向都报错**，
> 即 enum 成员引用本身也坏）。本变更只处理**传参触达的这一片**（`ExplicitEnum` 种类 + 两处 cast）；
> bug D 的完整修复（enum 名解析成孤立 `Z42ClassType`、成员却是 `long`，转换格里无边相连）
> 仍属独立 change，不在本 Scope。

### D5: 残留洞必须写进文档，不得静默

以下路径**签名不可知**，本变更不覆盖，须在 book 机制页显式登记（否则又是一条"没有东西盯着的断言"）：

1. **懒加载 stub 接收者 loose-bind**（`MemberResolver.z42:64-66`）——`ct.Methods` 为空且主符号表
   无真类时松绑，运行期经 DepIndex 解析。
2. **`Z42ErrorType` / `Z42UnknownType` 接收者**——级联抑制，本就该 Absorb。
3. **`Z42GenericParamType` 接收者查无 Object 方法**（`:123-124`）。
4. **`params` 尾位**：本变更按元素类型逐位检查展开形态；**规范形态**（直接传数组）沿用
   `_resolveParamsOverload` 既有判定，不重复检查。

## Implementation Notes

- 检查位置：`sig.ParamsFrom < 0` 时查 `i < min(argCount, sig.ParamCount)`；`ParamsFrom >= 0` 时
  定长段查 `[0, ParamsFrom)`，尾段按 `ParamTypes[ParamsFrom]` 的元素类型逐位查。
- **实参 span**：用 `rawArgs[i].Span`（指到实参本身）而非调用点 span——探针实测定位准确
  （`repro.z42(8,11)` 正指 `42`）。
- **不短路**：一次调用里多个实参不符应**逐条**报，不在第一条就 return（探针已验证 3 条齐出）。
- 检查须在 `BoxArgs` / `_withParamsExpansion` **之前**——那两步会改变实参形状与类型。

## Testing Strategy

- **单元（负例）**：`z42c.semantics/tests/typecheck/argument_type/` —— 三条调用路径
  （free / static / instance）× 每路径「类型不符 → E0402」；窄化 → E0439；常量在范围内 → 放行。
  🔴 **必须用 `DumpBody` 或 `collectDiags` 断言，不得只用 `SemanticDump.FirstErrorCode`**——
  后者**从不合并 collector 诊断**，签名位置的诊断恒不可见，会造出空门
  （[[add-associated-types-program]] 已实测踩过：7 条单测退回改动后仍 7/7 全绿 = 全是空测试）。
- **正例回归**：五个根因各配一条"修好后应当放行"的用例（跨包 `Array.Copy(byte[],byte[],int)`、
  `X[] → Array`、enum ↔ 整数、`Thread.Start(() => {...})`、跨包泛型接口 `IBag<int>.Add(1)`）。
- **GREEN**：完整 `xtask test`；自举不动点（gen1 == gen2 字节）必须成立。
- **边界**：`xtask test bootstrap` —— 本变更**不引入新语法、不改二进制格式**，上一 nightly 的
  z42c 编当前源应照常通过。

## Deferred / Future Work

### add-argument-type-check-future-overload-diagnostic

- **来源**：本 design D1
- **触发原因**：重载决议 no-match 时今天只返回 `null`（自由函数路径连诊断都没有），用户看到的是
  `undefined function` 而非"有这个方法但实参不匹配"。对标 C# CS1503/CS1501 需要一条专门的
  "no overload takes these arguments" 诊断并列出候选签名。
- **前置依赖**：本变更（先让单候选路径能报错）
- **触发条件**：用户抱怨重载调用的错误信息难懂时
