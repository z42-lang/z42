# Tasks: 单文件运行 `z42 run hello.z42`

> 状态：🟢 已完成 | 创建：2026-09-16 | 完成：2026-09-16

## 进度概览
- [x] 阶段 1: 源发现与诊断路径（编译侧）
- [x] 阶段 2: launcher 单文件路径
- [x] 阶段 3: 测试
- [x] 阶段 4: 学习手册第 2/3 章 + examples
- [x] 阶段 5: 文档同步与归档

## 阶段 1: 源发现与诊断路径（编译侧）
- [x] 1.1 `SourceDiscovery._expand` 前置分支：rooted 且无 `*`/`?` 的 pattern → 存在则返回单元素，
      否则空数组（`src/libraries/z42.project/src/SourceDiscovery.z42`）
- [x] 1.2 z42c driver 分离显示名：新增按 cwd 相对化的 `displaySrcs`，`cin.Files = displaySrcs`，
      读取仍用 `srcs`（`src/compiler/z42c.driver/src/Main.z42` 第 297 行附近）
- [x] 1.3 警告呈现处（第 386 行附近 `"warning(s) in " + srcs[wi]`）与错误呈现处
      （第 360 行附近 `origin`）改用显示名

## 阶段 2: launcher 单文件路径
- [x] 2.1 缓存目录解析：`${Z42_CACHE_DIR:-<SDK 根>/cache}/run/<abs 路径哈希>/`
      （`src/toolchain/launcher/core/launcher.z42`）
- [x] 2.2 `_synthSingleFileManifest`：写 `z42.toml`（`kind="exe"`、name 合法化、include=绝对路径），
      每次运行重写；**不复制源文件**
- [x] 2.3 `_cmdRun` 单文件分支：目标以 `.z42` 结尾 → 校验存在（不存在 → 退出码 2）→ 合成清单 →
      复用 `_buildAndResolveRun`；与 `--bin` 同用报错
- [x] 2.4 `_isSourceProject` 放行 `.z42`
- [x] 2.5 `launcher_cli.z42` 简写路由扩到 `.z42`
- [x] 2.6 `_printRunHelp` 补单文件用法 + 「单文件只能用标准库，需要依赖用 `z42 new`」

## 阶段 3: 测试
- [x] 3.1 `src/libraries/z42.project/tests/source_discovery_glob.z42`：rooted 命中 / 不存在 /
      含通配符仍走 glob / 相对字面路径语义不变（四条）
- [x] 3.2 `scripts/test/xtask_test_dist_cli.z42`：`z42 run hello.z42` / `z42 hello.z42` /
      `-- <args>` / 诊断路径为 `hello.z42(…)` 且不含绝对路径 / 源文件不存在 → 退出码 2 /
      当前目录无新增文件
- [x] 3.3 回归确认：既有断言 `./src/Main.z42(6,5): E0401` 一字不变通过（D2 恒等性证明）

## 阶段 4: 学习手册第 2/3 章 + examples
- [x] 4.1 `examples/getting-started/hello-world/` 重排为单文件三例（hello / greet / typo），
      删 `new/`（移入第 3 章）与各例的 `z42.toml` + `src/`
- [x] 4.2 重写 `docs/learn/src/getting-started/hello-world.md`：写文件 → 运行 → 读代码 →
      参数 → 出错 → 小结（不出现 namespace / z42.toml / glob）
- [x] 4.3 新建 `examples/getting-started/projects/`：`new/new.console` + 多源文件工程 + build/clean 会话
- [x] 4.4 新建 `docs/learn/src/getting-started/projects.md`（第 3 章）：什么时候需要工程 →
      `z42 new` → `z42.toml` 各字段 → `build` / `--release` / 产物 / `clean` → 多源文件 → 小结
- [x] 4.5 `SUMMARY.md` 挂入第 3 章；`OUTLINE.md` 更新第 2/3 章要点与状态
- [x] 4.6 `xtask test examples` 全绿（先 `xtask build sdk`）

## 阶段 5: 文档同步与归档
- [x] 5.1 `docs/book/src/toolchain/cli.md`：`z42 run` 补单文件用法
- [x] 5.2 `docs/design/runtime/launcher.md`：删 Deferred 条目 `launcher-future-single-file-exe-zpkg`，
      改写为已实现；**新增** Deferred 条目 `launcher-future-single-file-cache-gc`（回收方案见 design D1）
- [x] 5.2b `docs/roadmap.md` Deferred Backlog Index 加 `launcher-future-single-file-cache-gc` 索引行
      （旧条目 `launcher-future-single-file-exe-zpkg` **从未**登记过索引，违反 philosophy.md
      「延后必须就近记录 + roadmap 索引」双处登记规则——本次顺带补齐这条纪律）
- [x] 5.3 归档前 doc-check 清单逐项核对（含命令面新增后的 grep 核查）
- [x] 5.4 tasks 标 🟢 + `changes/` → `archive/2026-09-16-add-single-file-run/`（**PR 内完成**）

## 备注

**GREEN（基于 main c7f9f8ff2）**：`xtask test` **13 stage 全绿**（5m06s）；`xtask test dist` 655 通过 / 0 失败
（含新增 6 条单文件断言）；`xtask test examples` 5 脚本 14 步真实重放全绿。

**Scope 外发现（未修，可另开 change）**：`z42 build` 的 `cache -> …` 与 `z42 clean` 的 `removed …` 打的是
**绝对路径且含 `/./`**（如 `…/greeter/./artifacts`），教程 transcript 只能用通配符糊掉。与本次修的诊断
路径同属「呈现给用户的路径不体面」一族。

**实施中踩到、值得记住的两处**：
- `IncrementalBuild.Rel` 对工程目录外的源原样返回绝对路径 ⇒ `Path.Join(cacheDir, rel)` 直接返回该绝对
  路径 ⇒ `.zbc` / `.meta` **写到用户源文件旁边**。`_writeCacheZbc` 还内联复制了一份 `Rel` 的逻辑，
  故第一次只修 `Rel` 仍漏掉 `.zbc`——已改为调用 `Rel`，重复消除。
- 显示名一度被塞进 `cin.Files`，而它直接成为 zpkg 的 `ZbcFileZ.SourceFile` ⇒ **可复现 build_id 判红**
  （GREEN 抓到）。显示名只能走 Parser，不得进产物。

- **已实测的事实**（别重查）：
  - 单文件 hello **不需要 `namespace`**，5 行可跑
  - 绝对路径写进 `[sources].include` 当前得到 `z42c build: no sources matched`（D3 必需）
  - `z42 run hello.z42` 现状：落到 VM → `unrecognised artifact extension Some("z42")`，退出码 1
  - `z42 hello.z42` 现状：`unknown command`，退出码 2
  - `_stabilizeSourceIdentity` 已在写 zpkg 前相对化 `SourceFile` ⇒ 改显示名不动产物字节
- **环境**：worktree `wt-run`（基于 `origin/main` c7f9f8ff2）。种子格式坑见 memory
  `z42-learn-book-line`：main 已 bump 到 zpkg 0.47，供种后必须
  `rm -rf artifacts/build/{compiler,libraries}` + 重建 `xtask.zpkg` 再冷建，否则 strict-pin 拒读。
- 本机 rustc 需 `RUSTUP_TOOLCHAIN=1.98.1`。
