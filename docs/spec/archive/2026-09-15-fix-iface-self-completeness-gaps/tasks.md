# tasks：fix-iface-self-completeness-gaps

状态：🟢 已完成并归档（2026-09-15）

## A2 — impl 块接口成员齐备性
- 🟢 `_checkOneIfaceMembers`/`_checkOneIfaceMethod` 参数 `ClassDecl c` → `name`+`Span`（声明路径逐字不变）
- 🟢 新 pass `InheritanceResolver._passImplIfaceComplete`（迭代 ImplDecl、复用 `_checkOneIfaceMembers`）
- 🟢 3 处编排点接线（`SymbolCollector`：单文件 / 包级多-CU / deps，均在 `_passImpls`/`_passInheritFields` 后）
- 🟢 `_passImpls` 抬头自陈注释更新（不再是 gap）
- 🟢 单元门 ×3（缺成员→E0412 / 正例→"" / 错签名→E0412）

## A3-Func — `Self` 下钻 `Func<…>`
- 🟢 `MemberResolver._substSelf` 补 `Z42FuncType` 分支
- 🟢 `InheritanceResolver._substForIface` 补 `Z42FuncType` 分支
- 🟢 单元门：Func 参数满足性 ×2（正例→"" 真门 / Func 位写错→E0412）+ DumpBody Func 返回替换 ×1

## A3-索引器 — 接口索引器端到端
- 🟢 `MemberParser._parseIndexer` 区分有体/无体 accessor（`get;`/`set;`）
- 🟢 `MemberCollector._fillInterface` 补 `IndexerDecl` → `get_Item`/`set_Item` lowering
- 🟢 `ExprTyper._bindIndex` 补 `Z42InterfaceType` 收者分支（读，Self 经 `_substSelf` 替换）
- 🟢 `AssignTyper._bindAssign` 补 `Z42InterfaceType` 收者分支（写，set_Item）
- 🟢 DumpBody 门 ×2（get_Item 解析 / Self 返回替换）+ 运行期 e2e（interp+jit）

## 文档
- 🟢 `generics.md`：满足性节补 A2（impl 块）+ A3（Func<Self> 替换）
- 🟢 `member-accessors.md`：新增「接口索引器」小节（三处机制）
- 🟢 `generic-constraints.md`：Self 节补 Func<Self> 下钻 + 索引器交叉引用

## 验证
- 🟢 退回对照：4 条精确变红（A2 缺成员/错签名、`_substForIface`/`_substSelf` Func），无关门保持 PASS
- 🟢 完整 GREEN（interp）全绿 + 自举不动点 3/3 gen1==gen2（零字节漂移）
- 🟢 `test bootstrap` NO boundary violation
- 🟢 `test e2e --dir interfaces/cross-zpkg --mode jit` + `test stdlib --mode jit`
