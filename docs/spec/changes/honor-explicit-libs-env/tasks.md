# tasks: honor-explicit-libs-env

> 类型：**fix**（apphost 静默丢弃显式 `$Z42_LIBS`）｜ 创建：2026-09-27
> 出身：结构审计 2026-09「批 B：给静默的地方装门」——`Z42_LIBS` apphost 那条。

## Why

`hostrun.rs::exec_app` 无条件 `cmd.env("Z42_LIBS", &rt.libs)` ⇒ 用户显式设的值被
**静默丢弃**。实测（patch 过的 apphost + 回显 `Z42_LIBS` 的 `z42vm` 桩）：

| `Z42_LIBS` | 子进程实际看到 |
|---|---|
| 未设 | `<runtime>/libs` |
| `/my/explicit/libs` | `<runtime>/libs` ← **被吞** |

影响面远不止已发布的 app：**SDK 自己的 `bin/z42c` 就是 apphost**
（`packaging.md` 的 `kind = apphost`，`builder_publish_build.z42:60` 也这么称它）⇒
拿装好的工具链跑 `Z42_LIBS=… z42c build …` **完全无效**。而 `Z42_LIBS` 在
`knob_table.rs` 里标着 `PUBLIC`、在 `runtime-settings.md` 是有名有姓的一行 ——
「一个从不生效的旋钮 = 没有旋钮」，与批 B 其余几条同族。

这条在本项目记忆里是**有案底的**：「`Z42_LIBS` 对装好的 z42c 无效（恒扫 SDK 自己的
libs）—— 害我三轮假阴性」。当时记成了「z42c 的毛病」，实际根因在 apphost 这一行。

## What Changes

新增纯函数 `libs_env_for_child(current, libs)`，规则与 VM 侧
`libs_env_to_publish` **逐字对齐**：只填未设/为空（空串等同未设）。

⚠️ **不能简化成「不设」**：安装布局下 z42vm 在 `<dir>/z42vm`，VM 自己的第 ② 档探
`<binary-dir>/../libs` = `<dir>/../libs`，**不是** apphost 找到的 `<dir>/libs`。
删掉这次 set，安装布局就定位不到 stdlib —— 所以这是「加条件」，不是「删代码」。

顺带把 command 组装抽成 `build_app_command(…, current_libs)`，把环境决策变成可测的纯
输入（`env::set_var` 在 `cargo test` 多线程下是竞态的，不能靠它写测试）。

## Scope（允许改动的文件）

- `src/toolchain/workload/desktop/platform/apphost/src/hostrun.rs`
- `docs/internals/src/runtime/vm-architecture.md`（两个写入方、同一条规则）
- `docs/reference/src/toolchain/runtime-settings.md`（`libs` 条目 + 旧 SDK 绕行法）

## Tasks

- [x] `libs_env_for_child` + `build_app_command`，`exec_app` 改为经它们
- [x] 4 条单测：未设填 / **显式不覆盖** / 空串等同未设 / 无 `libs/` 时不设
- [x] 阴性对照：撤回修复后**恰好** `explicit_libs_is_left_untouched` 变红（21 passed, 1 failed），
      另三条仍绿 ⇒ 证明「未设仍填」那半没被改坏
- [x] 端到端复验真二进制（三臂：未设 / 显式 / 空串），同一 fixture 同一路径、只变代码
- [x] 文档：internals 交接规则表 + reference `libs` 条目
- [ ] GREEN：CI 全矩阵绿

## 验证配方（下次要复现这条，照着做）

```sh
cd src/toolchain/workload/desktop/platform/apphost && cargo build   # → artifacts/build/runtime/debug/apphost
# 造靶子：假 z42vm 只回显它收到的 Z42_LIBS
mkdir -p t/.z42/libs && printf '#!/bin/sh\necho "[$Z42_LIBS]"\n' > t/.z42/z42vm && chmod +x t/.z42/z42vm
touch t/app.zpkg && cp artifacts/build/runtime/debug/apphost t/myapp
# patch 占位符：magic "z42-apphost-target-v1-MAGIC-0001" 之后写 "app.zpkg\0"
codesign -s - -f t/myapp      # 🔴 macOS 必须重签，否则内核直接卡住/杀掉（builder_apphost.z42:49 早写了）
cd t && env -u Z42_PORTABLE_VM -u Z42_HOME Z42_LIBS=/my/explicit/libs ./myapp
```

⚠️ 我第一次跑这个探针「挂住」了 120s，差点误判成死锁 —— 实际是 patch 破坏了代码签名。
patch 完必重签。

## 不做（Out of Scope）

- **不改 `resolve_libs_dir` 的探测顺序**，也不动第 ② 档 `<binary-dir>/../libs` 那条
  （安装布局与它差一级是既有事实，apphost 的 set 正是为它兜的底；要收敛是另一条线）。
- **不给「显式值指向一个坏目录」加校验**。与 VM 侧同一口径：显式但坏是调用方的刻意选择，
  strict-pin 会在加载期响，不在这里替用户判断。
