# Tasks: Phase-1 旋钮绕过配置层 —— `libs` / `path` / `crash-dir` 只认 env

> 状态：🟢 已完成 | 创建：2026-09-05 | 完成：2026-09-05

**变更说明：** `Z42_LIBS` / `Z42_PATH` / `Z42_CRASH_DIR` 三个旋钮在登记表里标了
`..PUBLIC` + `toml_key`（= 声称四层全收），但它们的消费点仍在 `std::env::var` 直读，
于是配置文件 / `--set` / 应用侧车层的值解析进了 `RuntimeConfig`、`--show-config` 也如实
报 `[user-config]`，**实际启动路径完全看不到**。本次把四个消费点改成读 `cfg.<field>`，
并加一层守在「解析 → 消费」这条缝上的回归测试。

**原因：** 登记表自己的注释（`knob_table.rs:38-44`）写着：消费点内联读 env 的旋钮必须标
`ENV_ONLY`，标成四层全收会让 `--list-knobs` 声称能设而实际静默无效，「**比不登记更坏**」。
这三个旋钮正是它警告的那种状态，只是方向相反——已标 PUBLIC 却没兑现。#443 / #446 刚把
配置文件层送到嵌入方与已发布 app，这个缺口因此从「内部不一致」变成用户可见。

**文档影响：** `docs/book/src/runtime/runtime-settings.md`（在「`sources` 不是所有旋钮都
给全」一节补反向陷阱：标 PUBLIC 就必须真的从 `RuntimeConfig` 读，附自相矛盾的 `--info`
实录 + 二选一出路）、`config/knob_table.rs` 的三条 `consumed_by`（用户可见，经
`render.rs:67` 渲染成 "read by"，`main.rs` → `startup.rs`）。

## 实测依据

复现（修复前，同一次 `--info` 自相矛盾）：

```
$ cat z42.toml
[runtime]
libs = "…/.z42/libs"

$ Z42_CONFIG=z42.toml z42vm --info | grep -i '^libs'
libs = …/.z42/libs                              [user-config]   ← 解析层读到了
libs dir: (not found — run xtask build stdlib or set Z42_LIBS)  ← 消费点没读

$ Z42_LIBS=…/.z42/libs z42vm --info | grep -i '^libs'           ← env 走另一条路
libs = …/.z42/libs   [env]
libs dir: …/.z42/libs
```

修复后 A/B/C/D 四例全部正确：配置文件生效；env 仍生效；两者皆无 → 仍回落搜索路径；
**env 显式但指向不存在目录时仍压过配置文件**（保持旧的 `is_dir()` 落空即穿透语义，
没有因为放开层而放松校验）。

回归测试有效性已验证：把 `startup.rs` 临时改回 env 直读 → 5 个测试里那 2 个针对性的
**确实变红**（另外 3 个是「没放松校验」的不变量测试，两侧都绿，符合预期）。

## 阶段 1: 消费点改读解析后的配置
- [x] 1.1 `startup.rs::resolve_libs_dir` 加 `cfg: &RuntimeConfig` 参，第 1 步读 `cfg.libs_dir`
- [x] 1.2 `startup.rs::resolve_module_paths` 同款；顺带修好 Windows——旧代码对原始 env
      串恒按 `':'` 切分，而 `cfg.module_path` 已由配置层按平台分隔符切好
- [x] 1.3 `startup.rs::install_panic_hook` 改收 `Option<PathBuf>`，install 时捕获；
      panic 路径因此不再调 `getenv`（正在 panic 时少一件会出错的事）
- [x] 1.4 `signal_handler::install` / `open_crash_report_fd` 改收 `Option<&Path>`
- [x] 1.5 `print_build_info` 加 `cfg` 参 —— `--info` 的 "libs dir:" 行与上方旋钮表
      从此走同一个 `cfg`，不可能再自相矛盾
- [x] 1.6 调用点更新：`main.rs` ×4、`signal_handler_tests.rs` ×2、
      `examples/signal_crash_helper.rs`（无配置文件层，走 `runtime_config()` 的
      env-only 惰性回落，保持父测试 `cmd.env("Z42_CRASH_DIR", ..)` 的期望不变）

## 阶段 2: 守住这条缝
- [x] 2.1 新增 `src/runtime/src/startup_tests.rs`（5 测）：配置层的值必须被消费；
      不存在的覆盖值必须穿透；无覆盖不 panic；module_path 生效；不存在的条目被过滤
- [x] 2.2 反向验证：临时还原 env 直读，确认 2 个针对性测试变红

## 阶段 3: 文档同步
- [x] 3.1 `runtime-settings.md` 补反向陷阱注记
- [x] 3.2 三条 `consumed_by` 更正到 `startup.rs`

## 延后（不在本次）

- 其余 `INLINE_ENV` 旋钮（`Z42_JIT_THRESHOLD` / `Z42_STACKALLOC` / `Z42_NO_FUSION` 等）
  仍是 env-only，**但它们标的就是 `ENV_ONLY`，登记表与实现一致，不是缺陷**。要放开层
  需各自收编进 `RuntimeConfig`（登记表注释里已登记为 deferred）。
- `metadata/superinstr.rs:122-130` 每编译一个函数做 3 次 `env::var`（`build_block_indices`
  按函数调用），属性能问题不是正确性问题，另案。
