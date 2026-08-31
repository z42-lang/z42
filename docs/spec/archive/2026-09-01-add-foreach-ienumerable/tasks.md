# Tasks: foreach 端到端支持 IEnumerator 协议 + List/Dict 实现 IEnumerable

> 状态：🔵 DRAFT 待审批 | 创建：2026-08-31 | 类型：lang / vm（需规范先行）

> ✅ 原 5 个 Open Question 已由 User 敲定并回填 design/proposal：D5=A（Std.Collections 独立文件）、
> D6=B（槽游标零分配）、D7=B（独立 `BoundForeachEnumerable` 节点）、D8=pattern-based 无装箱、
> D9=预期不触发自举纪律（`xtask test bootstrap` 复核）。
>
> **自包含、无前置依赖**：enumerator 是 `var __e = coll.GetEnumerator()` 得到的**标量** struct（非数组），
> 跨包实例化正常工作，不依赖任何前置 layout 修复。

## 进度概览
- [x] 阶段 0: DRAFT 审批（4 条锁定决策 + 5 个 Open Question 已裁决）
- [ ] 阶段 1: Binder（新增 `BoundForeachEnumerable` 节点 + StmtBinder IEnumerable 检测，D7/D8）
- [ ] 阶段 2: Emitter（新增 `_emitForeachEnumerable`：GetEnumerator + try/finally + MoveNext/Current/Dispose）
- [ ] 阶段 3: stdlib enumerator（List/Dict 实现 IEnumerable + enumerator `[Record] struct`）
- [ ] 阶段 4: 测试体系（golden e2e + stdlib 单测 + examples）
- [ ] 阶段 5: 文档同步 + GREEN + 归档

## 阶段 0: DRAFT 审批（已完成）
- [x] 0.1 User 确认识别优先级（数组→索引鸭子→IEnumerable，Decision 1）
- [x] 0.2 User 裁决 Decision 5 / 6 / 7 / 8（见顶部）
- [x] 0.3 复核 Decision 9（无格式 bump / 无两-nightly）——结论维持

## 阶段 1: Binder（`src/compiler/z42c.semantics/src/`）
- [ ] 1.1 `BoundStmt.z42`：新增独立 `BoundForeachEnumerable : BoundStmt` 节点（`VarName`/`VarType`/`Collection`/`Body`/`EnumeratorType`=具体 struct 类型）；现 `BoundForeach` 不动（Decision 7）
- [ ] 1.2 `StmtBinder.z42` `_bindForeach`：在数组/索引鸭子判定之后，新增"目标实现 `IEnumerable<T>`（含基类/接口链）"检测分支（优先级最后）
- [ ] 1.3 `StmtBinder.z42`：pattern-based 解析 `GetEnumerator()` 的**具体返回类型**（Decision 8）作 `EnumeratorType`；elem type 经该 enumerator `get_Current` 返回类型 / `IEnumerable<T>` 实参推断（`var` 与显式标注两路）
- [ ] 1.4 验证：仅实现 IEnumerable 的类不再落 unsupported；List/数组仍走原 path

## 阶段 2: Emitter（`src/compiler/z42c.semantics/src/StmtEmitter.z42`）
- [ ] 2.1 `_emitStmt` 新增 `BoundForeachEnumerable` 分派分支；现 `BoundForeach` 分支 + `_emitForeach` 不动
- [ ] 2.2 新增 `_emitForeachEnumerable`：`VCall/Call GetEnumerator` 取 `__e`（按具体 struct 类型分配，无装箱，Decision 8）
- [ ] 2.3 组装 while(MoveNext){ get_Current; body } + finally{ Dispose }（倾向：构造合成 `BoundTry` 复用 `_emitTry`，见 design Implementation Notes）
- [ ] 2.4 循环块 `PushLoop`/`PopLoop` + `_writeBack(fe.VarName, current)`，保证 break/continue 经 finally
- [ ] 2.5 验证 struct enumerator 局部（无装箱）`MoveNext` 原地改 `_index`/`_slot` 且被后续调用看到（design 标注的头号坑：struct 可寻址 lvalue）

## 阶段 3: stdlib enumerator（`src/libraries/z42.core/src/Collections/`）
- [ ] 3.1 `ListEnumerator.z42`（NEW，`Std.Collections`，Decision 5）：`[Record] struct ListEnumerator<T>`，字段 `_list`/`_index`（初值 -1），`MoveNext`/`Current`/`Dispose`
- [ ] 3.2 `List.z42`：`: IEnumerable<T>` + `GetEnumerator()` 返回**具体** `ListEnumerator<T>`（Decision 8）
- [ ] 3.3 `DictionaryEnumerator.z42`（NEW，`Std.Collections`，Decision 5/6）：`[Record] struct DictionaryEnumerator<K,V>`，持 Dict 引用 + 槽游标 `_slot`，`MoveNext` 扫 `occupied[]` 跳空槽，`Current` 组装 `KeyValuePair<K,V>`，零分配
- [ ] 3.4 `Dictionary.z42`：`: IEnumerable<KeyValuePair<...>>` + `GetEnumerator()` 返回具体 `DictionaryEnumerator<..>` + 为内部槽加 internal 访问器（occupied/keys/values/capacity，供 enumerator 读，Decision 6）
- [ ] 3.5 README / 类型注释同步（若 Collections 有目录 README）

## 阶段 4: 测试体系
- [ ] 4.1 `src/runtime/tests/golden/run/foreach_ienumerable/`（NEW）：自定义 iterable 正确迭代
- [ ] 4.2 golden：Dispose 正常/异常/break/return 均调用（Dispose 内置标记断言）
- [ ] 4.3 golden：空 IEnumerable 不进循环体但调 Dispose
- [ ] 4.4 golden：List foreach 不调 GetEnumerator（GetEnumerator 内埋"不应被调"标记）
- [ ] 4.5 `src/libraries/z42.core/tests/list_enumerator.z42`（NEW）：List/Dict GetEnumerator + enumerator struct 单测
- [ ] 4.6 `examples/foreach_ienumerable.z42` + `.z42.toml`（NEW）

## 阶段 5: 文档同步 + GREEN + 归档
- [ ] 5.1 `docs/book/src/compiler/source-compile.md`：新增 "foreach 三-path 下沉" 机制小节
- [ ] 5.2 `docs/book/src/language/foreach.md`（NEW）+ `docs/book/src/SUMMARY.md` 挂目录项
- [ ] 5.3 `docs/roadmap.md` / 相关标记（若适用）
- [ ] 5.4 GREEN：`rm -rf /tmp/z42c-e2e-*` → `xtask test` 全绿 + `xtask test bootstrap` 无越界（Decision 9）
- [ ] 5.5 归档到 `docs/spec/archive/<date>-add-foreach-ienumerable/` + PR（`parallel-development.md`）
