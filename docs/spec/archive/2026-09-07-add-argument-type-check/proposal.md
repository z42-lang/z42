# Proposal: 调用实参类型检查

## Why

**z42c 今天对调用实参不做任何类型检查。** 下面这段代码 `rc=0`、零诊断、产物照写、**运行期也不报错**：

```z42
namespace Probe;
class A { } class B { }
void TakeA(A a) { }
void TakeS(string s) { }
void Main() {
    TakeA(new B());     // B 传给 A
    TakeS(42);          // int 传给 string
    TakeS(new A());     // A 传给 string
}
```

缺口是全面的，三条调用绑定路径无一检查：

| 路径 | 现状 | 位置 |
|---|---|---|
| 自由函数 `f(x)` | **连 arity 都不校验**，直接 `GetFunc` 取符号 | `MemberResolver.z42:365-371` |
| 单 arity 候选方法 | `return byArity[0]`，无 assignability 检查 | `OverloadBinder.z42:232` |
| bare 兜底（全类唯一同名方法） | 同上 | `OverloadBinder.z42:225` |
| 构造器 `new C(x)` | 有 arity 检查（E0426），**无类型检查** | `ConstructTyper.z42:199` |

只有**重载决议**顺带用到类型（`OverloadResolver._applicable`），但那只在同 arity ≥2 个候选时才跑；候选唯一
时直接返回，类型从不参与。

### 为什么这条要现在做

1. **它决定后续所有欠债估算的分母。** [[restore-emit-zbc-diagnostics-program]] 已查明 70 文件 / 469 条
   欠债；那 469 条是在「实参不检查」的前提下数出来的，**是下界不是上界**。不先补上这一项，后续
   B/D/A/C 各 bug 的欠债面无法定量。
2. **它解释了为什么 A/B/D 那几类 bug 在调用点从不报错**——赋值处报的错，换成传参就消失了。
3. **机制早已就位、只差接线。** `TypeChecker.CheckImplicitConvert` 的注释白纸黑字写着覆盖
   「赋值 / return / **传参**」（`TypeChecker.z42:198`），但全仓只有 3 个调用点：return、var-decl、assign。
   **传参那一项从来没接上。** 这正是本程序反复记录的「没有东西盯着的断言迟早变成谎言」。

## What Changes

- 在调用实参绑定的通用汇聚点接上既有的 `CheckImplicitConvert`，上下文串为 `"argument"`。
  **不引入新诊断码、不引入新转换规则**——与赋值/return 走同一条 `Conversion.Classify(...).ImplicitOk()` 门。
- 修复开启该检查后暴露的 **5 个根因**（详见 design.md）。这些不是"被检查制造出来的错误"，而是
  **本来就存在、只是没有任何东西在看**的编译器缺陷。
- 补负例测试（每个根因 + 三条调用路径各自的正例/负例）。

## 爆炸半径（已实测，非估算）

用探针编译器（把 `CheckImplicitConvert` 接到实参汇聚点）实编全仓，**cache-cold**：

| 语料 | 规模 | 新增实参诊断 | 波及文件 |
|---|---|---|---|
| stdlib（`src/libraries/`，27 库） | 716 文件 | **39** | 9 |
| z42c 自身（`src/compiler/`，5 子包） | 135 文件 | **1** | 1 |
| 测试语料（`src/tests/` + `examples/` 单文件路径） | 299 文件 | **54** | **14** |
| **合计** | | **~94** | **24** |

> ⚠️ **量测陷阱（已踩并纠正）**：`z42c build` **跳过 cached 文件 → 不打印其诊断**。首轮 warm 扫描
> 25 个包里多数 `cached: N/N`，得出的 37 条是**低估**。上表为清空全部 `.cache` / `dist` 后的冷跑结果。

**~94 条命中里，没有一条是真实的用户类型错误**——全部落在 6 个既存编译器根因上：

| # | 根因 | 条数 | 状态 |
|---|---|---|---|
| **R1** | **跨包 imported 泛型签名丢失型参身份**（`T` 读回成名为 `"T"` 的普通类，非 `Z42GenericParamType`） | **79** | 🆕 本次发现 |
| R2 | `X[]` / `T[]` → `Array` 不放行 | 4 | 已知 **bug A** |
| R3 | 限定类型名固化成字面量 `"unknown"` | 4 | 已知 **bug B3** |
| R5 | 无目标 lambda 推成 `Func<<unknown>>` 而非 `Action` | 3 | 🆕 本次发现 |
| R6 | enum ↔ 底层整数不可转（`GCHandleType`） | 2 | 已知 **bug D** |
| R7 | `Func<int>` ≠ `Func<Int32>`（`Z42FuncType.IsAssignableTo` 用 `Dump()` 逐字比、不 `Canon`） | 4 | 已知 **bug C** |

> **R1 一条占 84%。** 它同时解释了 stdlib 的 `X[]→T[]`（`Array.Copy<T>`）、
> `int→T`（跨包泛型接口 `IBasicCollection<int>.AddOne`）、以及测试语料里 52 条
> `Action<Int32>→Action<T>` / `Func<...>→Func<TArg,TResult>`（`delegates/` + `closures/` 全簇）。
>
> **R1 只能经传参触达**——泛型方法体内无法凭空造出具体类型的值赋给 `T[]`，所以它在赋值上下文
> 永远不出现。这正是它长期无人发现的原因，也正是本变更的价值。
>
> **R2 / R7 已独立坐实为既存 bug**（在**赋值**上下文即可复现，与本变更无关）：
> `Array boxed = x;` 报 `E0402 (var-decl)`；`closure_l3_loops.z42:58` 报
> `cannot assign Func<int> to Func<Int32> **(assign)**`——只是 `--emit-zbc` 一直吞掉了它们。
>
> 测试语料 83 个失败文件中，**仅 14 个是本检查独有**（其余是本程序已知的 E0404 私有成员机械欠债
> 414 条等）；这 14 个全部落在 `delegates/` + `closures/`，即 R1 + R7 的簇。

### R1 的修复代价：无格式 bump（已核实）

`ZbcReader.z42:531-541` 读方法级 tp 块时，**型参名（pool 索引）被读出后直接丢弃**（`c.U32();`）。
`add-array-paired-sort` 曾用同样手法把 tp **个数**捕获出来并明确记「SIGS 早已存 tp 块，**无格式
bump**」。因此 R1 = 把名字一并捕获 + 喂进 `ImportedSymbolLoader._resolve` 的型参表：

- **不改 wire 格式** → 不 bump zbc/zpkg minor
- **不受 [bootstrap-seed.md](../../../../.claude/rules/bootstrap-seed.md) 两-nightly support/use 纪律约束**

`_resolve` 现有两个重载：2 参版传**空**型参表（`ImportedSymbolLoader.z42:435-437`，用于自由/静态
方法）、4 参版只喂**类级**型参（`:334/348/349`）。`Array.Copy<T>` 是「类无型参 + 方法级型参」，
两条路都漏——与实测完全吻合。

## Scope（允许改动的文件）

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `src/compiler/z42c.semantics/src/OverloadBinder.z42` | MODIFY | 汇聚点接检查（`FillDeferredArgs` → `BindArgsToSignature`） |
| `src/compiler/z42c.semantics/src/MemberResolver.z42` | MODIFY | 汇聚点改名调用点；R4 形参类型代换 |
| `src/compiler/z42c.semantics/src/Conversion.z42` | MODIFY | R2 `Array` 目标；R6 enum ↔ 整数 |
| `src/compiler/z42c.semantics/src/Z42Type.z42` | MODIFY | R7 `Z42FuncType.IsAssignableTo` 改走 `Canon` |
| `src/compiler/z42c.semantics/src/ExprTyper.z42` | MODIFY | R5 lambda 实参延迟绑定 |
| `src/compiler/z42c.semantics/src/ConstructTyper.z42` | MODIFY | 构造器实参类型检查（D4） |
| `src/compiler/z42c.semantics/src/ImportedSymbolLoader.z42` | MODIFY | R1 方法级型参进 `_resolve` 表；R3 `"unknown"` 固化 |
| `src/libraries/z42.ir/src/BinaryFormat/ZbcReader.z42` | MODIFY | R1 捕获 tp 块里的型参名（现读弃；**无格式 bump**） |
| `src/libraries/z42.ir/src/ExportedTypes.z42` | MODIFY | R1 `ExportedMethodZ` 增 `TypeParams`（同 `TypeParamCount` 手法，ctor 元数不变） |
| `src/libraries/z42.ir/src/TsigReconcile.z42` | MODIFY | R1 型参名从 `SigEntryZ` 传到 `ExportedMethodZ` |
| `src/compiler/z42c.semantics/tests/typecheck/argument_type/argument_type_tests.z42` | NEW | 负例/正例门 |
| `docs/book/src/compiler/source-compile.md` | MODIFY | 机制页：实参检查的汇聚点与残留洞 |
| `docs/spec/changes/add-argument-type-check/**` | NEW | 本变更容器 |

**只读引用**：

- `src/compiler/z42c.semantics/src/TypeChecker.z42` — `CheckImplicitConvert` 契约
- `src/compiler/z42c.semantics/src/OverloadResolver.z42` — 重载决议里的 `_applicable`
- `src/libraries/z42c.core/src/DiagnosticCodes.z42` — 确认复用 E0402 / E0439

## Out of Scope

- **不新增诊断码**——复用 E0402（无转换）/ E0439（存在显式转换缺 cast）。
- **不收紧重载决议规则**（`OverloadResolver._applicable` 的白名单不动）。
- **loose-bind / stub 接收者路径不补检查**——那些路径签名确实不可知（见 design.md 残留洞）。
- 欠债表里 B / D / B1 / B2 / B5 / B7 / B8 等其余 bug **不在本变更**，按原计划后续处理。
- 不开 P1 的门（`--emit-zbc` 打印诊断）——那是本程序第 ⑧ 步，仍排在最后。

## Open Questions（已裁决 2026-09-06）

- [x] **Q1（主裁决）→ 一个 PR 全做。** 6 个根因 + 接线 + 负例门作为**一个原子逻辑单元**交付：
      任何提交点上树都自洽，不出现"根因修了但检查没开"或反之的中间态。
      **代价与对策**：diff 跨 8 文件 / 6 根因，且 main 被并发会话高频推进 →
      **实施期间尽量少 rebase，合并前一次性 rebase + 完整 GREEN**（见 tasks 阶段 5）。
- [x] **Q2 → enum ↔ 底层整数要求显式 cast（对标 C#）。** `(GCHandleType)n` / `(long)e`；
      enum **成员**引用（`GCHandleType.Weak`）仍免 cast。理由：与 z42 已确立的
      `tighten-implicit-conversions`（窄化 / 有损须显式）同向，不弱化 enum 的类型区分度。
      落地：`GCHandle.z42` 两处调用点加显式 cast。
- [x] **Q3 → 构造器同批接类型检查。** `new C(x)` 与 `f(x)` 口径一致，不留不对称洞。
