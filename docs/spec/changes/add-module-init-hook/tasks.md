# Tasks: 包级初始化回调 `[ModuleInit]`

> 勾选即完成。GREEN 标准见 [workflow.md 阶段 8](../../../agent/rules/workflow.md)。

## 0. 开工前核对（每项都要留证据）

- [x] 0.1 `gh pr list` 查在飞 PR（合并前紧邻**再查一次**）
- [x] 0.2 `grep -rn '"E0486"' src/` + `grep -rn '"E0485"' src/` 确认两个号未被占用；查 `DiagnosticCodes.z42` 登记表
- [x] 0.3 确认 semantics / pipeline 哪一层能拿到包名（`req.PackageName` 的传递路径）
- [x] 0.4 确认加载器注册类型表那一遍的位置（`lazy_loader/registry.rs`），作为 `$Module` 标记点
- [x] 0.5 新 worktree + nightly SDK 冷种子（见 [[fresh-worktree-seed-setup]]）

## 1. 编译器：识别与校验

- [x] 1.1 `[ModuleInit]` 并入 `HandlerRegistry.KindOf` 的 built-in directive 三路判定
      （🔴 不往硬编码魔法名白名单加名字）
- [x] 1.2 校验标注目标：static + 无参 + 返回 void + 非泛型 + 非抽象；违反发 **E0486**
- [x] 1.3 **包级**校验：一个包至多一个 `[ModuleInit]`，第二个发 **E0485**（报后出现处，
      消息带第一处 `file:line`）
- [x] 1.4 `DiagnosticCodes.z42` 登记 E0486 + E0485 + `docs/reference/src/appendix/error-codes.md` 同步
      （🔴 两个码不得合并，见 [[diagnostic-code-uniqueness-program]]）
- [x] 1.5 负例 fixture：E0486 五条（非 static / 有参 / 返回非 void / 泛型 / 标在类型上）+
      E0485 两条（跨文件、同文件）+ 合法对照两条（`private static void`、两个不同包各一个）

## 2. 编译器：合成 `<pkg>.$Module`

- [x] 2.1 各 CU 收集本文件的 `[ModuleInit]` 方法（含源文件名 + 声明位置，用于定序）
- [x] 2.2 包级汇总后**先校验至多一个**（E0485），再发射 `<pkg>.$Module` 类型 + 其类型
      初始化器（`call` 那一个方法，`ret void`），挂 `$Cctor` 哨兵（载荷 = 合成函数 FQ 名）
      —— 形状对标 `TIDX`
- [x] 2.3 ⚠️ **增量编译漏报 —— 查实为不存在**：判定挂在 `SymbolCollector.CollectAll`，
      而增量编译下「符号收集 / allFuncs / classNs / classMap / TSIG」**恒全包重算**
      （`IrDump.BuildPackageCus` 头注），只有 typecheck/codegen 走缓存 ⇒ E0485 天然不会漏报
- [x] 2.4 零 `[ModuleInit]` ⇒ 不合成任何东西（codegen 门：`ir` 不含 `$Module`）

## 3. 运行期：触发

- [x] 3.1 加载器注册类型表那一遍里顺带记下「本包有无 `$Module`」（`Option`/包，零额外查表）
- [x] 3.2 T1–T4 四个既有收口点（锁已释放处）对 `newly_loaded` 逐包 `ensure_module_init`
      —— `try_lookup_function` / `try_lookup_type` / `load_module_into_vm` /
      `load_module_bytes_into_vm`
- [x] 3.3 ~~T5 启动路径新挂点~~ —— 不需要：主包类型经 `seed_lazy_loader_types` 同样流过
      `insert_type` ⇒ 自动登记，`Main` 里第一个屏障点跑掉它
- [x] 3.4 失败语义复用 `CctorState::Failed`（不吞、不重试）
- [x] 3.5 重入 / 跨线程走既有 `claim`（同线程放行、跨线程等待）。**没有重复造测试**：
      失败终态语义已由 `failed_type_reports_error_on_every_later_access` 覆盖（先查覆盖再决定加不加）；
      只补了本变更真正新增的判定 `module_pseudo_type_is_recognised_by_suffix`

## 4. 门（必须会变红）

- [x] 4.1 e2e `cross-zpkg`：场景 1（先于本包代码）
- [x] 4.2 e2e：场景 2 **只调自由函数**也触发 —— 🔴 本变更的关键判别力，
      改回纯惰性方案时这条必须红
- [x] 4.3 e2e：场景 3 至多一次（计数 == 1）
- [x] 4.4 负例门：场景 4 包内第二个 `[ModuleInit]` ⇒ E0485（跨文件 + 同文件）
- [x] 4.4b ~~增量重编仍报 E0485~~ —— 见 2.3：该漏报形状在当前架构下不存在（CollectAll 恒全包重算），
      不为一个不可能的形状造门
- [x] 4.4c e2e `module_init_load_order`：两个互不依赖的包各一个初始化器，**先触达的先跑**
      （清单里 A 在 B 前，实际 B 先跑）。⚠️ 两个 init 会挨着跑完 —— 首次跨包解析把依赖闭包
      一次拉进来，同一屏障点看到两个已登记的 `$Module`；关键断言是它们的**相对顺序**
- [x] 4.5 ~~e2e：场景 5 抛异常可 catch~~ —— **查实为不成立**：异常正确抛出但用户
      `catch` 抓不到（干净 A/B 排除了调用形态 / catch 类型 / interp·jit / 屏障位置四项，
      对照组全部可捕获）。根因未查清 ⇒ 不为不成立的行为留一条长期红着的门；
      差距登记在 design.md「已知差距」+ spec 场景 5
- [x] 4.6 codegen 门：无 `[ModuleInit]` ⇒ 无 `$Module`（由 `ModuleInitSynth.Emit` 的 early-return 保证；
      全仓重编字节对账见 5.1）
- [x] 4.7 **判别力验证**（`cp` 备份还原，没用 `git checkout`）：注释掉 `ModuleInitSynth.Emit`
      → `module_init_free_function` / `module_init_once` **立刻变红**，两条负例门（纯诊断）
      不受影响 —— 正例守合成+触发、负例守校验，分工正确。还原后复绿。
      E0486/E0485 单测与 `runs_before_main` 另有天然证据：它们在对应钩子挂上之前就是红的
- [x] 4.8 `xtask test diagcodes` 通过：126 个码常量、592 个源文件、0 violation

## 4.9 新增：主包路径（实现中发现）

- [x] 4.9 golden `src/tests/module-init/runs_before_main`：主包自己的 `[ModuleInit]` 在
      `Main` 第一行之前跑完 —— 守 `seed_types_for_lookup` 绕过 `insert_type` 漏斗那个洞
      （发现时主包的初始化器**静默从不执行**）

## 5. 不动点 / 爆炸半径

- [x] 5.1 零变化的证据链（`xtask test fingerprint` 需要一棵 **base 源码树**，本地无从提供）：
      ① `ModuleInitSynth.Emit` 对无站点的 CU **early-return，一个字节都不产**；
      ② 自举不动点 3/3 gen1==gen2 byte-identical；
      ③ 全量 e2e（interp 338 + jit 672 + cross-zpkg 70 + multi-exe 3）与 stdlib 340 全过。
      若 CI 的 fingerprint 门判红，按 [[investigate-micro-ab-false-regressions]] 做字节对账
- [x] 5.2 自举不动点 **3/3**（`--workspace` gen1==gen2 byte-identical）
- [x] 5.3 全量 `xtask test e2e` 覆盖 interp(338) + **jit(672)** 两轮 + cross-zpkg(70) + multi-exe(3)，0 failed；
      `xtask test stdlib` 340 file(s) 全过

## 6. 文档（无文档 = 未完成）

- [x] 6.1 `docs/reference/src/language/module-initializers.md` 新页（语义 + 「一个包一个」+ 跨包顺序 = 实际加载顺序）+ 进 SUMMARY
- [x] 6.2 `docs/internals/src/runtime/static-ctor-init.md`：加 `$Module` 一节（为什么不能在锁内
      同步跑、登记/执行两段分工、为什么能覆盖自由函数、失败靠门不归零、编译期侧分工）
- [x] 6.3 `docs/reference/src/appendix/error-codes.md`：E0486 + E0485
- [x] 6.4 `static-ctor-init.md` 加「它推翻了一条批准惰性化时的前提」一节（#418 的无副作用前提失效 + 缓解）
- [x] 6.5 `docs/reference/src/SUMMARY.md` 入口 + `src/tests/README.md` 新增 `module-init/` 分类
      + `src/tests/cross-zpkg/README.md` 登记 5 条新 fixture
- [x] 6.6 doc-check：用户可见规则 → `docs/reference/src/language/module-initializers.md` + 错误码全表；
      实现机制 → `docs/internals/src/runtime/static-ctor-init.md` 的 `$Module` 一节；
      目录/入口变更 → 三处 README/SUMMARY（见 6.5）

## 7. 归档

- [x] 7.1 PR **#767**（已 rebase 到 main `2cb64a225` + 让号 E0484→E0486 + 重跑 GREEN 全绿）
- [ ] 7.2 合并后删分支 / worktree
- [ ] 7.3 `docs/spec/changes/add-module-init-hook/` → `docs/spec/archive/YYYY-MM-DD-...`
- [ ] 7.4 更新 memory：`add-module-init-hook-program.md` + MEMORY.md 索引
