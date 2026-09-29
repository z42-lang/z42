# Tasks: E0499 —— 类型名与内建基元拼写冲突

**状态：🟢 已完成 | 开始：2026-09-29 | 完成：2026-09-30**

## 进度概览

- [x] 阶段 1: 诊断码登记 + 文档
- [x] 阶段 2: 实现（DeclEnforcer 新 pass）
- [x] 阶段 3: 测试（正面 + 阴性对照）
- [x] 阶段 4: 修既有判红点（4 个编译器 fixture）
- [x] 阶段 5: GREEN + 归档

---

## 阶段 1: 诊断码登记 + 文档

- [x] `z42c.core/src/DiagnosticCodes.z42`：加 `BuiltinTypeNameClash = "E0499"`
      （⚠️ 取号前**按当时的 main 重查一遍**空号，别用本文件里的数字）
- [x] `docs/reference/src/appendix/error-codes.md`：加一行（与登记表**双向对账**，
      状态列必须与真实发射面一致；行格式见该页既有行）
- [x] 判断要不要进 `scripts/test/diag-literal-emitters.txt`：
      **只有在发射处写字面量 `"E0499"` 而非常量时才需要**。
      自举分阶段：本 change 同时加常量与发射点 ⇒ 上一版 z42c 的 `z42c.core` 里没有这个常量
      ⇒ **发射点在过渡轮必须用字面量**，并登记进该文件（3 天宽限）。
      ✅ **实测结论（2026-09-30）**：`xtask test bootstrap` 报
      「nightly z42c compiles current source — NO staged-bootstrap boundary violation」
      ⇒ **用常量即可，不需要登记字面量欠账**。`xtask test diagcodes` 0 violations 复核。

## 阶段 2: 实现

- [x] `z42c.semantics/src/DeclEnforcer.z42`：新增 `internal void _passBuiltinNameClash(CompilationUnit cu)`
      - [x] 豁免：`cu.HasNamespace && cu.Namespace == PreludeNs.Root()` ⇒ 直接返回
            ⚠️ **实施期更正**：原写字面量 `"Std"`。并入 main 后发现新增了
            `PreludeNs.z42`（prelude ns 的唯一 SoT），改用 `Root()`，不写第四份字面量
      - [x] 遍历 `cu.Decls` → `_sc._unwrap` → `is ClassDecl` 且 `Kind != "interface"`
      - [x] 判据：`PrimModel.Code(PrimModel.Canon(c.Name)) >= 0`
            （**必须用 `Code` 不是 `IsScalarValue`** —— 后者只到 11，漏掉 `string` / `object`）
      - [x] 消息按 design「消息措辞」节，span = `c.Span`
- [x] 挂到与 `_passAttributeSuffixEnforce` **相同的三个调用点**
      （`SymbolCollector.z42:76 / :331 / :385` —— 动手前重新定位行号）
- [x] 🔴 **核实 Decision 5 的前提**：✅ 已实证 —— `--dump-ir` 显示嵌套类型 flatten 后是
      `Demo.Outer+Single`（`fn @Demo.Outer+Single.Equals$1`），`Canon` 折不动 ⇒ 豁免成立。
      ⚠️ 走 `--dump-ir` 而不是在 pass 里插打印：插打印那次把 `dist/` 搞成了混合状态
      （编译失败后 `z42.core.zpkg` 从 compiler dist 里消失，xtask 自己都起不来）。

## 阶段 3: 测试

正面（每条都要**按码断言**，否则要进 `diag-untested-codes.txt`）：

- [x] `struct Single`（顶层、非 `Std`）⇒ E0499
- [x] `class String` ⇒ E0499（钉住 `string`/`object` 那两格 —— `IsScalarValue` 会漏掉它们）
- [x] `struct Single<T>`（泛型）⇒ E0499
- [x] `[Record] struct Single(int X, int Y)` ⇒ E0499
      ⭐ 这条**此前让编译器自己崩**（uncaught exception @ `AccessEmitter.z42:557`），
      是最有说服力的一条
- [x] 声明了但**从不使用** ⇒ 仍报 E0499（钉住「挂声明位」）

阴性对照（**缺一不可**，它们是范围的唯一表达）：

- [x] `struct Solo` ⇒ 不报
- [x] `enum Single` ⇒ 不报
- [x] `interface Single` ⇒ 不报
- [x] **嵌套** `class Outer { struct Single { … } }` ⇒ 不报（钉住 Decision 5）
- [x] `namespace Std; struct Single` ⇒ **不报 E0499**（钉住豁免）
- [x] `namespace Std; struct MyThing` ⇒ 不报任何码（钉住「`Std` 不是保留命名空间」）

## 阶段 4: 修既有判红点

- [x] `z42c.semantics/tests/typecheck/prim_member/prim_member_tests.z42`
      （`:29-32` 的 `_wrappers()` 串、`:72`、`:82`、`:84`）：片段加 `namespace Std;`
- [x] `z42c.semantics/tests/typecheck/bare_type_param_member_tests.z42:21`（`class Object`）
- [x] `z42c.semantics/tests/typecheck/generic_inference/generic_inference_tests.z42:304, 334`
- [x] ⚠️ 加 `namespace Std;` 后**逐条重跑**：这些用例断言的是消息文本
      （如 ``no method `Bogus` on `Int32` ``），命名空间变化可能影响解析与措辞
- [x] ⭐ 它们走 `SemanticDump.FirstErrorCode` 那条**不链 stdlib** 的路径
      （`StmtBinder.z42:343`）⇒ 加了 `namespace Std;` 也不会撞 E0606（没有 import 可遮蔽）

## 阶段 5: GREEN + 归档

- [x] `xtask test`（18 stage）—— 含 `diagcodes` 与 `rust units`：**✅ GREEN 18/18**
      🔴 第一次跑是**假红**（`✗ workload build failed: …/desktop/appbuilder/…`）：
      `xtask.zpkg` 停在合并 main 之前，而 #948 删掉了那个工程 ⇒ **并入 main 后必须重建 xtask**。
      见 [[z42-worktree-seeding-false-failures]] 第三个触发形态。
- [x] `xtask test bootstrap`（自举边界；需 `gh` 已登录）—— 新诊断不该拒绝 stdlib/编译器自身
- [x] 文档同步三问（`doc-system.md`）：
      - 用户能看见吗 ⇒ `docs/reference/` 错误码页（阶段 1 已做）
      - 下一个接手的人不读文档能看懂吗 ⇒ 判据「问 `Canon` 不抄名单」值得落 internals
- [x] 归档：`docs/spec/archive/2026-09-30-add-builtin-name-clash-diagnostic`
      🔴 **归档必须在本 PR 内**（workflow 阶段 9 铁律；别事后单推 `docs: 归档`）

## 欠账 / 明确不做

- **不改 `PrimModel.Canon`**（根治但风险全局，理由见 design「不做」节）
- **不碰 `ZbcWriter` 无条件写空 struct 块**（它现在是信号，#947 的覆盖门靠它抓到本缺陷）
- 🔴 **本 change 只把症状变成诊断，没有让 `Demo.Single` 变得可用** —— 若将来要支持，
  那是 `Canon` 命名空间感知化，另开提案
