# Design: 多维索引器使用侧 `obj[a, b]`

## Architecture

```
源码 m[r,c]=v
  │  ExprParser 后缀 [ 分支：循环解析逗号分隔下标
  ▼
IndexExpr { Target, Expr[] Indices, int IndexCount }   ← AST（本次改载荷）
  │  ExprTyper
  ├─ 写 _bindAssign（Op=="=" 且 Target is IndexExpr）
  │     recv 是类且有 set_Item → BoundCall set_Item(indices..., value)   [N+1 实参]
  ├─ 读 _bindIndex
  │     recv 是数组 → BoundIndex(Indices[0])（IndexCount!=1 报错）
  │     recv 是类且有 get_Item → BoundCall get_Item(indices...)          [N 实参]
  ▼
BoundCall（既有，支持任意实参数）→ IrGen vcall（get_Item/set_Item lowering 已就绪）
```

改动集中在 **AST 载荷 + Parser 解析 + Typer 路由**；Bound/IrGen/VM 全部复用既有能力。

## Decisions

### Decision 1: IndexExpr 载荷用数组，单构造器

**问题：** `IndexExpr` 原携带单个 `Index`；多维需要携带 N 个下标。

**选项：**
- A — 保留单 `Index` + 加 `Expr[] ExtraIndices`：语义割裂，消费端要拼两处。
- B — 改为 `Expr[] Indices` + `int IndexCount`，单一构造器，所有下标同构存放。
- C — B 之上再加一个单下标便捷构造器（ctor 重载）。

**决定：** 选 **B**。所有下标统一存 `Indices[0..IndexCount)`，消费端语义单一。不引入 ctor 重载
（现有 AST 每类恰一个构造器，风格一致）；集合字面量脱糖的 4 处单下标合成点经 ExprTyper 私有
helper `_ix1(target, index, sp)` 包成 1 元素数组，保持合成代码可读。

### Decision 2: 数组路径仍单下标，多下标显式报错

**问题：** 数组 `a[i]` 走 `BoundIndex`（单下标）；`a[i, j]` 该如何处理？

**决定：** 数组路径取 `Indices[0]`，并断言 `IndexCount == 1`；`IndexCount > 1` 时报
`TypeMismatch` 诊断「数组不支持多维下标」。z42 数组是单维 jagged（多维用 `a[i][j]`），
多维数组 `int[,]` 在 Out of Scope。不 panic、不误派发到不存在的 get_Item。

### Decision 3: 不引入重载解析，靠声明参数个数天然匹配

**问题：** 多个 `this[...]` 重载？

**决定：** 本次不做（Out of Scope）。一个类仍限一个索引器（现状即如此，`get_Item`/`set_Item`
按名唯一）。N 个下标的调用天然匹配该唯一索引器的 N 参声明；参数个数不符由既有 vcall/类型检查
在下游暴露，无需本次新增重载解析。

## Implementation Notes

- **`Ast.z42` IndexExpr**：`public Expr[] Indices; public int IndexCount;` 构造器
  `IndexExpr(Expr target, Expr[] indices, int count, Span span)`；`Dump()` 输出
  `(index <target> <idx0> <idx1> ...)`，逐下标拼接（golden 对账需稳定）。
- **`ExprParser.z42` 后缀 `[`**：`_advance()` 吃 `[` 后，`while peek != ] && != Eof` 循环，
  `pc>0` 先 expect `,`，解析 `_parseExpr(0)` 存入定长缓冲（如 `Expr[8]`，与索引器声明参数上限
  一致的宽松上界），expect `]`，`new IndexExpr(left, buf, pc, left.Span)`。
- **`ExprTyper._bindAssign` 写路由**：绑定全部 `aix.Indices[0..IndexCount)` → `BoundExpr[] sa`
  大小 `IndexCount+1`，前 N 为下标、末位为 value，`BoundCall(..., "set_Item", sa, IndexCount+1, ...)`。
- **`ExprTyper._bindIndex` 读路由**：数组分支 `IndexCount==1` 取 `Indices[0]` 建 `BoundIndex`，
  否则报错；索引器分支绑定全部下标 → `BoundExpr[] ia` 大小 `IndexCount`，
  `BoundCall(..., "get_Item", ia, IndexCount, ...)`。
- **helper `_ix1`**：`private IndexExpr _ix1(Expr t, Expr i, Span sp) { Expr[] a = new Expr[1]; a[0] = i; return new IndexExpr(t, a, 1, sp); }`，替换 516/579(×2)/589 的单下标 `new IndexExpr`。

## Testing Strategy

- **单元（parser）**：`m[a, b]` / `x[a, b, c]` 解析出 `IndexCount==2/3`；`v[i]` 仍 `IndexCount==1`。
- **单元（codegen）**：`m[r,c]` 读 dump `get_Item(...)` 2 实参；`m[r,c]=v` 写 dump `set_Item(...)` 3 实参。
- **Golden e2e**：`src/tests/e2e/indexer_multidim/` —— Matrix 类双下标读写，打印验证数值正确。
- **回归**：`xtask test` 全 stage（含 stdlib List/Dictionary 单下标索引器、数组下标）必须逐字节全绿。
- **自举**：纯前端、无格式 bump → z42c 自举不动点 gen1==gen2 应保持（`xtask test compiler`）。
