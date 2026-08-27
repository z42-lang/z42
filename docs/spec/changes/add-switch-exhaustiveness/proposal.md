# Proposal: 模式匹配 C —— switch 穷尽性诊断（封闭域 bool / enum）

## Why

模式匹配核心已就位，但 `switch` 对**封闭域**（值集有限的类型）**未覆盖全部情形时无任何提示**——遗漏一个
enum 成员、漏掉 `false` 分支，都要到运行期才暴露（或静默走 default）。Rust `match` 的穷尽性检查是其类型
安全的核心价值之一。C 给 z42 `switch`（语句 + 表达式）补上**穷尽性诊断**：封闭域未覆盖全部情形且无兜底
臂时报 warning。

```z42
enum Color { Red, Green, Blue }
string name = c switch {
    Color.Red   => "r",
    Color.Green => "g"
    // ⚠️ W07xx: switch 未覆盖 Color.Blue（穷尽性）
};
```

## What Changes（含两处对原设想的事实校正）

**校正 ①：不能用 analyzer 框架。** 记忆/路线图原设想「复用 analyzer 框架 + `[lints]`」。但探查确认
**analyzer 框架跑在 AST(syntax) 层，拿不到任何类型信息**（`Analyzer.OnSyntaxNode(int kind, object node,
DiagSink)` 只给原始 AST，无 `Z42Type`/`SymbolTable`）——而穷尽性判定**必须知道 subject 的解析类型 + enum
成员表**。因此 C **落在 binder(semantics) 阶段**：`StmtBinder._bindSwitchStmt`（:66）与
`ExprTyper._bindSwitchExpr`（:190）里 subject 类型已知、所有 case/arm 的 pattern 在手，直接往
`_tc._diags.Warning(...)` 报（范例 `AccessChecker.z42:86` 的 `[Deprecated]` 告警）。

**校正 ②：sealed 域不可行。** 原设想覆盖「bool/enum/sealed 层次」。但 z42 的 `sealed` 语义是
**「不可被继承（final）」**（`Z42Type.z42:58-61`），**不是** Rust/Kotlin 的「封闭子类集」；且
`SymbolTable` **无反向子类型索引**（只有前向 `IsSubclassOf`）——无法枚举一个基类的全部已知子类。故
**sealed 穷尽性 out-of-scope**（要支持需先引入「封闭子类集」语义 + 反向索引，属独立大改动）。C **只做
bool + enum 两域**。

| 域 | 可行 | 判定 | 数据来源 |
|----|------|------|----------|
| **bool** | ✅ | case 覆盖 `true` ∧ `false`，或有兜底臂 | `PrimModel.Canon(t.Name())=="bool"`；常量模式 `BoundConstantPattern.Value` |
| **enum** | ✅ | 常量臂整数值集合 ⊇ `EnumConsts` 全成员整数值集，或有兜底臂 | `SymbolTable.EnumTypes/EnumConsts`；enum 成员访问降级为 `BoundLitInt`（**按整数值比对**，成员名 bind 期已丢失） |
| ~~sealed~~ | ❌ | sealed=final，无反向子类索引 | out-of-scope |

### 判定算法（bind 阶段，两位点各一次）

```
_checkExhaustive(subjType, cases[], symbols, span):
  if 任一臂无条件兜底 (HasPattern==false，或 pattern 是 Wildcard/裸Binding 且 Guard==null): return  // 穷尽
  if PrimModel.Canon(subjType.Name()) == "bool":
      covered = { 常量臂的 bool 值 }（跳过带 Guard 的臂——守卫下不算无条件覆盖）
      if !(true ∈ covered ∧ false ∈ covered): Warning("W07xx", "未覆盖 bool 全部取值")
  else if symbols.EnumTypes.ContainsKey(subjType.Name()):
      all = { EnumConsts["<Enum>.*"] 的整数值 }
      covered = { 常量臂 BoundLitInt 值 }（跳过带 Guard 的臂）
      missing = all \ covered
      if missing 非空: Warning("W07xx", "switch 未覆盖 <Enum> 的 N 个成员")
  // 其它类型（开放域）：不检查
```

**兜底判定**：任一臂 `HasPattern==false`（default）**或** pattern 为 `BoundWildcardPattern` /
无守卫裸 `BoundBindingPattern` → 视为穷尽，不报。带 `Guard` 的臂**不算**无条件覆盖。

### 严重级别 / 开关

- 走 `DiagnosticBag.Warning`（**不经 LintConfig**，那是 analyzer 侧）。默认 **warning、默认开启**
  （不阻断编译，GREEN 不受影响；下面确认默认级别）。
- 诊断码：新 warning 码（如 `W0700 SwitchNotExhaustive`），加在 `DiagnosticCodes.z42` W07xx 段。
  **接线注意**：core→semantics 新符号在冷启动可能撞 stale-cache（`two-gen-bootstrap` 教训）→ 首版
  **在 semantics 直接用字面量 `"W0700"` 发码**（同 E0449/E0450 做法），待随 nightly 载入 core 后再切常量。

## 实现落点（Scope 文件）

| 文件 | 变更 | 说明 |
|------|------|------|
| `src/compiler/z42c.semantics/src/ExhaustCheck.z42` | NEW | `ExhaustChecker`：封闭域识别 + 覆盖集比对（StrMap）+ 兜底判定 + or 递归 |
| `src/compiler/z42c.semantics/src/TypeChecker.z42` | MODIFY | 加 `_exhaust` 字段 + ctor 实例化 |
| `src/compiler/z42c.semantics/src/StmtBinder.z42` | MODIFY | `_bindSwitchStmt` 尾部调 `_exhaust.CheckStmt(bsw, env)` |
| `src/compiler/z42c.semantics/src/ExprTyper.z42` | MODIFY | `_bindSwitchExpr` 尾部调 `_exhaust.CheckExpr(bse, env)` |
| `src/compiler/z42c.semantics/tests/exhaust/` | NEW | semantics 单测（10 例：enum/bool × stmt/expr × 缺/全/default/or/守卫/开放域） |
| `docs/book/src/language/pattern-matching.md` | MODIFY | 新增「穷尽性诊断」节（域范围 + 兜底规则 + sealed 为何 out-of-scope） |

> **W0700 诊断码**：首版直接在 semantics 侧用**字面量 `"W0700"`** 发码（避 core→semantics 冷启动
> stale-cache，同 E0449/E0450 做法），不改 `DiagnosticCodes.z42`。

## 自举 / 格式影响

- **无格式 bump、无新语法/token/runtime**：纯诊断，只加 warning。
- **⚠️ 自举字节不动点风险点**：C 是**唯一可能影响 codegen 之外行为**的 change——它**只加诊断不改发码**，故
  gen1==gen2 不受影响。但须确认：z42c **自身源码里的 switch** 若恰好是「封闭域未覆盖」会新报 warning
  ——warning **不阻断编译**（非 error），但若 z42c 源有此类 switch，构建日志会新增 warning。**IMPL 首步
  grep z42c 源的 enum/bool switch 用法**，确认不会把干净构建刷成一堆 warning（若有，评估是补 default 还是
  调低默认级别）。
- **两-nightly 纪律**：纯 semantics 诊断，不涉及跨 nightly 语法/格式；但新诊断码若走 core 常量则受
  core→semantics 冷启动约束（故首版走字面量规避）。

## User 6.5 裁决（已确认）

1. **默认级别 = warning 默认开启**（不阻断编译，GREEN 不受影响）。
2. **仅 bool + enum**：sealed out-of-scope 已确认（事实：z42 sealed=final 无封闭子类集 + 无反向子类索引，
   不可做）。
3. **enum 按整数值比对**：别名成员（同值多名）按值天然合并——已接受这一近似。
4. **同时检查 switch 语句 + switch 表达式**（两者都查）。
