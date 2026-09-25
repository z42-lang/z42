# Tasks: 型参收者上的成员可用性（A 修静默错值 + B 补检查）

> 状态：🟢 已完成 | 创建：2026-09-25 | 完成：2026-09-25
> 分支 `fix-erased-blob-unbox` @ worktree `../wt-erasedblob`（起点 main `abb0bc6fb` 之后）

## 进度概览
- [x] 阶段 0: 全仓摸底（先知道 B 会打到谁）
- [x] 阶段 1: A —— 属性路补约束/Object 查找
- [x] 阶段 2: B —— 两个兜底改报错
- [x] 阶段 2b: C —— 细化约束的校验 + stdlib 补约束
- [x] 阶段 3: 测试与阴性对照
- [x] 阶段 4: 验证与文档

## 阶段 0: 全仓摸底（**必须先做**）
- [x] 0.1 先只做 A（不加 B），跑完整 `xtask test` —— A 单独不应产生任何红
- [x] 0.2 临时加 B 的诊断（只打印不判红），全量跑一遍，**列出全仓所有命中点**
      （stdlib / z42c 自举 / examples / tests）
- [x] 0.3 **「零命中」必须解释**：若某类命中为 0，说明为什么它不可达，否则交付的可能是恒不响的门
- [x] 0.4 命中点逐处定方案（加约束 / 写显式类型实参）；若某处改不动 → **停下报 User**，
      不得为了让门禁过而放宽判据

## 阶段 1: A —— 属性路补约束 / Object 查找
- [x] 1.1 `MemberResolver.z42:322`（GS5）在 Unknown 兜底前插入：① `Object` 的 `get_<Name>`
      ② `_constraintIfaceMethod(env, rt.Name(), "get_" + m.Name)`，返回类型过 `_substSelfSig`，
      产 `BoundCall.Marked` 形态。**顺序：Object 优先于约束接口**（不可反 —— 会改派发键）
- [x] 1.2 `xtask build sdk` 重建（🔴 产物在 `artifacts/.z42/`，不是 `.z42/`）
- [x] 1.3 探针实跑：`where T : IHasName` 的 `a.Name` 从 `null` 变真实值；`a.Name.Length` 不崩

## 阶段 2: B —— 两个兜底改报错
- [x] 2.1 `BoundExprOp.z42`：`BoundCall` 增 `RetIsErasedTypeParam`（ctor 初始化 false）；
      `Marked(bc, sig)` 里按 `sig.Ret is Z42GenericParamType` 置位
- [x] 2.2 `MemberResolver.z42` 方法路兜底（`:263`）：收者带标记 → **E0455**；否则 → **E0401**
- [x] 2.3 GS5 兜底同样分流（仅当收者确为 `Z42GenericParamType`；🔴 别把 Unknown 兜底整条拆了）
- [x] 2.4 措辞：E0455 指出写显式类型实参；E0401 指出加 `where`。**一个错误一条诊断**
- [x] 2.5 阶段 0.4 定下的全仓命中点：`Array.z42` 7 个函数已补 `where T : IComparable`
      （摸底预测 6 处、实测构建报 4 处 —— 另 2 处在 `z42.collections`，因 `z42.core` 先失败未走到；
      补的是 7 个，因为约束要沿调用链向上传播到公开入口）

## 阶段 2b: C —— 细化约束的校验 + stdlib 补约束
- [x] 2b.1 `ConstraintChecker._diagnoseMethodWheres`：去掉 `md.TypeParams.Count == 0` 早退；
      where 的型参**在类级型参表里** ⇒ 合法细化（不报）；两边都不在 ⇒ 报 E0401 未知型参
- [x] 2b.2 `ConstraintChecker` 新增调用点入口：按**收者的**类型实参（`Z42InstantiatedType.TypeArgs`）
      校验被方法级 `where` 细化的类级型参，违反报 **E0402**（复用 `_fillBundle` / `_checkBundle`）
- [x] 2b.3 `MemberResolver` 绑定实例方法调用时调用该入口（收者是实例化泛型类才调）
- [x] 2b.4 stdlib：`Collections/List.z42` 的 `Sort()`、`Collections/List.Query.z42` 的
      `BinarySearch(T)` 补方法级 `where T : IComparable`
- [x] 2b.5 探针：`Box<Opaque>().Cmp()` 从「零诊断+运行期崩」变 E0402；`Box<int>().Cmp()` 仍 `-1`；
      同类里 `Plain()` 在 `Box<Opaque>` 上仍可用（约束不上升为类级）

## 阶段 3: 测试与阴性对照
- [x] 3.0b C 的诊断单测：细化约束不满足 / 满足 / 同类无约束方法不受牵连 / where 挂未知名字
- [x] 3.1 诊断单测 `z42c.semantics/tests/typecheck/bare_type_param_member_tests.z42`：
      E0455 三形态 + E0401 体内形态 + **同名不同物必须报 E0455** + 一错一诊断
      （🔴 `SemanticDump` 不加载 stdlib ⇒ 用例里自己声明类型，否则是空测试）
- [x] 3.2 e2e `src/tests/generics/bare_type_param_members.z42`：A 四条正面 + C 放行面逐条
- [x] 3.3 **阴性对照（取最强的那种 = 撤回修复本身）**：撤 A → 属性用例判红；撤 B → 诊断用例判红；
      逐条确认红在哪一行（不是「整个文件红了」就算）

## 阶段 4: 验证与文档
- [x] 4.1 完整 `xtask test` 全绿（先 `xtask build sdk`）
- [x] 4.2 **不带过滤的 `cargo test --lib`**（debug）—— `xtask test` 不含它
- [x] 4.3 `xtask test compiler` 字节不动点：**A 改了属性路派发，z42c 自身若有该写法字节会漂**，
      实跑确认，不靠推理
- [x] 4.4 `docs/reference/src/language/generic-constraints.md`：成员可用性段补「不可用时报什么码 +
      两种修法」；订正该页 Deferred（本变更关闭「收者位」那半，目标位那半仍开）
- [x] 4.5 `docs/learn/src/types/generics.md`：「裸 T 上什么都调不了」从口头规则升级为会判红
- [x] 4.6 `docs/reference/src/appendix/error-codes.md`：E0455 / E0401 补触发形态（不取新号）
- [x] 4.7 spec scenarios 逐条对账
- [x] 4.8 归档：tasks 改 🟢 + `changes/` → `archive/2026-09-25-check-bare-type-param-member-access/`
      （🔴 **必须在开 PR 之前** commit 进本分支）

## 备注

### 已确认的事实（探索期实测，别重新探）

| 探针 | 实测 |
|---|---|
| `where T : IHasName` 的 `a.Name`（属性） | 🔴 `null` 静默错值 |
| 同一约束的 `a.GetName()`（方法） | ✅ `Ada` |
| 非泛型 `p.Name` | ✅ `Ada` |
| 裸 T 上 `ToString` / `GetHashCode` / `GetType` / `Equals` | ✅ 全部可用（`ToString` 正确派发 override）|
| `typeof(T)` 在泛型方法体内 | 🟡 已报 E0455 |
| `id(v).X` / `.Bogus` / `.NoSuch()` | 🔴 零诊断 + 运行期崩 |
| `id<Vec2>(v).X`（显式） | ✅ —— 靠**特化函数 `@id<Vec2>`**（两份 IR 只差 callee 名）|
| `Vec2 r = id(v); r.X` | ✅ `7`（`StructCopy` 容忍 `BoxedStruct`）|
| `id(v).Sum()` | ✅ 恰好能跑（VCall 动态派发）—— **B 会让它判红** |

### 🔴 明确不碰（User 2026-09-25 裁决「泛型的不处理」）

- 「blob 型参 + 对实例化泛型类调任何方法 ⇒ ctor 运行期解析不到」（`GBox<Vec2>` 调 `Tag()` 就崩，
  `GBox<int>` / `GBox<One>` 都好）——属泛型特化线，且 `wt-geninst` 有别的会话正在做。
- 让 `id(v).X` 真的工作（需特化）。
- ④a 泛型约束运算符派发的 sret ABI 错位。

### 🔄 实施中被实测改判的三处（留痕，别当成原计划）

1. **B 的范围收窄**：初版按字面全面执行规则，**打红两条本来全绿的 e2e**
   （`generic_constraints` 的 `var m = Max(a,b); m.value` —— `Num` 是 class，今天正常打 7；
   `generic_baseclass` 的 `where T : Animal` 的 `pet.legs` —— 我只查了接口约束、漏了基类约束）。
   ⇒ User 裁决收窄到「成员名全仓不存在」。**存量 e2e 救了一次**：我自己的诊断单测全绿却照不到
   这两种形态。连带把只为区分两种修法而加的 `BoundCall.RetIsErasedTypeParam` 撤了（成了死机制）。
2. **`constraint_member_tests` 那条护栏最终一字未改**：我一度把它改成断言 E0401，收窄后它
   **本来就是对的** ⇒ 还原。这反过来是收窄版「零存量破坏」的证据。
3. **C 只在同包生效**：TSIG 不导出方法级 `where`（导入符号 `HasDecl=false`）⇒ 所有方法级约束
   跨包都不校验，**包括本变更前就存在的「方法自己的型参」那种**（实测跨包
   `Array.Sort<Opaque>` 无诊断）。⚠️ 我最初把「方法自己的型参 → ✅ E0402」报成无条件成立，
   那是只造同包探针的结论 ⇒ **凡「是否被检查」的结论，探针必须同包 + 跨包各造一个。**

### 📌 文档门禁抓到我自己（值得记）

正文引用 `E0401` 却无实跑凭据 ⇒ **B11 判红**（那道门是上一条线我自己加的）。另有两条 B6
（```z42 块必须是单条 `{{#include}}` 指向 `examples/`，我图快内联了代码）。
**没有去 `learn-prose-diag-allow.txt` 给自己开豁免**（那等于拆门），而是建了两个活示例
`ctprop.z42` / `typo.z42`，实跑取输出写进 `run.console`，正文改走 include。

### 环境

- worktree `../wt-erasedblob`，分支 `fix-erased-blob-unbox`，已供种 + xtask 已重建
- ⚠️ `RUSTUP_TOOLCHAIN=1.98.1`；Rust workspace 在 `src/runtime/`
- 🔴 验修复一律 `./artifacts/.z42/z42 run`；`./.z42/z42` 是种子（拿它验 = 假阴性）
- ⚠️ 别在跑着 `xtask test` 的同时改源码
- ⚠️ 机器上常有 3–5 个会话并发构建，单次 `xtask test` 可能 10 分钟以上
- 探针在 `<scratchpad>/gaps/`：`c1.z42`/`c2.z42`（约束属性 vs 方法）、`t4b.z42`（擦除返回位）、
  `y.z42`/`z.z42`（裸 T 各成员）、`d_inferred.z42`/`d_explicit.z42`（IR 对照）
