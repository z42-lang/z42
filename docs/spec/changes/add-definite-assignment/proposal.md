# Proposal: definite assignment —— 变量未赋值就读要报错

## Why

`DiagnosticCodes.UninitializedVariable = "E0407"` 这个常量**在 z42c 里就摆着**，
`docs/reference/src/appendix/error-codes.md` 也记着它：

```
| E0407 | 变量未初始化就使用 | ⚠️ 零发射点 | — |
```

**零发射点**——常量在、文档在、发它的那个 pass 不在。

### 根因：自举移植时整个 pass 丢了

老的 C# 宿主编译器有 `src/compiler/z42.Semantics/TypeCheck/FlowAnalyzer.cs`，**528 行**，
含完整的可达性分析（`AlwaysReturns`）+ definite assignment（`CheckDefiniteAssignment`）+
visitor 派发。`f8ff73d59`（"delete src/compiler — the C# bootstrap compiler is gone"）
把它连同整个 C# 编译器删掉了，而 z42c 这边**从来没有补上**。

这与 `simplify-ref-parameters` 发现的三态塌缩是同一条线索的两个样本：
**archive 里 Scope 表全是 `.cs` 的 spec，其声称的能力在 z42c 里未必存在。**

### 现在的后果

```z42
int x;
Console.WriteLine(x);     // 编译通过；运行期读到 Null
```

`enforce-value-type-non-null`（#741）给**字段**建立了零值保证，但**局部变量**不在其列——
它们既没有零初始化（刻意的：零初始化会把「忘了赋值」从响变成静默），也没有 DA 兜底。
⇒ 今天是两头落空。

### 为什么它是下一步

它同时服务三件事：

1. **补回 E0407**，让「忘了赋值」在编译期响
2. **`define-null-check-marks` 的设施前提** —— 空值流分析需要同一套「结构化数据流 +
   正常结束分析 + join 规则」，而 DA 的**误报率天然接近零**（「用了没赋值的变量」是确定性事实，
   不是概率判断），是验证引擎的理想第一刀
3. 老实现可直接当参考（算法、join 规则、注释里的取舍全在）

## What Changes

### 新增 DA pass（语义层，绑定之后）

对每个方法体走一遍 `BoundStmt` 树：

- 无初始化器的局部 → 记入 `uninit`
- 赋值 → 记入 `assigned`
- **读**一个在 `uninit` 且不在 `assigned` 的名字 → `E0407`

### join 规则（照搬老实现，注释里有取舍依据）

| 结构 | 规则 |
|---|---|
| `if` / `else` | 两支都正常结束 → **交集**；一支必定 return/throw → 取另一支；都必定退出 → 不传播 |
| `if` 无 `else` | 条件假时会落下来 ⇒ then 里的赋值**不算** |
| `while` | 循环体可能零次执行 ⇒ 体内赋值不传出 |
| `do-while` | 体必执行一次 ⇒ 赋值传出 |
| `switch` | 各 case 的**交集**（非正常结束的 case 跳过） |
| `try` / `catch` | try 里的赋值在 catch 里**不算**（异常可能在赋值语句之前抛出） |

「一支必定 return/throw」需要**正常结束分析**（`AlwaysReturns`），老实现里是独立的一遍。

### 不需要的部分

老实现有一半是 **`out` 形参的 callee 端 DA**（Decision 5/6）。
`simplify-ref-parameters`（#728）**把 `out` 砍了**，且被 `ref` 取址的无 init 局部会在声明处
回填 `default(T)` ⇒ `ref` 形参恒为已赋值。⇒ 这半整块不要。

## Scope（允许改动的文件）

| 文件 | 变更 |
|---|---|
| `src/compiler/z42c.semantics/src/FlowAnalyzer.z42` | NEW —— DA + 正常结束分析 |
| `src/compiler/z42c.semantics/src/DeclBinder.z42` | 方法体绑定后调用该 pass |
| `src/compiler/z42c.semantics/tests/typecheck/definite_assignment_tests.z42` | NEW |
| `docs/reference/src/appendix/error-codes.md` | E0407 从「零发射点」改为实装 |
| `docs/reference/src/language/*` | 局部变量必须先赋值 |

## Out of Scope

- **可空性流分析** —— 本变更只建设施 + DA 一条规则；`?` 标记走 `define-null-check-marks`
- **字段 / 静态字段的 DA** —— 它们零初始化（#741），不在 DA 管辖
- 「传 `ref` 前未赋值」的警告 —— 被 ref 取址的局部已回填零值，该警告无从触发（见上）

## Open Questions

**Q1：现存代码里有多少处会被判红？**

老实现在 C# 编译器时代是开着的，但 z42c 从未有过 ⇒ **z42c 自身 + stdlib 是在没有这条检查的
环境里长出来的**，可能积累了真实的未初始化读。

⇒ 必须先摸底：加上 pass、跑**全量**（不是只 `build stdlib` —— `enforce-value-type-non-null`
那次的「0 命中」是缓存骗的），逐条判读是真 bug 还是需改写。
