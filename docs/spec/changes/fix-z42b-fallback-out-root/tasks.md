# Tasks: `z42b test --out-root` 在零目标回落路径上被丢掉

**状态：🟡 进行中 | 开始：2026-10-01**

类型：`fix`（z42b，用户可见：`--out-root` 的产物落点）→ 最小化模式。「源码树零写入」系列的遗留项
（#975 归档时记下的「零目标回落 `_buildProject` 不认 `--out-root`」）。

## 问题

`z42b test <toml> --out-root <dir>`：`_runDevTargets` 把 out-root 套在**内存里**那份清单上（补 `output_dir`）。
清单没声明任何 `[[test]]`、而源里自带 `[Test]`（独立测试工程的形态）时，走「编项目自身再跑」的回落
`_buildProject(r, "")` —— 它**从磁盘重新加载清单**，内存覆盖到不了 ⇒ 产物照样写回 `<清单目录>/artifacts`
+ `<清单目录>/dist`。xtask 目前只对声明了目标的包传 `--out-root`，所以 GREEN 看不到；直接用 z42b 的人会踩到。

## 方案

- 覆盖逻辑抽成 `_applyOutRoot(m, outRoot)`，按目标跑与零目标回落共用。
- `_buildProject(r, mode)` 委托给新的 `_buildProjectAt(r, mode, outRoot)`（加载清单后套 `_applyOutRoot`）；
  回落路径传 out-root，其余调用方不变。
- `xtask test targets` 加 `_smokeFallbackOutRoot`：compile-then-test 夹具（零目标、源里带 [Test]）带 `--out-root`
  跑 ⇒ 断言通过 + 清单目录旁无 artifacts/dist + out-root 下有 zpkg。

## 进度概览

- [x] `builder_test.z42` / `builder_commands.z42`
- [x] `xtask_test_targets.z42`：`_smokeFallbackOutRoot`（修前判红「清单目录旁仍然出现了 …/artifacts」，修后绿）
- [x] 文档：`framework.md`（顺带更正 #977 之后已过时的「父包写进源码树」）、`cli-z42.md`
- [x] 本地 GREEN（基底 1fb724594，9m20s，src 下零产物）
- [ ] PR CI
