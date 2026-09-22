# Design: definite assignment

## Architecture

```
DeclBinder._bindMethodBody(...)  ──→ BoundStmt 树
                                        ↓
                          FlowAnalyzer.CheckDefiniteAssignment(body, params, diags)
                                        ↓
                          两个互相调用的遍：
                            · _stmt(s)     —— 状态机（uninit / assigned 两个集合）
                            · _reads(e)    —— 表达式里读到未赋值的名字 → E0407
                          外加一个纯函数：
                            · AlwaysReturns(s) —— 「这条语句必定 return/throw 吗」
```

**不新建 visitor 设施** —— z42c 没有 Bound 树 visitor（`xtask test walkers` 覆盖的是 AST 侧），
按仓里惯用的 `s is BoundXxx` 链递归，与 `StmtEmitter._emitStmt` 同形。

---

## Decisions

### D1：照搬老实现的 join 规则

`f8ff73d59^:src/compiler/z42.Semantics/TypeCheck/FlowAnalyzer.cs`（528 行）是同一个语言、
同一个项目的完整参考，join 规则连同取舍注释都在。逐条移植而不是重新发明：

| 结构 | 规则 | 为什么 |
|---|---|---|
| `if`/`else` | 都正常结束 → **交集**；一支必定退出 → 取另一支；都退出 → 不传播 | 只有两条路都赋了才算「必定赋值」 |
| `if` 无 `else` | then 的赋值**不传出** | 条件假时会落下来 |
| `while` | 体内赋值不传出 | 可能零次执行 |
| `do-while` | 体内赋值**传出** | 必执行一次 |
| `switch` | 各 case 的**交集**（非正常结束的跳过） | 同 if/else |
| `try`/`catch` | try 里的赋值在 catch 里**不算** | 异常可能在赋值语句之前抛出 |

最后一条最容易漏，且漏了就是**漏报**（该报不报），不是误报。

### D2：`out` 那半不移植

老实现近一半是 `out` 形参的 callee 端 DA。`simplify-ref-parameters`（#728）砍了 `out`；
且被 `ref` 取址的无 init 局部会在**声明处**回填 `default(T)` ⇒ `ref` 形参与被取址的局部
恒为已赋值。⇒ 这半整块不要，形参一律预置为 assigned。

### D3：误报是零容忍

DA 的价值前提是**误报率接近零**——「用了没赋值的变量」是确定性事实。
任何误报都说明 join 规则漏了一种结构，不是「规则太严」。

⇒ 摸底阶段（Q1）的命中必须**逐条判读**，不能因为「太多了」就放宽规则。

### D4：漏报可以接受，误报不可以

不覆盖的结构（如某个新 Bound 节点忘了处理）应当**保守地当成「赋了」**而不是「没赋」——
漏报只是少抓一个 bug，误报会拦住合法代码。

⇒ 未知节点的默认行为：递归其子表达式做 reads 检查，但不改变 assigned 集合。

### D5：lambda / 局部函数体

老实现对 `BoundLambda` 的处理需要核对：lambda 体里的赋值不应传到外层，
外层的 assigned 也不该直接给 lambda（捕获时机不同）。
⇒ v1 保守：**进 lambda 体只做 reads 检查，用外层 assigned 的快照；不回传赋值**。

---

## Risks

| 风险 | 缓解 |
|---|---|
| **误报**（D3） | 摸底逐条判读；任一误报即回头补 join 规则 |
| 现存代码积累的真·未初始化读 | Q1 摸底；数量大时与 User 商量分批 |
| 覆盖不全（58 个 Bound 节点） | D4 的保守默认；`switch` / `try` / `foreach` 等复合结构逐个对照老实现 |
| 性能（每个方法体多走一遍树） | 纯遍历、无分配密集操作；必要时看 `xtask bench` |
