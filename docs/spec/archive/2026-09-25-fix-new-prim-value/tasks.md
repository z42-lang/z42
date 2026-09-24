# tasks：fix-new-prim-value

状态：🟢 已完成（2026-09-25）

## 代码（两处，各管一条发射路径）
- 🟢 `ConstructTyper._bindNew`（编译期，类型已知）：`IsScalarType()` → 折成 `BoundDefault(t, -1)`
  （**与 `default(t)` 复用同一个 Bound 节点**）；`string` → `BoundLitStr("\"\"")`。
  放在 `n.Type == null` / `!= null` 两支**汇合之后** ⇒ target-typed `int x = new();` 同样命中。
- 🟢 同处：`ArgCount != 0` → **E0426**（既有码），消息用 surface 拼法（`int` 而非 `Int32`），
  `string` 附带 `String.FromChars` 指引。
- 🟢 `builtin_activator_create`（运行期，`new T()` 且 T 为方法级型参）：先认基元包装类 →
  `default_value_for(td.name)`；`Std.String` → `""`。
- 🟢 `well_known_names::is_scalar_prim_wrapper`（新）：12 个标量包装名，注释里钉住
  「与编译期 `PrimModel.Code 0..=11` 是同一集合的两侧表述，改一侧必须改另一侧」。

## 测试
- 🟢 e2e golden `src/tests/basic/new_prim_value.z42`（assert-only，interp + JIT 两模式）：
  bool 的三条互相矛盾断言 / 数值与 char 直接形态 / `string` 构造 ≠ default /
  target-typed `new()` / 泛型 `make<X>()` 六格 / 用户 struct·class 阴性对照。
- 🟢 诊断单测 `z42c.semantics/tests/typecheck/new_prim/`（5 条）：三条钉 E0426（含
  「消息用 surface 拼法」那条——第一版实现真打出了 `new Int32()`），两条护栏（零实参不报、
  用户类实参仍走 ctor 解析）。

## 验证
- 🟢 **阴性对照实跑**：退回两处改动重建 → 诊断单测 3 条精确变红、2 条护栏保持 PASS；
  e2e golden 在**第 28 行**（`Assert.True(!b)`）就红（`expected bool in register %1, got Object(Std.Boolean)`）。
- 🟢 完整 GREEN：`xtask test all` / `runtime`（改了 VM，单列一跑）/ `examples` /
  `diagcodes` / `walkers` / `docs` / `lines` **全部 exit 0**；z42c 自举不动点 3/3 byte-identical。
- 🟢 爆炸半径量测：全仓 `new <基元>(…)` 出现 **2 次，且都在注释里**
  （`Tar.z42:411` / `Zip.z42:404` 的 2026-05 踩坑记录）⇒ 生产代码零命中、无字节漂移。

## 文档
- 🟢 `reference/language/generic-constraints.md`：新增「基元满足 `new()`，构造出来的是零值」
  一节（12 格对照表 + `string` 为什么单独一格 + 修前的 `bool` 悖论 + E0426）。
- 🟢 `reference/appendix/error-codes.md`：E0426 词条补基元这一支，并写明「不拦实参的话折叠会把
  `new int(5)` 静默变成 0」这条**非加不可**的理由；顺带订正行号（`165,291` → `188,223,355`）。
- 🟢 `internals/compiler/generics.md`：订正两处过期表述（「`new T()` 泛型 body 实例化未实现」
  —— `add-generic-methods` 早已实现；`TypeChecker.HasNoArgConstructor` → `ConstraintChecker._hasNoArgCtor`），
  新增「`new T()` 走哪条路（两条）」对照表 + 「`bool` 是唯一坏值不会自己崩的标量，应作头号探针」。

## 记下来的三条
- ⭐ **旧记录把这条的边界记窄了**：记忆写的是「泛型约束 `new T()` 崩」，实测**直接写
  `new int()` 一样坏**，且 `bool` / `string` 两格是**静默**的。
  （方法论第 6 条：「某某场景坏了」往往是「更大的东西坏了，只有这个场景会暴露」。）
- ⭐ **加折叠必须同时加护栏**：只折不拦的话 `new int(5)` 会从「运行期 MissingSymbolException」
  退化成「静默 0」—— 修复本身会**造出一个新的静默洞**。
- ⚠️ **注释里的括注也会说谎**：`builtin_activator_create` 那句 `bail!` 写着
  「primitive/array/synthetic?」，读起来像「基元走不到这里」，实际基元**恒定走到**
  （`Std.Int32` 有真 handle）。与 `#724` 那条「理由已过期」同族。
