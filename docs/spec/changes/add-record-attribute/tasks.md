# Tasks: `[Record]` attribute 替代 `record` 关键字

> 状态：🟡 DRAFT（待 6.5 gate） | 创建：2026-08-26
> **分两 nightly**：阶段 1-3 + 5（support）= nightly N；阶段 4（use + remove keyword）= nightly N+1。

> **🔴 重大重构（PR #295 首推 CI 炸后）**：原「ClassDecl 加 PrimaryParams/IsRecord 字段 + RecordExpand
> AST pass」方案**违反自举约束**——z42c.semantics 对着上一 nightly 的 z42c.syntax 编译，semantics 读新
> syntax 字段 = `E0401 no field`。**改为 parser 就地展开**（attrs 传进 `_parseTypeDecl`）+ bit3 由 IrGen 从
> 既有 `AttributedDecl.Attrs` 判 → semantics 零新 syntax 字段。见 design Decision 2。

## 进度概览
- [x] 阶段 1: `[Record]` directive 识别（HandlerRegistry `IsRecordDirective`/`HasRecord`；**无需 RecordAttribute.z42**）
- [x] 阶段 2: parser `_parseTypeDecl(mods,kind,attrs)` 接受 `(params)` **就地展开**（public if [Record] else private）+ `;` 短形式；Parser/MemberParser 传 attrs
- [x] 阶段 3: bit3 由 `IrGen` 从原始 AttributedDecl 判 `HasRecord` 传 `ClassDescBuilder._classDesc(c,hasRecord)`（**无 ClassDecl 新字段、无 RecordExpand pass**）
- [x] 阶段 5: 文档（book 机制页 + SUMMARY + language-overview）+ e2e golden（record_attribute）
- [~] 阶段 6: 验证（z42c 自建✔ / stdlib 24/24✔ / record_attribute e2e 2/2✔；self-host 字节 + bootstrap 边界交 CI，本机 seed 老一 nightly 已手动 patch）
- [ ] 阶段 4: 【nightly N+1】迁移 stdlib/examples/tests + 删 `record` 关键字

## 阶段 1: `[Record]` directive 定义 + 识别接线
- [x] 1.1 ~~RecordAttribute.z42~~ **不需要**——directive 靠名字识别、无 backing 类（AttributeSynth 只为
      store-meta 合成工厂；实测 stdlib 无 Deprecated/Suppress/Native 类）
- [x] 1.2 `HandlerRegistry.z42`：加 `IsRecordDirective(name)`（`name=="Record"`）+ `HasRecord(rawMem)`
      （逐字节仿 `IsDeprecatedDirective`/`HasDeprecated`）
- [x] 1.3 `HandlerRegistry.IsDirectiveAttr` 加 `|| IsRecordDirective(name)`；directive 天然豁免 D8 后缀
- [x] 1.4 `[Record]` 在 class/struct 上解析成功（e2e record_attribute 覆盖）

## 阶段 2: parser 就地展开（无 ClassDecl 新字段）
- [x] 2.1 **不加 ClassDecl 字段**（自举约束：semantics 读新 syntax 字段 = `E0401`）——位置参数就地消费。
- [x] 2.2 `DeclParser._parseTypeDecl(mods, kind, attrs)`：解析 `(params)` → 就地展开成 `FieldDecl(vis)` +
      主构造器（`_attrsHaveRecord(attrs)` 判 vis=public/private）；`_attrsHaveRecord` helper（名字匹配 Record）。
      - Parser.z42 顶层 + MemberParser 嵌套两处调用传 `attrs`。
      - ⚠️ `;` 短形式：`primaryPc>0 && 非 {` → `_expectSemi()`（否则拖垮整文件→「Demo.Main not found」）。

## 阶段 3: bit3 由原始 AttributedDecl 判（无 RecordExpand pass）
- [x] 3.1 bit3：`IrGen:182` `_classDesc(descDecl, HandlerRegistry.HasRecord(cu.Decls[i]))`；`ClassDescBuilder._classDesc(c, hasRecord)`
      bit3 = `Kind=="record" || hasRecord`。`HasRecord` 读原始 `AttributedDecl.Attrs`（既有结构，semantics 零新字段）。
- [x] 3.2 primary ctor 裸字段访问 + `[Record]` public / 无-attr private（e2e record_attribute 覆盖 S1-S6）。
- [ ] 3.3 【N+1】`ClassDescBuilder`：bit3 去 `Kind=="record"` 项；`isStructOrRecord` 去 record 分支（基类随 Kind）。
      > ⚠️ 关键：N 阶段 `[Record]` 与 keyword `record` **并存**，ClassDescBuilder 改动必须加法式——
      > 直接替换会让现存 `record Foo(...)`（stdlib z42.build 等）在 N 丢 bit3 / 改基类。

## 阶段 5: 文档 + 测试（support 阶段绿）
- [ ] 5.1 单测：parser（S1-S4）、RecordExpand（展开正确 + IsRecord + S6 诊断）
- [ ] 5.2 golden：`[Record] class`/`[Record] struct` 端到端 + `__type_is_record` 反射（S1/S2）+ 等价性（S8）
- [ ] 5.3 `docs/book/src/language/record-attribute.md` NEW（机制页）+ `SUMMARY.md` 挂入
- [ ] 5.4 `docs/design/language/language-overview.md` record 段重写；`grammar.peg` record 产生式 →
      class/struct 可选 `(params)`；`docs/book/src/compiler/zbc-format.md` bit3 说明更新
- [ ] 5.5 目录 README（z42.semantics 加 RecordExpand / z42.core 加 RecordAttribute）

## 阶段 6: 验证（support）
- [ ] 6.1 `cargo build`（z42vm，实际无 runtime 改动，确认）
- [ ] 6.2 `xtask test compiler` —— 自举 5/5 **byte-identical**（z42c 源不用 `[Record]`）
- [ ] 6.3 `xtask test e2e` + `--dir cross-zpkg` —— 全绿（含新 golden）
- [ ] 6.4 `xtask test stdlib` + `vscode-syntax` —— 全绿
- [ ] 6.5 `xtask test bootstrap` —— 上一 nightly z42c 能编当前源（无越界）
- [ ] 6.6 spec S1-S6/S8 逐条覆盖确认

## 阶段 4: 【nightly N+1，新 nightly 发布后】use + remove
- [ ] 4.1 迁移 stdlib：`z42.build/Models.z42`（5 处）+ `ICompiler.z42`（2 处）`record X(...)` → `[Record] class X(...)`
- [ ] 4.2 迁移 examples：`patterns.z42`（Expr/Num/Add/Mul/Neg/Shape2/Circle2/Rect2）+ `oop.z42`（Point）
- [ ] 4.3 迁移 tests：`decl_tests.z42`（`test_record`）+ `z42.core/tests/reflection.z42`（`__type_is_record`）
- [ ] 4.4 删关键字：`TokenKind.z42`（`Record=23`）+ `Lexer.z42`（`_kw("record")`）+ `DeclParser._parseRecord`
      + `Parser.z42:341`/`MemberParser.z42:115` 分派
- [ ] 4.5 删 ~22 处 `|| Kind=="record"` 子句（MemberCollector:21 / StubCollector:109,126 / TypeChecker:290 /
      InheritanceResolver:55,102,189 / DeclBinder:22,31 / ExportedTypeExtractor:101,117,135 / IrGen:166 /
      IrDump:128 / ConstraintChecker:29 / CuCompile:26 / CuPreprocess:51 / TestIndexBuilder:52 /
      ClassDescBuilder:110 已在 3.5 处理）
- [ ] 4.6 `DeclEnforcer.z42` E0431 文案 + `Decl.z42`/`Parser.z42` 各注释去 "record"
- [ ] 4.7 加负测试：`record X` → "expected declaration"
- [ ] 4.8 重跑阶段 6 全部 gate（含 self-host 字节不变）

## 备注
- **格式中立**：无 zbc/zpkg bump（bit3 复用，恒等语义）→ warm 本地可验，无两代自举墙。
- **自举安全**：support（阶段 1-3）落地时 z42c/stdlib **不使用** `[Record]`；use（阶段 4）等新 nightly。
- **唯一待 gate 确认项**：Decision 3（S6，无 `[Record]` 的 `(params)` = 诊断 B / primary ctor A / 脱糖 C）。
