# Tasks: CI 的 xtask 调用收敛到一个垫片

**状态：🟢 已完成 | 开始：2026-09-30 | 完成：2026-09-30（#958）**

类型：`refactor`（CI 编排，不改构建/测试行为）→ 最小化模式。叠在 `speed-up-ci-quick-wins` 之上。

动机：ci.yml 里 38 处手抄同一段样板（`vm=…; [ -f "$vm.exe" ] …; Z42_PORTABLE_VM=… Z42_LIBS=… "$vm" artifacts/xtask/xtask.zpkg -- …`），
z42vm / flat stdlib / xtask.zpkg 三个位置因此在 CI 里被复制 50+ 次；Windows 的 `.ci-runner` 拷贝技巧又各写一遍。
这是整理 `artifacts/` 布局的前置：不先收敛，挪一次目录就要改 200+ 行 yaml。

## 进度概览

- [x] 阶段 1: `.github/ci/xtask` 垫片 + 两个 bootstrap action 把它加进 PATH
- [x] 阶段 2: ci.yml 37 处调用改为 `xtask <cmd>`；删除随之无用的 vm / libs / runner 变量与 Windows 拷贝块
- [x] 阶段 3: 文档（ci.md「步骤里怎么调 xtask」）
- [x] 阶段 4: 本地验证（shellcheck / actionlint / `test ci-shell` / macOS 上用垫片实跑 `deps check`）
- [x] 阶段 5: PR CI 实测：26 个检查全过，含 Windows 的 test-host / package-host（拷贝启动路径）
- [x] 阶段 6: 归档

## 不在本 change

- `bench-pr.yml`：同一 job 里分别驱动 base 与 PR 两棵树，需要能指定树根的形态，另做。
- `release.yml`：只在打 tag 时跑，PR 上验证不到；待 ci.yml 的垫片形态经过几轮 CI 后再迁。
- `test-consume`：故意用下载来的 current-sdk 里的 z42vm，不走垫片。
