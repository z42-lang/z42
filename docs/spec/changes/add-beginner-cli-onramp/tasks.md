# Tasks: 新手上手路径的 CLI 补齐

> 状态：🟡 进行中 | 创建：2026-09-15
> 分三个 PR：**PR-B1 命令面梳理 + CLI 修正**（阶段 1–3，分支 simplify-z42-cli）→ **PR-B3 安装脚本**（阶段 5）→ **PR-B2 单文件运行**（阶段 4）

## 进度概览

- [x] 阶段 1: z42.project 共享组件
- [x] 阶段 2: launcher / z42b / z42c 接入 + 命令面梳理（D11）
- [ ] 阶段 3: PR-B1 测试与文档
- [ ] 阶段 4: 单文件运行（PR-B2）
- [ ] 阶段 5: 安装脚本 + self-update（PR-B3）

## 阶段 1: z42.project 共享组件

- [x] 1.1 `ManifestLocator.FindUp(startDir)`（D1 四条规则，候选排序确定）+ `[Test]`
- [x] 1.2 `BuildLayout.Resolve(manifest, profile)`：从 z42c `BuildPaths` 下沉，z42c 改为调用它（产物字节不变）+ `[Test]`

## 阶段 2: 接入

- [x] 2.1 launcher：`build` / `run` / `publish` 无路径时定位；删 `_findProjectToml` glob 兜底；`publish` positional 改可选
- [x] 2.2 launcher `run`：按 `BuildLayout` 找产物；内部构建传 `--quiet`
- [x] 2.3 launcher：`--version` / `-V` / `version` / `help <cmd>`（D7）
- [x] 2.4 z42c `build`：无参定位、失败退出 2；新增 `--quiet`
- [x] 2.5 z42b：`build` / `test` / `bench` / `clean` 使用定位；`clean` 按 `BuildLayout`
- [x] 2.6 `z42 new`：`--path` 语义、名字校验、提示语、README、删 PARKED 注释；`--test` 模板移除（D11）
- [x] 2.7 移除 D11 所列命令与死代码；launcher 依赖收缩；workload export 提示改 `z42 workload install`；book 新增 `toolchain/cli.md`

## 阶段 3: PR-B1 测试与文档

- [x] 3.1 `xtask test dist` 新增 design 测试表中 CLI 相关用例（先红后绿，做阴性对照）
- [x] 3.2 文档：book `compiler/tools.md`（`z42 new` / `build` 已接入）、`project-build.md`、launcher 设计页、`docs/workflow/quickstart.md`、README
- [ ] 3.3 GREEN（含冷构建：清 `src/compiler/*/dist` 与 z42b fixture 产物后跑一轮）→ PR

## 阶段 4: 单文件运行（PR-B2）

- [ ] 4.1 核实 `SourceDiscovery` 是否接受清单外绝对路径；不接受则扩展为接受字面文件路径（诊断保留原路径）
- [ ] 4.2 launcher：`run *.z42` 合成清单到缓存目录（`Z42_CACHE_DIR` 覆盖）；路由 `.z42` 简写
- [ ] 4.3 单文件声明依赖时的报错提示
- [ ] 4.4 dist 用例：参数透传、改源重跑、错误路径、缓存隔离（两个同名文件不同目录）
- [ ] 4.5 launcher 设计页删延后项 `launcher-future-single-file-exe-zpkg` + roadmap Deferred 索引行；GREEN → PR

## 阶段 5: 安装脚本 + self-update（PR-B3）

- [ ] 5.1 `scripts/install/install.sh`（POSIX sh，D3/D4；D2 结论）+ `install.ps1`
- [ ] 5.2 `scripts/install-z42.{sh,bat,command}` 改为薄封装（`installed-by = "repo"`、不改 profile）；确认 CI `ci-bootstrap` 与本地 `./.z42` 引导不受影响（冷启动入口清单，bootstrap-seed.md）
- [x] 5.3 ~~launcher `self-update` 按 D9 重写~~ → 已移除（D11）
- [ ] 5.4 CI：package-host 用 `--archive` 离线安装验证（4 OS）；release / publish-nightly 上传安装脚本资产；deploy-book 拷脚本到站点根；nightly 发布后真实 URL 安装冒烟
- [ ] 5.5 文档：`docs/workflow/release.md`（资产名更正）、quickstart、launcher 设计页安装章节（含 D2 决策改写）
- [ ] 5.6 GREEN + CI 三 OS 绿 → PR；归档随最后一个 PR
