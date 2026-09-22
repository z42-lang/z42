# Tasks: 包级初始化回调 `[ModuleInit]`

> 勾选即完成。GREEN 标准见 [workflow.md 阶段 8](../../../agent/rules/workflow.md)。

## 0. 开工前核对（每项都要留证据）

- [ ] 0.1 `gh pr list` 查在飞 PR（合并前紧邻**再查一次**）
- [ ] 0.2 `grep -rn '"E0484"' src/` + `grep -rn '"E0485"' src/` 确认两个号未被占用；查 `DiagnosticCodes.z42` 登记表
- [ ] 0.3 确认 semantics / pipeline 哪一层能拿到包名（`req.PackageName` 的传递路径）
- [ ] 0.4 确认加载器注册类型表那一遍的位置（`lazy_loader/registry.rs`），作为 `$Module` 标记点
- [ ] 0.5 新 worktree + nightly SDK 冷种子（见 [[fresh-worktree-seed-setup]]）

## 1. 编译器：识别与校验

- [ ] 1.1 `[ModuleInit]` 并入 `HandlerRegistry.KindOf` 的 built-in directive 三路判定
      （🔴 不往硬编码魔法名白名单加名字）
- [ ] 1.2 校验标注目标：static + 无参 + 返回 void + 非泛型 + 非抽象；违反发 **E0484**
- [ ] 1.3 **包级**校验：一个包至多一个 `[ModuleInit]`，第二个发 **E0485**（报后出现处，
      消息带第一处 `file:line`）
- [ ] 1.4 `DiagnosticCodes.z42` 登记 E0484 + E0485 + `docs/reference/src/appendix/error-codes.md` 同步
      （🔴 两个码不得合并，见 [[diagnostic-code-uniqueness-program]]）
- [ ] 1.5 负例 fixture：E0484 五条（非 static / 有参 / 返回非 void / 泛型 / 标在类型上）+
      E0485 两条（跨文件、同文件）+ 合法对照两条（`private static void`、两个不同包各一个）

## 2. 编译器：合成 `<pkg>.$Module`

- [ ] 2.1 各 CU 收集本文件的 `[ModuleInit]` 方法（含源文件名 + 声明位置，用于定序）
- [ ] 2.2 包级汇总后**先校验至多一个**（E0485），再发射 `<pkg>.$Module` 类型 + 其类型
      初始化器（`call` 那一个方法，`ret void`），挂 `$Cctor` 哨兵（载荷 = 合成函数 FQ 名）
      —— 形状对标 `TIDX`
- [ ] 2.3 ⚠️ **增量编译**：只重编一个 CU 时，其它 CU 的 `[ModuleInit]` 信息要从增量缓存
      拿得到，否则 E0485 漏报（门见 4.4b）
- [ ] 2.4 零 `[ModuleInit]` ⇒ 不合成任何东西（codegen 门：`ir` 不含 `$Module`）

## 3. 运行期：触发

- [ ] 3.1 加载器注册类型表那一遍里顺带记下「本包有无 `$Module`」（`Option`/包，零额外查表）
- [ ] 3.2 T1–T4 四个既有收口点（锁已释放处）对 `newly_loaded` 逐包 `ensure_module_init`
      —— `try_lookup_function` / `try_lookup_type` / `load_module_into_vm` /
      `load_module_bytes_into_vm`
- [ ] 3.3 T5：主包在启动路径（`boot` / `app`）触发一次
- [ ] 3.4 失败语义复用 `CctorState::Failed`（不吞、不重试）
- [ ] 3.5 重入 / 跨线程：确认走既有 `claim`（同线程放行、跨线程等待），补单测

## 4. 门（必须会变红）

- [ ] 4.1 e2e `cross-zpkg`：场景 1（先于本包代码）
- [ ] 4.2 e2e：场景 2 **只调自由函数**也触发 —— 🔴 本变更的关键判别力，
      改回纯惰性方案时这条必须红
- [ ] 4.3 e2e：场景 3 至多一次（计数 == 1）
- [ ] 4.4 负例门：场景 4 包内第二个 `[ModuleInit]` ⇒ E0485（跨文件 + 同文件）
- [ ] 4.4b 负例门：场景 4b **增量**重编后仍报 E0485 —— 🔴 守「只重编一个 CU 就看不见
      别的 CU」这个漏报形状
- [ ] 4.4c e2e：场景 7 跨包各一个，输出顺序 == 实际加载顺序
- [ ] 4.5 e2e：场景 5 抛异常 ⇒ 包装异常，第二次触达仍抛
- [ ] 4.6 codegen 门：无 `[ModuleInit]` ⇒ 无 `$Module`（场景 6 上半）
- [ ] 4.7 **判别力验证**：给每道新门注入假断言确认会红，再还原
      （🔴 用 `cp` 备份还原，**不要** `git checkout <file>`——会抹掉自己的改动）
- [ ] 4.8 `xtask test diagcodes` 通过（E0484 / E0485 登记表 ↔ 文档双向相等）

## 5. 不动点 / 爆炸半径

- [ ] 5.1 全仓重编，与变更前 zpkg **逐字节对账**（无 `[ModuleInit]` ⇒ 零变化，场景 6 下半）
- [ ] 5.2 自举不动点 3/3（gen1 == gen2 byte-identical）
- [ ] 5.3 `xtask test stdlib --mode jit`（触发点改了派发/加载面，本地默认只跑 interp）

## 6. 文档（无文档 = 未完成）

- [ ] 6.1 `docs/reference/src/language/module-initializers.md` 新页（语义 + 「一个包一个」+ 跨包顺序 = 实际加载顺序）
- [ ] 6.2 `docs/internals/` 初始化机制页：加 `$Module` 一节（为什么不能在锁内同步跑、
      五个触发点、与惰性 cctor 的区别）
- [ ] 6.3 `docs/reference/src/appendix/error-codes.md`：E0484 + E0485
- [ ] 6.4 更新 `#418 惰性化前提已失效` 的记述（初始化器不再保证无副作用）
- [ ] 6.5 目录 README / mdBook SUMMARY 入口
- [ ] 6.6 按 doc-system「三道门」的门③ doc-check 清单逐项核对

## 7. 归档

- [ ] 7.1 PR（合并前并入 main 最新 + 重跑 GREEN）
- [ ] 7.2 合并后删分支 / worktree
- [ ] 7.3 `docs/spec/changes/add-module-init-hook/` → `docs/spec/archive/YYYY-MM-DD-...`
- [ ] 7.4 更新 memory：`add-module-init-hook-program.md` + MEMORY.md 索引
