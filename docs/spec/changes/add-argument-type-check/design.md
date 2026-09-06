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

| # | 根因 | 命中 | 修在哪 | 性质 |
|---|---|---|---|---|
| **R1** | **跨包 imported 泛型签名丢失型参身份**：`Array.Copy<T>(T[],T[],int)` 经 `ImportedSymbolLoader` 读回后 `T` 成了名为 `"T"` 的普通 `Z42ClassType`，不是 `Z42GenericParamType` → `Conversion` 分支 B 的擦除放行不触发 | **79** | 见下「R1 修复链」 | 🆕 本次发现 |
| **R2** | `X[]` / `T[]` → `Array` 不放行（`_classifyBuiltin` 只特判 `object`，没管数组基类 `Array`） | 4 | `Conversion.z42:124` 邻近加分支 | 已知 **bug A** |
| **R3** | 限定类型名固化成字面量 `"unknown"`，且读回成 `Z42ClassType.Builtin("unknown")` 而非 `Z42UnknownType` → Absorb 守卫失效 | 4 | `ImportedSymbolLoader.z42:376` | 已知 **bug B3** |
| **R5** | **无目标 lambda 实参**推成 `Func<<unknown>>`：`_bindLambdaArg` 硬编码返回类型为 `Z42UnknownType`，而 lambda 实参在重载决议**之前**就被绑定，永远拿不到目标签名 | 3 | `ExprTyper.z42:136` + `MemberResolver._bindCall:345-349` | 🆕 本次发现 |
| **R6** | enum ↔ 底层整数不可转（`GCHandle.z42` 传 `long` 给 `GCHandleType` 形参） | 2 | `Conversion`（语义待 Q2 裁决） | 已知 **bug D** |
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

### D4: 构造器实参 —— 同批接，不留不对称

`ConstructTyper` 自绑实参、不经本汇聚点；今天只有 **arity** 检查（E0426），无类型检查。
若本变更只接方法/函数而放过构造器，就留下一个"`f(x)` 查、`new C(x)` 不查"的不对称——正是本程序
反复在清理的那类洞。**决定：同批接**，在 `ConstructTyper` 解析出 ctor 的 `MethodSymbol` 后调用
同一个检查函数。

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
