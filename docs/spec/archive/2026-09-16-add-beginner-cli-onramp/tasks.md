# Tasks: 新手上手路径的 CLI 补齐

> 状态：🟢 已完成 | 创建：2026-09-15 | 完成：2026-09-16（#683 CLI 梳理 + #685 安装脚本；单文件运行拆出，见阶段 4）
> 分三个 PR：**PR-B1 命令面梳理 + CLI 修正**（阶段 1–3，分支 simplify-z42-cli）→ **PR-B3 安装脚本**（阶段 5）→ **PR-B2 单文件运行**（阶段 4）

## 进度概览

- [x] 阶段 1: z42.project 共享组件
- [x] 阶段 2: launcher / z42b / z42c 接入 + 命令面梳理（D11）
- [x] 阶段 3: PR-B1 测试与文档
- [x] 阶段 4: 单文件运行 —— **拆出为后续独立 change**（`add-single-file-run`，未开始；与学习手册第 3 章一起做），本变更不再承载
- [x] 阶段 5: 安装脚本（self-update 已按 D11 移除）

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
- [x] 3.3 GREEN（warm + 冷构建）→ PR #683（已合并）

## 阶段 4: 单文件运行（PR-B2）

- [x] 4.1 （拆出到 add-single-file-run）核实 `SourceDiscovery` 是否接受清单外绝对路径；不接受则扩展为接受字面文件路径（诊断保留原路径）
- [x] 4.2 （拆出到 add-single-file-run）launcher：`run *.z42` 合成清单到缓存目录（`Z42_CACHE_DIR` 覆盖）；路由 `.z42` 简写
- [x] 4.3 （拆出到 add-single-file-run）单文件声明依赖时的报错提示
- [x] 4.4 （拆出到 add-single-file-run）dist 用例：参数透传、改源重跑、错误路径、缓存隔离（两个同名文件不同目录）
- [x] 4.5 （拆出到 add-single-file-run）launcher 设计页删延后项 `launcher-future-single-file-exe-zpkg` + roadmap Deferred 索引行；GREEN → PR

## 阶段 5: 安装脚本 + self-update（PR-B3）

- [x] 5.1 `scripts/install/install.sh`（POSIX sh，D3/D4；D2=A 默认写 profile）+ `install.ps1`；实测真实 nightly 下载安装、profile 幂等、保留非 SDK 内容、sha256 未变跳过
- [x] 5.2 `scripts/install-z42.{sh,bat,command}` 改为薄封装（不改 profile）；CI `ci-bootstrap` 不使用它（自带下载），本地 `./.z42` 引导实测可用
- [x] 5.3 ~~launcher `self-update` 按 D9 重写~~ → 已移除（D11）
- [x] 5.4 CI：package-host 用 `--archive` 离线安装 + new/run 冒烟（4 OS）；release / publish-nightly 上传安装脚本资产；deploy-book 拷脚本到站点根
- [x] 5.4b apphost 运行时探测改为 SDK 根（D12）+ Rust 单测；`test dist` 的 publish 冒烟改用 SDK 根布局
- [x] 5.5 文档：`docs/workflow/release.md`（资产名更正）、quickstart、README、scripts/README、launcher 设计页安装章节
- [x] 5.6 GREEN（rebase 到含 #681 格式 bump 的 main 后冷构建重跑）+ dist 全量 → PR #685；归档随本 PR
