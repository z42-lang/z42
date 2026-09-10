# 修前基线实测（2026-09-10，main = a36566bf，编译器为本树自建、非旧 nightly）

> ⚠️ 这些结论**必须用当前 main 的编译器测**。我第一轮用 `.z42/` 里的旧 nightly z42c 测，
> 得出「函数类型实参的推断失败 ⇒ CheckMethod 根本不跑」——**是假的**，旧种子早于推断阶段 C。
> 同一族坑见 [[verify-conclusion-after-reseeding]]。

## ① 违反 func 约束今天完全不报（`probe_before.z42`）

四种违反（形参类型不符 / 返回类型不符 / arity 不符 / 字面量形态返回不符）
→ `z42c --emit-zbc` **REAL_EXIT=0，零诊断**。注意这是**开门之后**（#550）测的，
不是被 `--emit-zbc` 吞掉，是真的没有任何代码路径检查它。

## ② 有运行期后果，不是纯洁癖（`probe_rt2.z42`）

```z42
void Run<T>(T handler, int x) where T: Action<int> { handler(x); }
Func<string,int> g = s => { Console.WriteLine("len=" + s.Length); return 1; };
Run(g, 3);
```

- 编译：`COMPILE=0`（干净）
- 运行：`uncaught exception: VCall: expected object, got I64(3)`
        `at Main__lambda_0 (line 3, col 33)` / `at Run (line 1, col 54)`

即 `int 3` 被直接喂进 `string s` 形参。换成 `"[" + s + "]"` 则**不崩**、静默打印 `got: [3]`
——错值一路流下去。

## ③ 可达性：哪些调用形态真的走到 `CheckMethod`（`probe_reach.z42`）

把 func 约束换成不存在的 `IFooo`，看声明级 E0443 是否发出（发出 = `_fillBundle` 被走到）：

| 形态 | 结果 |
|------|------|
| `void RunA<T>(T h, int x)` + `RunA(inc, 7)`（单型参，实参是 `Action<int>`） | ✅ 报 E0443 ⇒ **推断成功、校验路径可达** |
| `bool AllC<T,U>(T pred, U[] items)` + `AllC(isEven, xs)` | ✅ 报 E0443 ⇒ 可达（`U` 由 `xs` 推出） |
| `R ApplyB<T,R>(T f, int x)` + `ApplyB(doubler, 5)` | ❌ 零诊断 ⇒ `R` 不出现在任何形参位 → `TypeArgInference` 边界 ① 整体失败 → **不可达** |

⇒ 本轮新诊断在 5 个既有 `func_constraint_*.z42` 里，`basic` / `captured` 的 `Apply<T,R>`
那两条**天然不参与**（推断失败），其余参与。

## ④ 🔴 book 的「顶层函数的 where 不校验」是**过期断言**（`probe_toplevel.z42`）

`docs/book/src/language/generic-constraints.md` 「### 3. 顶层函数的 `where` 不校验 —— 只有类的
成员方法走方法级校验路径。Deferred：`where-constraint-future-toplevel-func`」。

实测顶层泛型函数**两半都跑**：

```
probe_toplevel.z42(9,5):  E0402: type argument `Plain` for `T` does not satisfy constraint `enum` on `TakesEnum`
probe_toplevel.z42(10,5): E0402: type argument `NeedsArg` for `T` does not satisfy constraint `new()` on `TakesNew`
```

⇒ 该 Deferred 与该节应随本 change 一并订正（又一条「没有东西盯着的断言变成谎言」）。
