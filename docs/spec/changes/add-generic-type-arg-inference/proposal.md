# Proposal: 方法级类型实参推断 + 形参位类型实参代换

> 类型：lang（新类型规则）｜ 创建：2026-09-08 ｜ 状态：**DRAFT（待阶段 6.5 确认）**
> 前置：`add-generic-methods`（显式 `Foo<T>()` 落地）、`add-argument-type-check`（#523 接线实参检查）、
> `bind-self-param-and-constraint-members`（#530 型参收者约束成员绑定）、
> `fix-imported-generic-func-fidelity`（#531 跨包泛型自由函数型参保真度）
> 分支：`tighten-bare-type-param-erasure` ｜ worktree：`../z42-erasure`（基于 `origin/main` 54c8a1df）

## Why

roadmap 的 Deferred 队首 `tighten-bare-type-param-target-erasure` 记的是
「`Conversion._classifyBuiltin` 分支 B 对『目标是裸型参、来源是具体类型』也放行 ⇒ `a.Same("nope")`
今天仍无诊断」，并注明「爆炸半径未量」。**开工前把半径量了，结论是这条记述的前提不成立。**

### 事实 1：z42c 在任何调用路径上都不把类型实参代回形参类型

| 路径 | 现状 | 证据 |
|---|---|---|
| 泛型类实例方法（`List<string>.Add`） | **只代换返回位**，形参位原样交给 `_withDefaults` | `MemberResolver.z42:140-141` |
| 显式方法类型实参（`IdOf<string>(7)`） | `_applyMethodTypeArgs` 只写 `MethodTypeArgs` + 校 arity/where，**从不代回签名**；且在 `_withDefaults` **之后**跑 | `MemberResolver.z42:474-493` |
| 隐式推断（`IdOf(7)`） | **全仓零实现**；`call.TypeArgCount == 0` 第一行早退 | `MemberResolver.z42:475` |

⇒ 泛型形参位的实参检查今天 **100% 静默通过**。唯一例外是 #530 给约束接口方法做的 `Self` 形参替换
（`_substSelfSig`，`MemberResolver.z42:449`），而它替换出来的是**裸型参**，照旧被分支 B 放行。

### 事实 2：按 roadmap 字面收紧擦除规则 = 全线假红，真欠债 0

探针把分支 B 的「目标含型参、来源具体」判 `None`（C# CS1503 口径），`xtask build stdlib` 在头 3 个包就崩：

```
z42.core/src/Collections/Dictionary.z42(31,111): E0402: cannot assign Dictionary to Dictionary<TKey, TValue>
z42.ir/src/BinaryFormat/ByteWriter.z42(24,20):   E0402: cannot assign Byte[] to T[]
z42c.syntax/src/DeclParser.z42(14,20):           E0402: cannot assign Decl[] to T[]
z42c.pipeline/src/Z42cReplCompiler.z42(172,65):  E0402: cannot assign string to T
```

33 条逐条核对，**全部是合法代码**（`Array.Copy(this._buf, bigger, this._len)`、
`outp.Add(m.Classes[c].Name)`、`new DictionaryEnumerator<TKey,TValue>(this)`），**真欠债 0 条**。

**根因不是擦除规则太松，是检查时拿到的形参类型根本没被代换。** 收紧擦除规则的硬前置是
「类型实参推断」——而它今天不存在。这与 #531 是同一形状：**Deferred 条目记的「根因」是当时的推断，
动手前必须自己再走一遍链路。**

### 事实 3：根因修法已验证可行，欠债 0

把 receiver 的 `TypeArgs` 代换进形参位、产出**诊断专用签名**再查一遍实参：

- 欠债实测 **= 0**（stdlib 25 库 + compiler 全量自建，`REAL_EXIT=0`）
- **阳性对照**：`List<string>.Add(42)` → `E0402: cannot assign int to String`；`l.Add("ok")` 不报
- **退回对照**：同一份源用基线编译器只剩无关的 `E0436`
- **真实构建面破坏性对照**：把 `Z42cReplCompiler.z42:172` 实参改成 `12345`，`xtask build compiler`
  立刻 `REAL_EXIT=1` ⇒ 检查活在真实构建路径上，不是只在单测里

## What Changes

四条链路，按阶段落地。**贯穿全篇的安全不变式：代换出来的签名只喂 `CheckArgTypes` 产生诊断，
绝不回灌 `_withDefaults` / `BoxArgs` / `ConvertIfNeeded` / `_withParamsExpansion` / 重载决议。**
（理由见 design D2；那四条通道每一条都是确定性的自举字节漂移。）

### 阶段 A —— 泛型类实例方法的形参位代换

`MemberResolver` 的 `Z42InstantiatedType` 分支新增 `_substGenericSig`（镜像既有 `_substSelfSig`），
把 `T→string` 代换进形参位，额外调一次 `CheckArgTypes`。`_withDefaults` 的输入**保持原签名不变**。

收益：`list.Add(42)` / `dict.Set(7, v)` 这类调用第一次被真检查。

### 阶段 B —— 显式方法类型实参的形参位代换

`_applyMethodTypeArgs` 已经解析出 `targs`，就地用它对 `ms.Signature` 做**方法级**型参代换，
再查一次实参。**不调整执行顺序**——它在 `_withDefaults` 之后跑，正好只影响诊断。

收益：`IdOf<string>(7)` 报错（#531 正面留下的口子）。

### 阶段 C —— 方法级类型实参推断

新增 `TypeArgInference`：把形参类型与已绑定的实参类型做**结构化 unify**（裸型参 / 数组元素 /
实例化类型实参 / func 形参·返回），求出型参绑定。挂在 `_applyMethodTypeArgs` 的早退之前。

推断成功 ⇒ ① 形参位代换后真检查实参 ② 复用 `ConstraintChecker.CheckMethod` 校验 where 约束
（关掉 Deferred `where-constraint-future-inferred-method-args`）。
**推断失败 ⇒ 完全按今天行为（擦除放行、不发任何诊断）** —— 爆炸半径由此被限制在「推断成功」的子集内。

**不回灌 `bc.MethodTypeArgs`**：全仓普查（stdlib 25 库 + compiler 自建，`REAL_EXIT=0`）显示
隐式泛型调用**共 112 处，全部是同一个方法 `Array.Copy<T>`**，而它的 `T` 纯粹是编译期类型安全装置
——函数体只做参数校验，搬运落到非泛型 native 原语 `CopyRange`（`Array.z42:52-58, 76-81`）。
回灌会把这 112 处热点全部推出 native/JIT 快路径（`interp/exec_call.rs:135` 以
`method_type_args.is_empty()` 为快路径门）+ 重排 zbc 字符串池，换来**零**语义收益。

### 阶段 D —— callee 真消费型参时要求显式类型实参（新诊断）

阶段 C 不回灌留下的洞：若 callee 体内有 `typeof(T)` / `new T()` / `default(T)` / `new T[n]`，
省略尖括号调用会让运行期读到空的 `frame.method_type_args`（→ 静默错值，同
`generic-new-array-value-type-null-tail` 那条坑的形状）。本阶段把**静默错值换成编译错误**：
能判定 callee 消费型参时，报错要求显式写出类型实参。

判定不到（导入方法无本地 `Decl`）⇒ 放行 = 今天行为，**严格无回归**。

### 阶段 E —— 规范冲突处置 + 文档迁移（User 已裁决）

`docs/design/language/generics.md:380` 写着「自由函数调用时 T 从实参推断后做约束校验；返回类型也按
推断做 T → 具体类型替换」，**与 book SoT `generic-methods.md:109`（「推断留后续」）及实现三方冲突**
——这条从未实现。User 裁决：**整段迁进 book 顺带完成迁移**（`language/README.md:31` 本就标着「⬜ 待迁」）。

## Scope（允许改动的文件）

| 文件路径 | 变更类型 | 说明 |
|---|---|---|
| `src/compiler/z42c.semantics/src/MemberResolver.z42` | MODIFY | 阶段 A `_substGenericSig` + 实例化分支接线；阶段 B/C/D 在 `_applyMethodTypeArgs` 接线 |
| `src/compiler/z42c.semantics/src/TypeArgInference.z42` | NEW | 阶段 C：结构化 unify + 绑定求解 |
| `src/compiler/z42c.semantics/src/OverloadBinder.z42` | MODIFY | 阶段 A/B/C：若需要一个「只做诊断、不重复装箱」的检查入口 |
| `src/libraries/z42c.core/src/DiagnosticCodes.z42` | MODIFY | 阶段 D 新错误码 **E0455**（已 grep 既有码表：E0446–E0454 全被占用，确需新造） |
| `src/compiler/z42c.semantics/src/MethodTypeParamUse.z42` | NEW | **阶段 D 实施期追加进 Scope**：完整 AST 表达式 walker，判定方法体是否消费方法级型参。DRAFT 时误以为有现成遍历设施，实测 `z42c.syntax` 只有 statement 级（`AnalyzerDriver._walkStmt`），表达式面 36 个节点类无遍历 ⇒ User 裁决走 D-1（补完整 walker） |
| `src/compiler/z42c.semantics/tests/typecheck/generic_inference/generic_inference_tests.z42` | NEW | 单测：阳性 + 正例 + 推断失败降级 |
| `src/compiler/z42c.semantics/tests/typecheck/generic_inference/z42c.semantics.test.typecheck.z42.toml` | NEW | 仅当子目录需要独立 toml；否则复用父级 `include = ["**/*.z42"]` |
| `docs/book/src/language/generics.md` | NEW | 阶段 E：`docs/design/language/generics.md` 迁入 + 修正失效段 |
| `docs/book/src/language/generic-methods.md` | MODIFY | `:109` 的「推断留后续」改写为已落地 + 边界 |
| `docs/book/src/language/generic-constraints.md` | MODIFY | 已知限制 §2 改写；`:58` 校验表格；`:217-226` 形参位边界改写 |
| `docs/book/src/language/README.md` | MODIFY | 迁移状态表 `generics.md` 打勾 |
| `docs/book/src/SUMMARY.md` | MODIFY | 挂载新页 `language/generics.md` |
| `docs/design/language/generics.md` | DELETE | 阶段 E：内容迁入 book 后删除（含失效的 `:380`） |
| `docs/roadmap.md` | MODIFY | 改写 `tighten-bare-type-param-target-erasure` 的根因与前置；关掉 `where-constraint-future-inferred-method-args` 与 `generic-methods-future-type-inference` |
| `docs/features.md` | MODIFY | 语言设计决策：类型实参推断 |
| `docs/spec/changes/add-generic-type-arg-inference/**` | NEW | 本 change 四件套（归档时 `git mv` 到 `archive/`） |

**只读引用**（不改）：`src/compiler/z42c.semantics/src/Conversion.z42`、`TypeChecker.z42`、
`OverloadResolver.z42`、`CallEmitter.z42`、`src/libraries/z42.ir/src/BinaryFormat/ZbcInstr.z42`、
`src/runtime/src/interp/exec_call.rs`。

## Out of Scope

- **收紧 `Conversion` 分支 B 本身**（`tighten-bare-type-param-target-erasure`）。阶段 A/B/C 让
  两侧都变具体类型，分支 B 自然不触发；**真正剩下的裸型参目标**（推断失败 / 型参收者的
  `a.Same("nope")`）仍放行。收紧它要动**通用**擦除规则，且需要区分「作用域内不透明型参」与
  「待推断型参」——`Z42GenericParamType` 今天只带名字、不带 owner ⇒ 独立 change。
- **把「callee 需要才发类型实参」统一到显式路径**。那能顺带救回今天显式泛型调用白掉的 JIT 快路径，
  但需要**可传递、跨包可见**的「方法体消费型参」分析（`$mta:` 转发意味着要传递闭包，跨包要进元数据）
  ⇒ 独立 change，登记 Deferred。
- **推断参与重载决议**。`OverloadResolver._assignable:222-228` 用裸 `IsAssignableTo`，今天在静默淘汰
  泛型候选；把推断插到决议之前会让 `void F(int)` 与 `void F<T>(T)` 变歧义 ⇒ 今天能编的代码编不过。
  推断一律在**决议选定唯一 `ms` 之后**做。
- **lambda 实参参与推断**。lambda 在形参类型未知时已被绑成 `Z42UnknownType`；让它参与 unify 要动
  延迟绑定通道，牵扯字节漂移（`ExprTyper.z42:582`）。v1 跳过含 lambda 的实参位。
- 跨包关联类型（`assoc-type-crosspkg`）、嵌套约束、`where` 接口类型实参匹配——三条 Deferred 不动。

## ✅ User 裁决（2026-09-08，勿重问）

1. **scope = A + B + C**（连类型实参推断一起做）。
2. **规范冲突处置**：`design/language/generics.md` **整段迁进 book**，顺带完成 `README.md:31` 标着
   「⬜ 待迁」的那次迁移（= 阶段 E）。
3. **阶段 D 本轮做**——不做就等于明知有静默错值洞而不堵，且它正是 D4「不回灌」的配套。
4. **阶段 E 取「原样迁入 + 修正失效段」**，不借机重写（重写与本 change 主线无关，成本不相称）。
5. **推断失败不发 warning**（自行裁定，依据充分）：`driver-hides-warnings` —— `Main.z42` 的
   `ErrorCount > 0` 门控**全部**诊断呈现，无 error 时 warning 一条不打印 ⇒ warning 在本项目是
   空操作。推断失败只在 `--dump-bound` 可见。
