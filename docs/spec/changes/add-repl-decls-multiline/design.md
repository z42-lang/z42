# Design: REPL 多行输入 + 顶层声明累积

## Architecture

```
z42i (interactive_main.z42)
  read 循环:  Repl.ReadBlock(">>> ", "... ")   ← 改：单行 ReadLine → 括号平衡多行块
      │  (block 文本, 已括号平衡)
      ▼
  Script.Eval(state, block)                     ← 改：分类多一类「顶层声明」
      │
      ├─ using 声明        → 累积到 state.Usings（现状）
      ├─ 顶层声明 (新)     → emit 进 Repl.R{N} + ExtendWithPackage + 登记 DeclNamespaces + 重名检测
      ├─ 变量声明          → Vars{N} carry-forward（现状）
      ├─ 语句 / 表达式     → 现状
      ▼
  prelude = namespace Repl.R{N};
            using <每个 state.Usings>;
            using Repl.R{VarsRound};            ← 变量类（现状）
            using <每个 state.DeclNamespaces>;  ← 新：先前声明所在 ns，逐个引入
            [声明轮] <用户声明原文>
            [变量轮] public static class Vars{N} { ... }（现状）
            public static object Eval{N}() { <body> }
      ▼
  PackageCompile.Compile(CachedScan) → ToBytes → Engine.LoadBytes → Engine.Invoke
```

关键洞察：**「carry-forward」推广**——变量靠「`Vars{N}` 静态字段 + `using` 前一轮类」跨轮存活；
fn/class 靠「定义留在 `Repl.R{N}` + 后续轮 `using Repl.R{N}`」跨轮可见。二者正交、可组合，且**都复用
既有的 `ExtendWithPackage` 内存增量并入**，不新增编译原语。

## Decisions

### Decision 1: 声明累积机制 —— 命名空间登记 + `using` 引入
**问题：** 用户定义的 fn/class 如何跨轮可见？
**选项：**
- A — 声明留在本轮 `Repl.R{N}`，把 `Repl.R{N}` 登记进活跃集，后续轮 prelude 逐个 `using`。
- B — 每轮重建一个「单一增长命名空间」含全部历史声明（原 Growing Transcript）。
- C — 反射/组件化动态注入重定义。
**决定：** 选 **A**。B 是 perf-optimize-repl-eval 已否决的 O(n) 全量重编老路；C 需 componentized-runtime、
overkill。A 完全复用变量 carry-forward 已验证的 `Repl.R{N}` + `ExtendWithPackage` + `using` 基建，
零编译器改动，且与变量、`using` 累积天然共存。

### Decision 2: 同名重定义 —— MVP 报错，不 supersede
**问题：** 用户重定义已有 fn/class 名怎么办？
**选项：** A — 报错拒绝；B — supersede（后轮覆盖、旧 ns 从 using 集剔除）。
**决定：** 选 **A**（User 2026-07-27 裁决）。B 需维护「符号名→最新归属 ns」并在 using 集里剔除旧 ns
防歧义，工作量与风险更高；MVP 先报错。B 记入 Deferred `repl-future-redefine`。
**副作用：** 因不 supersede，活跃声明 ns 集只增不减，`using` 集中每个符号名唯一 → 不会触发
`GetStaticScoped` 的跨 ns 歧义。重名检测在 `Eval` 编译前用 `state.DeclNames` 拦截。

### Decision 3: 声明名是否经 Rewriter 改写
**问题：** 变量裸引用要改写成 `Vars{N}.x`（静态字段限定 E0401）；声明名是否也要？
**决定：** **不改写**。fn/class 名不是「某个类的静态字段」，而是命名空间成员，靠 `using Repl.R{N}`
引入活跃集后裸引用即可解析（与自由函数 `Eval{N}` 同类）。Rewriter 仍只改写 `state.VarNames`。
（阶段 1 实测确认；若证伪则回本决策补限定，属实现细节不改 spec。）

### Decision 4: 多行输入 —— 复用 `__repl_readblock`，宿主只换调用
**问题：** 多行怎么读？
**决定：** 底座（括号平衡、字符串/注释感知、EOF→null）已在 [repl.rs](../../../../src/runtime/src/corelib/repl.rs)
与 extern `Std.Repl.ReadBlock` 就绪。宿主把 `ReadLine(">>> ")` 换成 `ReadBlock(">>> ", "... ")` 即可，
`null`（EOF）语义不变（现有 `if (line == null) break` 直接适用于块）。零 VM 改动。

### Decision 5: 声明轮的 body 与返回值
**问题：** 声明轮 `Eval{N}()` 该返回什么？
**决定：** 声明本身无求值结果 → body 为 `return null;`、`HasValue=false`（与语句轮一致，宿主抑制打印）。
声明的「执行」是把类型/函数注册进 VM（由 `LoadBytes` 完成），`Invoke(Eval{N})` 仅触发本轮包加载。

## Implementation Notes

- `_classify` 扩展（token 级，保守判定）：
  - 首 token ∈ {`class`,`struct`,`record`,`enum`,`interface`}（含可选前缀 `public`/`private`/`internal`
    修饰符后再看关键字）→ 顶层类型声明，符号名 = 关键字后第一个 Identifier。
  - `<Identifier> <Identifier> (`（第三 token 为 `LParen`）→ 自由函数声明，符号名 = 第二个 Identifier。
    与变量声明 `<Identifier> <Identifier> =` 区分点在第三 token（`(` vs `=`）。
- `ScriptState` 新增：`List<string> DeclNamespaces`（存 `"Repl.R{N}"` 串）、`List<string> DeclNames`
  （已声明符号名，重名检测）。构造器初始化为空 List。
- `Eval` 声明轮流程：① 用 `DeclNames` 查重名 → 命中则 `return EvalResult(false, …, "symbol already defined")`；
  ② 组装 prelude（含全部 `DeclNamespaces` 的 `using` + 用户声明原文）+ `Eval{N}(){ return null; }`；
  ③ 编译（失败则会话不推进）；④ `ToBytes` → `ExtendWithPackage(CachedScan, bytes, "repl_r{N}")` →
  `LoadBytes` → `Invoke`；⑤ 推进：`Counter=N`、`DeclNamespaces.Add("Repl.R{N}")`、`DeclNames.Add(sym)`。
- prelude 里 `DeclNamespaces` 的 `using` 对**所有轮**（不止声明轮）都要加——否则表达式轮引用不到已定义
  的 fn/class。位置：在现有 `using Repl.R{VarsRound}`（变量类）之后追加。
- perf 不回归：声明轮才 `ExtendWithPackage` + 前进；表达式/语句轮仍只 `using` 现有集、不并入、不发新类
  （保持 perf ⑤ 的 O(1) 每轮）。声明 ns 集随声明数增长，`using` 行数 = 声明数，属线性且远小于 stdlib 规模。

## Testing Strategy

- **单元 [Test]**（scripting `tests/` 或就近）：声明函数→调用、声明类→实例化、声明与变量共存、重名报错、
  声明编译失败会话不破坏 —— 逐条对应 spec scenario。
- **端到端**：多行块（`fn ... {` 续读）走 REPL `-c` 或脚本化 stdin 的 e2e 夹具，验证跨行读入 + 求值。
- **回归护栏**：现有 REPL 变量 carry-forward / using 累积用例必须仍绿（声明机制不得回归它们）。
- **GREEN gate**：`xtask test`（完整 stage：e2e + cross-zpkg + stdlib + compiler + vscode-syntax）。
  toolchain 改动经 `xtask test stdlib`/相关 e2e 覆盖；冷路径不涉及。
