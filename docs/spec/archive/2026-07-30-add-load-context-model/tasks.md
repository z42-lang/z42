# Tasks: 加载上下文模型（LoadContext / ALC 地基）

> 状态：🟢 已完成 | 创建：2026-07-30 | 完成：2026-07-30 | User 6.5 已确认
> 变更类型：`vm`（完整流程）| worktree 预抢：`worktree-load-context`（off origin/main）| 占用：runtime + stdlib

## 进度概览
- [x] 阶段 1: 运行时上下文模型（Rust）
- [x] 阶段 2: builtins + Value 句柄
- [x] 阶段 3: stdlib z42 类
- [x] 阶段 4: 测试
- [ ] 阶段 5: 验证 + 文档同步

## 阶段 1: 运行时上下文模型（Rust）
- [ ] 1.1 `metadata/context.rs`（NEW）：`ContextId` / `LoadContext`（name, is_collectible, arena）/ `ContextRegistry`（root=ContextId(0) + collectible 表 + create_collectible）
- [ ] 1.2 `metadata/mod.rs`：`pub mod context;`
- [ ] 1.3 `vm_context.rs`：`VmCore` 挂 `context_registry`；VM 初始化建 root（ContextId(0)）
- [ ] 1.4 `metadata/types.rs`：`TypeDesc` 加 `context: ContextId`（现有构造路径一律填 root）+ 所属 assembly 标识
- [ ] 1.5 `metadata/loader.rs`：`load_into_context(ctx, path)` —— 解析 zpkg + 建 Module/TypeDesc(context=ctx) 存入 ctx arena，不 merge 进 root；root 入口路径保持不变

## 阶段 2: builtins + Value 句柄
- [ ] 2.1 `metadata/types.rs`：`NativeData` 加 `LoadContextHandle` + `AssemblyHandle` 变体
- [ ] 2.2 `corelib/loadcontext.rs`（NEW）：`__lctx_default / __lctx_create_collectible / __lctx_load / __lctx_name / __lctx_is_collectible / __lctx_assemblies / __lctx_unload`
- [ ] 2.3 `corelib/loadcontext.rs`：`__asm_name / __asm_is_collectible / __asm_loadcontext / __asm_get_types`
- [ ] 2.4 `corelib/loadcontext.rs`：`__type_is_collectible / __type_assembly`
- [ ] 2.5 `corelib/mod.rs`：`pub mod loadcontext;` + BUILTINS 表追加注册（表尾，勿插中间）

## 阶段 3: stdlib z42 类
- [ ] 3.1 `Runtime/LoadContext.z42`（NEW）：`Std.Runtime.LoadContext`（Default / Name / IsCollectible / CreateCollectible / Load / GetAssemblies / Unload）
- [ ] 3.2 `Reflection/Assembly.z42`（NEW）：`Std.Reflection.Assembly`（Name / IsCollectible / LoadContext / GetTypes）
- [ ] 3.3 `Type.z42`：加 `IsCollectible` + `Assembly` 两个 extern 属性
- [ ] 3.4 确认 `Unload()` 抛 `Std.NotSupportedException`（若无此异常类型，改用既有等价并在 spec/design 注明）

## 阶段 4: 测试
- [ ] 4.1 `corelib/loadcontext_tests.rs`（NEW）：注册表 / root vs collectible / IsCollectible / Unload 抛异常
- [ ] 4.2 `src/tests/load-context/collectible-reflection/`（NEW）：dep zpkg 源 + source.z42 + expected_output.txt（建 collectible + Load + 反射断言 + Unload catch）
- [ ] 4.3 spec scenarios 逐条覆盖确认（9 个场景）

## 阶段 5: 验证 + 文档同步
- [ ] 5.1 `cargo build --release`（z42vm）无错
- [ ] 5.2 `xtask test`（完整 GREEN gate：e2e + cross-zpkg + stdlib + compiler + vscode-syntax）全绿
- [ ] 5.3 root 兼容回归确认（自举 gen1==gen2 + 全量 e2e/stdlib 不变）
- [x] 5.4 目录 README 同步：`corelib` / `metadata` / `Reflection` **均无 README**（第 4 层 / sibling 无先例）→ 无需改（Scope 校正）
- [x] 5.5 book 机制页 `docs/book/src/runtime/load-context.md`（NEW）+ 挂入 `SUMMARY.md`
- [x] 5.6 `docs/design/runtime/load-context.md` 页头对齐：Phase 1 地基已落地 + 决策修订（强制清理轴/粒度可调）
- [x] 5.7 归档：mv 到 archive + ACTIVE.md 登记 + PR

## 验证报告（GREEN）
- ✅ cargo build (release z42vm) 无错
- ✅ Rust 单测 `loadcontext_tests`：6/6
- ✅ e2e interp：215 passed / 0 failed（含 `load_context`）；`load_context` interp+jit 手动直跑输出与 expected 8 行全对
- ✅ e2e cross-zpkg：全过
- ✅ compiler 自举：**5/5 packages gen1==gen2 逐字节复现**（Type.z42 加 `__asmId` 不破不动点）+ z42c [Test] units 全过
- ✅ vscode-syntax：grammar ↔ Lexer 一致
- ⚠️ stdlib [Test]：278/279 —— 唯一失败 `z42.crypto.test.poly1305_vectors.zbc` 产物竞态读不到 = **已知 crypto heisenbug**（在办 change `fix-crypto-test-artifact-heisenbug`），**与本 change 无关**（不碰 z42.crypto；源手动编译 exit 0）。记 backlog，不阻塞本无关 change。

## 备注
- **Unload 语义**：Phase 1 声明抛 NotSupportedException（FC2=(ii)）；root 卸载语义上更是 InvalidOperation，实现取 NotSupported 并在 message 区分。
- **跨 context 执行不在本 change**：collectible zpkg 只保证反射可见，函数跨界调用是下一步。
- **锁**：worktree 预抢 runtime+stdlib（User 授权）；stdlib 名义持有者 `converge-z42c-onto-z42-project`，隔离 worktree + PR 避免直接争用，合并前 rebase。
- **实施期决策精化（写入 design.md）**：D5 关联放 ContextRegistry + Type `__asmId` 槽（不 mutate TypeDesc——它非 `Clone`）；D7 静态成员用 extern 方法（stdlib 无静态属性先例）→ API `Default()` 是方法。
- **发现的 pre-existing 问题（不在本 Scope，未修）**：regen 重生 `src/tests/zbc-format/{cross-import-token,with-tidx}/source.zbc` 时字节漂移（+45/+114B）。这两 fixture 是自包含小程序，import token 只依赖本 fixture 的 import 表，**与本 change 无关**（追加 builtins/类不影响它们）→ committed 基线相对当前 z42c 已 stale。已 `git checkout` 恢复、**不纳入本提交**；建议 main 单独 regen 修。
- **验证教训（重要）**：worktree 冷启动用 `Z42_HOME=<0.3.0 种子>` seed 时，golden 编译的 `stdlibDist` 会解析到**种子 stale libs**（`xtask_test_assets.z42:171` `_toolchainLibs(tc)`），其 z42.core 无 LoadContext → 静态调用宽松 fallback 成 `vcall null` → 运行期 VCall/FieldGet Null 假象。**去掉 Z42_HOME**（用 warm in-tree + `_libsDir(root)` 新 libs）后 interp+jit 输出全对。**结论：本 change 代码正确，e2e 失败纯属旧种子 libs 编译，非 bug。** 手动验证：`Z42_LIBS=<新libs> z42vm z42c.driver.zpkg --mode interp -- --emit-zbc <src> <out>` 再跑。
