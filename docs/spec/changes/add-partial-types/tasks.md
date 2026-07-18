# Tasks: partial 类型

> 状态：🔴 DRAFT，**设计封版**、排队中（等 `compiler` 锁释放）| 创建：2026-07-08 | 封版：2026-07-19
> Open Questions 三条全结（主碎片 D3 / partial method D5 / 能力版本号 D7）；开工无待决设计项。
> 前置：`add-indexed-zpkg-min-patch` 归档释放 `compiler` 锁 → 阶段 6.5 确认 → 开工。
> **v1 scope（2026-07-19 定，design D3/D8/D9）**：① 主碎片 = 路径 Ordinal 最小（单一规则）；
> ② indexed 方法体**默认散留各碎片 zbc**（不合并，零 VM/格式改动）；③ **只做顶层类型 partial**——
> partial 外层可含嵌套类，但**嵌套类自身 partial → 报错 + Deferred**（嵌套发射链路未接通）。

## 进度概览
- [ ] 阶段 1: 词法 + 语法（partial 修饰符）
- [ ] 阶段 2: 语义合并（SymbolCollector + Z42Type）
- [ ] 阶段 3: Codegen（合并 TYPE record + partial method 擦除）
- [ ] 阶段 4: 增量共存（IncrementalBuild 碎片组联动）
- [ ] 阶段 5: 跨包导出（ExportedTypeExtractor）
- [ ] 阶段 6: 测试与验证
- [ ] 阶段 7: 文档同步

## 阶段 1: 词法 + 语法
- [ ] 1.1 `TokenKind.z42` 新增 `Partial` 常量
- [ ] 1.2 `Lexer.z42` `_initKeywords()` 注册 `partial`
- [ ] 1.3 `Decl.z42` `ClassDecl` 加 `IsPartial`；`MethodDecl` 加 `IsPartial` + `HasBody`
- [ ] 1.4 `Parser.z42` 类型声明接受 `partial` 修饰符（class/struct/record/interface 同处）
- [ ] 1.5 `Parser.z42` 方法声明接受 `partial`（允许无 body：`partial R M(params);`）

## 阶段 2: 语义合并（按 pipeline 顺序）
- [ ] 2.1 `SymbolCollector._passClassStubs`：同名 `partial` 声明指向同一 `Z42ClassType`
- [ ] 2.2 `SymbolCollector._passMembers`：碎片成员按 relPath Ordinal 序 + 声明序并入（复用 `IncrementalBuild.Rel` 排序键）
- [ ] 2.3 合并校验：缺 `partial` 报错 / Kind 不一致报错 / 基类·主ctor 单碎片 / 接口并集 / 重复成员报错
- [ ] 2.4 partial method 声明↔实现配对（签名一致校验；至多一个实现）
- [ ] 2.5 `Z42Type.z42`：记录主碎片归属 + 有序备份按合并序追加（如需）

## 阶段 3: Codegen
- [ ] 3.1 `IrGen._classDesc`：成员序取自合并 `Z42ClassType`（非本地 ClassDecl）
- [ ] 3.2 `IrGen`：仅主碎片 CU 发出该类型 `TYPE` record，非主碎片跳过
- [ ] 3.3 partial method 擦除：无实现 → 不发 SIGS/FUNC；消解无返回值/无 out 调用点

## 阶段 4: 增量共存
- [ ] 4.1 `IncrementalBuild`：`IsPartial` 类型名的全部碎片文件互连成团（clique）
- [ ] 4.2 验证：touch 单碎片 → 整组 fresh，非 partial 文件不失效

## 阶段 5: 跨包导出
- [ ] 5.1 `ExportedTypeExtractor`：按合并后 `Z42ClassType` 导出一次完整 TSIG

## 阶段 6: 测试与验证
- [ ] 6.1 `z42c.syntax/tests/parser/partial/`：解析正常 + 缺 partial/Kind 冲突报错
- [ ] 6.2 `z42c.semantics/tests/collect/partial_merge/`：合并 + 冲突 + **顺序确定性**（交换发现序字节不变）
- [ ] 6.3 `src/tests/partial-types/`：跨文件 build+run golden（含 partial method 有实现/擦除）
- [ ] 6.4 `src/tests/partial-types/incremental/`：增量 dist == 全量 dist 逐字节 + mtime 断言
- [ ] 6.5 `examples/partial.z42`（或多文件目录）
- [ ] 6.6 `cargo build` (z42vm) —— 确认 VM 零改动仍编过
- [ ] 6.7 `xtask test`（e2e + cross-zpkg + stdlib + compiler 自举 7/7 byte-identical）
- [ ] 6.8 `xtask test bootstrap`：上一版 nightly 仍能编当前源（阶段 1 support-only，z42c/stdlib 未使用 partial）
- [ ] 6.9 spec scenarios 逐条覆盖确认

## 阶段 7: 文档同步（按阶段 9 触发矩阵）
- [ ] 7.1 `docs/book/src/language/partial-types.md` 新页（语法 + 合并语义 + 增量交互 + mermaid）
- [ ] 7.2 `docs/book/src/SUMMARY.md` 挂入
- [ ] 7.3 `docs/design/language/grammar.peg` 加 `partial` 修饰符产生式
- [ ] 7.4 `docs/roadmap.md` 0.4.4 partial 状态更新
- [ ] 7.5 `z42c.syntax/README.md` / `z42c.semantics/README.md` 六段同步
- [ ] 7.6 `docs/spec/changes/ACTIVE.md` 释放 compiler 锁

## 备注
- 自举分阶段：本 change 只落"support"，z42c/stdlib 源**不使用** partial；使用留待 nightly 发布后独立 follow-up。
- partial method（Decision 5）实施前最终确认；若砍掉，删阶段 1.5 / 2.4 / 3.3 相关子任务。
- 若增量单碎片粒度（body-only 不牵连兄弟）未来需要 → Deferred（合成 type-metadata 模块方案）。
