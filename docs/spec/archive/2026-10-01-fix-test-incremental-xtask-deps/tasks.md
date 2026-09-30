# Tasks: `xtask test incremental` 的 xtask 轮编不过（拷贝工程缺编译器域 path 依赖）

**状态：🟢 已完成 | 开始：2026-10-01 | 完成：2026-10-01（#984）**

类型：`fix`（xtask 测试门自身）→ 最小化模式。

## 问题

`xtask test incremental` 在第 ② 份语料（xtask 自身）上直接 `✗ initial build failed`：

```
E0494: 命名空间 `Z42.Project` 不存在（依赖的包里没有它，本包也没有声明它）
```

`_incrXtaskCopy` 把 `scripts/**/*.z42` 拷进 scratch，配一份**替身清单**（只有 `[project]` + `[sources]`）。
relocate-compiler-domain-libs（2026-09-27）把 `Z42.Project` / `Z42.Build` 挪出 `libs/`，真品
`scripts/xtask.z42.toml` 改为 path 依赖它们 —— 替身清单没跟上，按名走 `Z42_LIBS` 解析不到。

本门不在 `xtask test`（GREEN）也不在 CI 里，所以坏了没人发现。

## 方案

替身清单的 `[dependencies]` 从真品清单读出 path 依赖、改写成绝对路径（`_incrXtaskPathDeps`）。不在测试里
抄一份依赖列表 —— 下次 xtask 加减依赖时本门自动跟上。名字依赖不用抄（按名走 `Z42_LIBS`，拷贝与真品同解）。

## 进度概览

- [x] `scripts/test/xtask_test_incremental.z42`：`_incrXtaskPathDeps`
- [x] `xtask test incremental` 全绿（demo / demo-packed / xtask 87/87 三轮 / 各旋钮 / 泛型体 / 警告 / stdlib 读回；约 47 分钟，大头是 xtask 轮 87 文件 × 3 轮）
- [x] 本地 GREEN（基底 b1fb49b2f，10m09s）
- [x] PR CI：9 pass（其余按 path filter skip；本门本身不在 CI）
