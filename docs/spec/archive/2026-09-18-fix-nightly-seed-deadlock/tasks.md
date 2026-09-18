# Tasks: 破除 nightly 种子死锁

> 状态：🟢 已完成 | 完成：2026-09-18

- [x] 1 `ci-bootstrap` 种子下载加回退链：nightly →（拿不到 / 无 `programs/z42c/`）
      最近 5 次成功 CI 运行的 `z42-host-package-*` artifact
- [x] 2 按 RID **后缀**（`z42-*-<rid>-release`）挑目录，不硬编码 runner 标签
- [x] 3 修错误信息里被 shell 当命令替换吃掉的反引号
- [x] 4 更新 ci.yml 的 "escape hatch" 注释 —— 原文说 publish-nightly「与 bootstrap 解耦」，
      **实测不成立**（它的 needs 全是 bootstrap job）
- [x] 5 `internals/devinfra/ci.md` §3.1：回退链 / 为什么不能回落正式 release /
      权限要求 / 2026-09-17 事故复盘 + 人工恢复流程 / 残留缺口

## 🔴 第一版是错的，必须记下来

第一版回落到**最近一个正式 release**。桩测「走到了回退分支」就当成立，**推到 CI 上实测挂了**
（run 35290607109）：

```
fallback seed: v0.5.0    ← 回退分支确实走通了
zpkg minor: seed=43  current=48      ← 跨 5 个格式 bump
[1.5] 两代自举 …
Error: z42.core.zpkg … zpkg minor 48 not supported (writer is at 0.43)
```

两代自举**全程用种子自带的旧 VM**，它加载不了 gen1 产出的新格式 stdlib。跨一个 bump 能扛，
跨五个不行。

**教训**：桩测只能证明「分支被选中」，证明不了「选中之后能跑通」。回退路径的验收标准是
**种子能编出当前源码**，不是「代码走到了那一行」。

换成 CI artifact 后根本不进两代路径（artifact 顶多落后一两个 commit，格式天然一致，
实测 minor=48 == current）。顺带省掉 tar/zip 与 EXT 分支——artifact 里是已解包的 SDK 目录。

## 验证（桩版 gh + sleep，四条路径）

| 场景 | 期望 | 实测 |
|---|---|---|
| nightly 正常 | 用 nightly，不碰回退 | ✅ `seed_src=nightly`，`run list` **0 次调用** |
| nightly 下载不到 | 回落 CI run | ✅ `seed_src=ci-run-111` |
| **nightly 能下但无 z42c 种子** | 回落 CI run | ✅ 同上 ← 事故的真实形态 |
| 最新成功 run 的 artifact 已过期 | 跳到下一个；都不行则报错 | ✅ 逐个试 → 带恢复指引的 `::error::` |

另：`bash -n` + 两个 YAML 的 `yaml.safe_load` 均通过。

人工侧另有一次**端到端实证**：2026-09-18 用同一批 artifact 手工重发 nightly，四平台 SDK 包
解包后 `programs/z42c/` 各 5 个 zpkg、`bin/z42vm` 齐全、driver minor=48，重发后
CI（run 35296060668）的 `compile-toolchain` / `verify-selfhost` 全部转绿 —— 证明这个种子源可用。

## 备注：未修的根因

**`publish-nightly` 仍可能被整个 workflow 的取消打断**（`cancel-in-progress: false` 只管
该 job 的并发，不管 run 被 cancel），仍会留 stuck-draft。本 change 让它**不再致命**
（CI 能继续跑并自动重发健康 nightly），但没消除它。

彻底修需二选一，均超出本 change：
- 把 publish-nightly 拆成独立 workflow（不随 CI run 被取消）
- 改成 `create → 上传 → 转正` 而非 `delete → create`（不留中间态）
