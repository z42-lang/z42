# Tasks: `[Record]` attribute 替代 `record` 关键字

> 状态：🟡 DRAFT（待 6.5 gate） | 创建：2026-08-26
> **分两 nightly**：阶段 1-3 + 5（support）= nightly N；阶段 4（use + remove keyword）= nightly N+1。

## 进度概览
- [x] 阶段 1: `[Record]` directive 定义 + 识别接线（HandlerRegistry；**无需 RecordAttribute.z42**——directive 靠名字识别、无 backing 类）
- [x] 阶段 2: parser 接受 `(params)` → `ClassDecl.PrimaryParams` + `IsRecord` 字段（+ `;` 短形式修）
- [x] 阶段 3: `RecordExpand` AST pass + bit3 触发源迁移（加法式）
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

## 阶段 2: parser 接受 `(params)` + ClassDecl 新字段
- [ ] 2.1 `Decl.z42`：`ClassDecl` 加 `Param[] PrimaryParams` + `int PrimaryParamCount` + `bool IsRecord`
      （构造器默认空/false，沿用 `IsPartial` 的「构造后赋值」模式，不动现有 `new ClassDecl` 调用点）
- [x] 2.2 `DeclParser._parseTypeDecl`（:124）：`name`/typeParams 后、`:` base 前，若 `LParen` → 解析
      `_parseParamList` 存 `PrimaryParams`（**不展开**）
      - ⚠️ 实现时发现：`_parseTypeDecl` 原本无条件 `_expect("{")`，但短形式 `[Record] class Point(int X, int Y);`
        用分号无块体（镜像旧 `_parseRecord`）。修：`primaryPc>0 && 非 {` → `_expectSemi()` 早返回空成员
        ClassDecl。无位置参数仍要求 `{`（不放宽 `class Foo;`）。否则 semicolon 形式 parse 出错、拖垮整文件
        →「Demo.Main not found」（首轮 e2e 实测炸点）。
- [ ] 2.3 `Decl.z42:371` 注释更新（record 降级 → `[Record]` AST 展开）

## 阶段 3: RecordExpand AST pass + bit3 迁移
- [ ] 3.1 `src/compiler/z42c.semantics/src/RecordExpand.z42`（NEW，形态照抄 `BenchmarkDesugar.z42`）：
      CU 下降扫 `AttributedDecl`，命中 `[Record]`（`HandlerRegistry.HasRecord`）的 `ClassDecl`
- [ ] 3.2 展开（两分支共用代码，传 `vis`/`isRecord`）：`PrimaryParams` → `FieldDecl(vis)` + 合成主构造器
      `MethodDecl`（搬 `_parseRecord`:283-307）；插到 Members 前部
      - 有 `[Record]` → `vis="public"` + 置 `IsRecord=true`（= record）
      - 无 `[Record]` → `vis="private"` + `IsRecord=false`（= primary constructor，Decision 3=A）
- [ ] 3.3 golden 验证 primary ctor：`class Point(int X){int Sum(){return X;}}` 裸字段访问 + S6b 初始化器求值序
- [ ] 3.4 `HandlerRegistry.RunAst`（:45）串入：`AttributeSynth.Run(BenchmarkDesugar.Run(RecordExpand.Run(cu)))`
- [ ] 3.5 `ClassDescBuilder.z42`（**nightly N 加法式**，保 keyword `record` 不坏）：bit3 →
      `if (c.Kind=="record" || c.IsRecord)`（两触发并存）；`isStructOrRecord` **保留** record 分支
      （keyword 仍在，`[Record] class` 靠 Kind=="class" 自然拿基、无需改）。**N+1** 删关键字后再去
      `Kind=="record"` 两处。
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
