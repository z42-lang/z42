# Design: 模式匹配 C —— switch 穷尽性诊断（bool / enum）

## 落点：binder 语义阶段（非 analyzer 框架）

**事实校正**：早期设想「复用 analyzer 框架 + `[lints]`」不可行——analyzer 契约
`Analyzer.OnSyntaxNode(int kind, object node, DiagSink)` 跑在 **AST(syntax) 层**，只拿到原始 AST 节点，
**无** `Z42Type` / `SymbolTable`。穷尽性判定需要 subject 的**解析类型** + enum **成员表**，这些只在
semantics 阶段可达。故穷尽性检查落在 binder：

- `StmtBinder._bindSwitchStmt`（构造 `BoundSwitch` 后）→ `_tc._exhaust.CheckStmt(bsw, env)`
- `ExprTyper._bindSwitchExpr`（构造 `BoundSwitchExpr` 后）→ `_tc._exhaust.CheckExpr(bse, env)`

`ExhaustChecker` 持 `TypeChecker _tc`（同 `_pattern`），经 `_tc._diags.Warning(...)` 上报。

## 判定算法

```
Check(subject, cases/arms, env):
  if 任一臂无条件兜底: return                       # 穷尽
  covered = {}                                      # StrMap 作值集
  for 每个 guardless + hasPattern 臂: collect(pattern, covered)
  report(subject.Type(), covered, env)

isUncond(hasPattern, pat, guard):                   # 无条件兜底
  guard != null           → false                   # 带守卫不算
  !hasPattern             → true                     # default 臂
  pat is Wildcard/Binding → true                     # 恒匹配
  else                    → false

collect(pat, covered):                              # 覆盖的常量值
  BoundConstantPattern → covered.add(constKey(Value))
  BoundOrPattern       → 递归各 alt                  # Red | Green | Blue
  else                 → 忽略（不覆盖单一常量）

constKey(v):                                        # 值 → 集合 key
  BoundLitInt  → "i:" + Text                         # 整数 / 枚举成员（降级为整数字面量）
  BoundLitBool → "b:true" / "b:false"
  else         → ""（不计）

report(subjType, covered, env):
  if Canon(subjType.Name()) == "bool":
      缺 true 或 false → Warning W0700
  elif env.Symbols.EnumTypes.ContainsKey(subjType.Name()):
      for 每个成员 EnumConsts["<Enum>.*"]:           # 全成员整数值
          if "i:"+值 ∉ covered: 记入 missing
      missing 非空 → Warning W0700
  else: 不检查（开放域）
```

## 关键数据来源

| 需要 | 来源 |
|------|------|
| subject 类型 | `BoundSwitch.Subject.Type()` / `BoundSwitchExpr.Subject.Type()` |
| bool 识别 | `PrimModel.Canon(name) == "bool"` |
| enum 识别 | `env.Symbols.EnumTypes.ContainsKey(name)`（enum 无专用类型，降级 `Z42ClassType`） |
| enum 全成员值 | `env.Symbols.EnumConsts.Keys()` 过滤前缀 `<Enum>.` → `Get(key) as StrBox`.Value（整数字符串） |
| 臂覆盖的 enum 值 | `BoundConstantPattern.Value` = `BoundLitInt`（`Color.Red` 绑定期降级为整数字面量，**成员名已丢失** → 按整数值比对） |
| 兜底 / 通配 | `HasPattern==false` / `BoundWildcardPattern` / `BoundBindingPattern`（且 `Guard==null`） |

## sealed 为何 out-of-scope

z42 `sealed`（`Z42ClassType.IsSealed`）语义 = **不可被继承（final）**，用于继承强制 + receiver 去虚化，
**不是** Rust/Kotlin 的「封闭子类集」——一个 sealed 类恰恰**没有**子类。且 `SymbolTable` 只有前向
`IsSubclassOf`（沿 base 链上溯），**无反向子类型索引**（枚举一个基类的全部已知子类）。故 sealed 穷尽性
无数据基础，out-of-scope。要支持需先引入「封闭子类集」语义 + 反向索引 + 类型模式覆盖判定（独立大改动）。

## 字节不动点 / 格式 / 自举

- **纯诊断、无 codegen 改动**：只往 `DiagnosticBag` 加 warning，不改任何发码 → 自举 gen1==gen2 天然成立。
- **z42c / stdlib / xtask 源无任何 bool/enum switch 语句**（实测 `grep 'switch ('` = 0）→ 穷尽性检查在编
  z42c 自身时从不触发，**零 warning 洪水、零 byte 影响**。
- **W0700 走字面量**（不加 `DiagnosticCodes.z42` 常量）→ 规避 core→semantics 冷启动 stale-cache。
- 无 zbc/zpkg 格式 bump、无新语法/token/runtime。

## 测试

semantics 单测 `tests/exhaust/exhaust_tests.z42`（10 例）经 `SemanticDump.FirstErrorCode`（返回首条诊断码，
含 warning）断言。**switch-stmt 测试源不写 `break`**——独立 Infer 路径无循环深度上下文，`break` 会触发
无关 `E0410`（"break outside loop"）排在 W0700 前遮蔽它（该 E0410 是最小 Infer 路径的产物，非真错：
带 break 的 switch 在完整管线正常编译，如 `pattern_core`）。无 break 时唯一诊断即 W0700。

> **事实校正（2026-09-06，`fix-switch-break-diagnostic`）**：上一段括号里的判断**是错的**。该 `E0410`
> 不是"最小 Infer 路径的产物"，而是一个**真编译器 bug**——`_bindSwitchStmt` 从不建立 break 上下文，
> 于是**任何不在循环里的** `switch` 内 `break` 都被误报，完整管线同样中招（`pattern_core` 幸免只是
> 因为它的 switch 恰好在循环内）。已修。"测试源不写 break"的做法仍保留，但理由改为"让 W0700 成为
> 唯一诊断"，与 break 是否合法无关。
