# Tasks: app-config-follows-the-app

> 状态：🟢 IMPL 完成（P0–P2 + 文档） | [proposal.md](proposal.md) · [design.md](design.md) · [spec](specs/runtime-settings/spec.md)

## P0 — 推导 + z42vm 接入
- [x] 0.1 `config/source.rs`：`pub fn sidecar_for(app_file: &Path) -> Option<PathBuf>`
      （替换扩展名为 `runtimeconfig.toml`，`is_file()` 才返回 Some；不读文件）
- [x] 0.2 `main.rs`：`Inputs.app_config` = 显式 `Z42_APP_CONFIG` 优先，否则 `sidecar_for(cli.file)`
- [x] 0.3 单测：stem 派生 / 不存在 / 是目录 / `.zbc` 也行 / 显式优先
- [x] 0.4 e2e：`z42vm <payload>.zpkg` 直跑 → `source=app-config`；`--show-config <app>` 同理；
      不给文件时输出不变
- [x] 0.5 GREEN

## P1 — 嵌入入口接入
- [x] 1.1 `z42-host::run_app`：调 `app::run` 前装配含推导 app 层的配置；已装配则不覆盖
- [x] 1.2 单测（z42-host crate）
- [x] 1.3 GREEN

## P2 — 删 apphost 冗余
- [x] 2.1 删 `hostrun.rs::app_config_sidecar` + `exec_app` 里的注入 + 4 个单测
- [x] 2.2 dist smoke 仍绿（apphost 启动的 app 现在由 z42vm 推导拿到侧车）
- [x] 2.3 GREEN

## 文档
- [x] `docs/book/src/runtime/runtime-settings.md`：「侧车怎么到达已发布的 app」一节改写
      ——约定归 VM 一处，调用方只在路径已在手时转发

## 未决
无。


## 落地记录（2026-09-05）

**手工端到端**（每条都先复现了旧行为再验新行为）：
`z42vm <payload>.zpkg` 直跑 `source=default` → `source=app-config`；
`--show-config <app>` 显示 `[app-config]`；不给文件时仍 `[default]`；
`Z42_CONFIG` 逐 key 压过侧车并记 `ignored [app-config]`。
apphost 路径用**重新 publish 的新 stub**（`strings | grep -c runtimeconfig` = 0，
确认发现逻辑已删）验证仍得 `source=app-config`——证明删除安全。

**顺带拆分**：`z42-host/src/lib.rs` 566→497（`install_app_config` → `app_config.rs`、
两个 resolver → `resolvers.rs`），从 line-limit 棘轮基线剔除。中途一次失误：`mod` 声明
被插进 `run_app` 的文档注释与函数之间，文档挂到了模块上——已移到文件顶部。

**GREEN**：runtime cargo 1152/0；apphost cargo 17/0；release 无新告警（11 = main 基线）；
自举不动点 3/3；e2e 566 + cross-zpkg 17 + multi-exe 2；launcher dist smoke 3/3；
lines 31 known / 0 new-grown（基线只发生删除）。

## 剩余
- `runtimeconfig.template.toml` 手写模板合并（独立项）
