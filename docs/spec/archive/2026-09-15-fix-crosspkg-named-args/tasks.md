# Tasks: fix-crosspkg-named-args

> 状态：🟢 已完成 | 创建：2026-09-15 | 完成：2026-09-15

**变更说明：** 命名实参对导入的函数 / 方法 / 构造器可用，并可用于带 `params` 尾参的方法；顺带补上导入自由函数丢失的默认值。
**原因：** ① 读包重建导出签名时形参名一律合成 `p{i}`，导入符号没有真实形参名，`_adaptArgs` / `OverloadResolver.Map` 又只认本地 `MethodDecl` ⇒ 跨包 `F(width: 3)` 报 `undefined: width`；② 唯一候选是 params 方法时不走实参映射、`_adaptArgs` 把「params 尾参没给」当缺实参 ⇒ 同包 `P(head: "h")` 也报 E0437 + undefined；③ 自由函数发射路径从不填 `ParamAttrs`（`$Default` 哨兵载体）⇒ 跨包 `Label("a")` 报 E1005。
**文档影响：** `docs/book/src/language/named-arguments.md`（跨包 / params 两节 + 机制表）、`docs/book/src/language/member-forwarding.md`（「参数名导入时丢失」一条作废）、`src/compiler/z42c.semantics/README.md`（新文件 `CallParams.z42`）。

## 实测（修复前，main 6d57a694f 的编译器）

| 形态 | 结果 |
|---|---|
| 跨包 `Label(pad: "*", text: "b", width: 3)` | `E0401: undefined: pad / text / width` |
| 跨包 `Label("a")`（自由函数，`width`/`pad` 有默认值） | `E1005: missing required argument #2` |
| 同包 `U.P(head: "h")` 对 `P(string head, params int[] xs)` | `E0437` + `no static method P` + `undefined: head` |
| 同包 `U.Q(a: 1)` 对 `Q(int a, string tail = "t", params string[] rest)` | 同上 |

## 任务

- [x] 1.1 `Z42FuncType.ParamNames`（默认空）；8 个签名搬运点（MethodTypeArgSubst ×4 / MemberResolver ×3 / InheritanceResolver ×1）随 `ParamCallers` 一起搬
- [x] 1.2 `TsigReconcile._params`：名取 SIGS `ParamNames[i+off]`，缺失回落 `p{i}`（读包时重建，零字节变化）
- [x] 1.3 `ImportedSymbolLoader._fillParamDefaults` → `_fillParamMeta`，同批填 `ParamNames`（4 个调用点）
- [x] 1.4 AST 导出器同口径：`ClassExtractor._fromSymbol`（`md.Params[i].Name`）/ `_fromImportedMethod`（沿用导入祖先的 `ParamNames`）/ `FuncImplExtractor` 两处
- [x] 1.5 新 `CallParams.z42`（`DeclOf` / `CanName` / `Count` / `IndexOf`）：按名字找形参的唯一出处
- [x] 1.6 `OverloadBinder._adaptArgs(MethodDecl…)` → `_adaptArgs(MethodSymbol…)`：本地按 Decl、导入按签名；导入缺位走 `_crossPkgDefault`；`params` 尾位未给补空 `BoundArrayLit`；`_withDefaults` 命名路径门改 `CallParams.CanName`
- [x] 1.7 `OverloadResolver.Map` 命名实参映射改走 `CallParams`；`_resolveOverload` 唯一 params 候选也走映射
- [x] 1.8 `ConstructTyper._bindCtorArgs`：导入构造器带命名实参时走 `_adaptArgs`（纯位置导入调用路径不变）
- [x] 1.9 `IrGenAuxEmitter`：自由函数填 `ParamAttrs`（与 `IrGenMemberEmitter` 同一条）
- [x] 1.10 测试：cross-zpkg `named_args_cross_pkg`（自由函数 / 静态 / 实例 / 构造器乱序 / 同 arity 重载按名选 / params 尾参省略与按名传 / 继承链上的导入方法 / 导入自由函数纯位置默认值）；golden `src/tests/named-args/named_args_params.z42`（同包 params × 命名实参，含构造器）。阴性对照：修复前编译器编两份源码分别报 `undefined: pad` 与 E0437
- [x] 1.11 文档同步：named-arguments.md / member-forwarding.md / z42c.semantics README
- [x] 2.1 `xtask test` 完整 GREEN（base main 6d57a694f，全 stage ✅ 6m20s）

## 备注

- **不支持（已写进 book）**：命名实参 + 展开的多个位置元素（`P(head: "h", 1, 2)`）——位置元素无空位可落，报「找不到方法」；需要展开就全用位置实参。
- **观察到未处理**：自由函数发射路径同样不填**方法级** `Attrs`（`irf.Attrs`），跨包自由函数上的 attribute（如 `[Deprecated]`）因此可能丢失。补它会改变带 attribute 的自由函数（含 `[Test]`）的 zbc 与发现路径，风险面与本 change 不同，未动。
- `OverloadBinder.z42` 855 行（软限 500 / 硬限 886），命名实参归位（`_adaptArgs` 一族）可作为后续独立 refactor 拆出。
