# Tasks: PropertyInfo.GetValue / SetValue

> 状态：🟢 已完成 | 创建：2026-07-15 | 完成：2026-07-16
> 子系统锁：**runtime**（空闲）+ **stdlib**（现由 DRAFT converge 持有，footprint 零重叠——待 User 授权）

## 进度概览
- [x] 阶段 1: runtime builtin（灌槽 + 2 builtin + helper）
- [x] 阶段 2: stdlib（PropertyInfo 字段 + 方法）
- [ ] 阶段 3: 测试 + 文档 + 验证

## 阶段 1: runtime（corelib/reflection.rs + mod.rs）
- [x] 1.1 `PropAccum` 加 `getter_qualified` / `setter_qualified`；`accumulate_property` 写入
- [x] 1.2 `builtin_type_properties` alloc_named 追加 `__getterQualified` / `__setterQualified` 槽
- [x] 1.3 抽共享 helper `invoke_qualified(ctx, qualified, call_args)`（从 `builtin_method_invoke` 提取执行 + 异常传播），`builtin_method_invoke` 改调它
- [x] 1.4 新增 `builtin_property_get_value`（无 getter → bail Std.Exception 语义）
- [x] 1.5 新增 `builtin_property_set_value`（无 setter → bail）
- [x] 1.6 `corelib/mod.rs` 注册 `__property_get_value` / `__property_set_value`

## 阶段 2: stdlib（PropertyInfo.z42）
- [x] 2.1 加隐藏字段 `string __getterQualified` / `string __setterQualified`
- [x] 2.2 加 `[Native("__property_get_value")] public extern object GetValue(object obj);`
- [x] 2.3 加 `[Native("__property_set_value")] public extern void SetValue(object obj, object value);`
- [x] 2.4 订正头注释（删「需 0.5.x Invoke」过期语句）

## 阶段 3: 测试 + 文档 + 验证
- [x] 3.1 [Test] 加入 `reflection.z42`（roundtrip / 只读 SetValue 抛 / 继承属性读写；复用 PropHolder/PropChild）
- [x] 3.2 spec scenarios 逐条覆盖确认（roundtrip / 只读抛 / 继承 3 [Test] PASS）
- [x] 3.3 `cargo build` (z42vm) 无错（release，z42 v0.4.0）
- [x] 3.4 完整 `./xtask test` 门禁 **GREEN**（隔离验证，XTASK_EXIT=0，0 fail）：e2e 全绿 + stdlib 34/34 reflection + z42c 自举不动点 7/7 byte-identical + z42c [Test] 20/20 + vscode-syntax。**注**：首轮 209 fail 系环境问题——golden regen 用 *debug* z42vm，而门禁仅重建 *release* vm，stale debug vm 不识新 builtin `__property_get_value` → panic；`cargo build`（debug）后全绿。**门禁潜在缺口**：`xtask test` 不重建 debug vm，任何加 VM builtin 的变更都会误报，值得单独修（记 tasks 备注）。
- [x] 3.5 `reflection.md` PropertyInfo 段更新 + `reflection-future-properties` GetValue/SetValue 标落地 + 订正过期「依赖 0.5.x Invoke」
- [x] 3.6 README 同步：Reflection/ 为 4 层无 README；z42.core README 不列方法级反射 API → 无需同步
- [x] 3.7 归档到 archive/2026-07-16-add-property-getvalue-setvalue（ACTIVE.md 未登记锁 → 无需释放，见备注）

## 备注
- 无 zbc/zpkg 格式 bump（属性运行期派生，隐藏槽不持久化）→ 无 fixture golden 变更。
- 零编译器改动 → byte-identical gate 天然稳（实测 7/7 不动点），无需双侧镜像。
- **门禁缺口（新发现）**：`./xtask test` 只 `cargo build --release`，但 golden regen 用 `_activeVm(root,"debug")`；加 VM builtin 时 stale debug vm 会 panic「unknown builtin」→ 209 假失败。缓解=改动 VM builtin 后先 `cargo build`（debug）。根治=让门禁 build-runtime 同建 debug，或 regen 回退 release——建议单独开 fix change。
- **ACTIVE.md 锁**：因并发进程持续 reset/编辑 ACTIVE.md（本会话多次观测），未在其登记 stdlib+runtime 持有；本变更在独立分支 `feat/property-getvalue-setvalue` 实施、隔离验证 GREEN，合并时按常规解冲突。
