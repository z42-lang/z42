# Tasks: fix-iface-satisfaction-gaps

> 🔴=未开始 🟡=进行中 🟢=完成 ｜ proposal/design 见同目录
> 三 commit：P0（E0463 碰撞）/ #1（种类+返回校验）/ #2（base 假红）

## P0 — E0463 碰撞

- 🟢 P0-1 DiagnosticCodes 加 `ForwardMalformed = "E0468"` + 注释（四子类）。
- 🟢 P0-2 ForwardGenerator 4 处 `"E0463"`→`"E0468"`（:104/:126/:163/:208）。
- 🟢 P0-3 grep 全仓无「E0463 用于 Forward」残留；`field_trigger_tests.z42` 3 处断言同步 E0468。

## #1 — 接口满足性种类 + 返回类型

- 🟢 1-0 **阶段 0 爆炸半径**：`rm .cache` 全量重编 `build stdlib`+`build compiler`+`build test`
  → E0412 **命中 0**。⚠️ 首测漏了 `build test`（测试语料）+ 增量缓存假阴性 → 见 design 两条血泪教训。
- 🟢 1-1 `_checkOneIfaceMethod`：MangleKey 命中后加**种类**校验（`cm.IsStatic != ims.IsStatic` → E0412）。
- 🟢 1-2 同处加**返回类型**校验：`TypeKey` 归一相等（非 IsAssignableTo，见 design）或子类/接口协变 → 放行，
  否则 E0412。协变允许（裁决已定）。
- 🟢 1-3 **只对本包接口**（`it.IsImported` 保守跳过）：导入接口 static-abstract 成员 IsStatic 丢失（新照亮的
  latent bug → Deferred `imported-iface-static-member-fidelity`），跨包漏报不假红。
- 🟢 1-4 负例门（`constraint_tests.z42` 5 条）：static↔instance / 返回不符 → E0412；协变返回 + static 相符
  → 不报（守卫）。**退回对照**坐实（logic revert → 7 门 FAIL）。

## #2 — base 约束假红

- 🟢 2-1 `_satisfiesBase`/`_satisfiesParamRef` 开头放行 `Z42GenericParamType`/Error/Unknown（同 `_satisfiesInterface`）。
- 🟢 2-2 新 fixture `test_ok_forward_type_param_to_base_constrained_inner`：修前假红 E0402、修后 0。退回对照坐实。

## GREEN + 落地

- 🟢 G-1 验证：**自举不动点 3/3 gen1==gen2**（`test compiler`）+ 23 units + 9 门；golden regen 通过
  （`static_abstract_operator` 编译干净）；**stdlib 333 文件全过**（隔离，http_server_threaded 单跑绿）；
  `test e2e --dir cross-zpkg --mode jit` ✔；`test bootstrap` ✅（无格式 bump）。⚠️ 完整 `xtask test` 一轮曾
  卡 `http_server_threaded`——查明是**与另一会话并发争端口**（隔离单跑绿），非本改动；`test stdlib --mode jit`
  略过（emit-neutral，不动点已证零字节漂移 ⇒ jit 同 zbc 同果）。
- 🟢 G-2 文档同步：`generics.md` static-abstract 表下加「接口满足性现在真比种类与返回类型」说明。
- 🟡 G-3 归档 changes→archive + tasks 改 🟢；PR（合并前 rebase+重跑 GREEN；等 User sign-off 再 merge，
  合并后删分支/worktree）。

## 环境（已就绪）

- worktree `../z42-iface-gaps`（branch `fix-iface-satisfaction-gaps`，基于 origin/main `e96522990`）；
  nightly SDK 冷种子 `deb7587cf`=main−2 零格式 skew。每条命令带 `Z42_PORTABLE_VM=$PWD/.seedvm/z42vm`。
