# Design: test workload 打包发布 + workload 描述泛化

## Architecture

```
  现有 workload 打包（tooling 形态，per-RID apphost/facade）
    _buildDesktopWorkload(rid) → z42-workload-<v>-desktop-<rid>/  (apphost-<rid> + manifest)
    _releaseAssembleDesktop     → merge 4 RID → z42-workload-<v>-desktop.tar.gz

  新增 payload-only（test 形态，无 per-RID，平台无关单 zpkg）
    _buildTestWorkload(v) → z42-workload-<v>-test/
        ├── z42.testagent.zpkg          (编自 agent/z42.testagent.z42.toml)
        └── manifest.toml               (kind=workload-tooling, host=["*"], runtime-pack="",
                                         [contents.payload] payload="z42.testagent.zpkg")
      → tar → z42-workload-<v>-test.tar.gz     (无 merge：无 per-RID piece)

  release-index.json
    workloads.test = { archive, sha256, host:["*"], runtimes:[] }

  install（现有 CLI，manifest 驱动，无需改逻辑）
    z42 workload install test
      → _fetchWorkloadEntry("test") → 下载 tar → 解压到 runtimes/<v>/workloads/test/
      → runtimes=[] → bedding 循环跳过（desktop 已是此路径）
```

## Decisions

### D6（承 archived design D6，User 2026-08-29 裁决）: 复用 kind="workload-tooling" + 新 [contents.payload]

**问题：** test workload 是纯 payload（单 zpkg，无 apphost/facade/runtime），用哪个 kind？现有
`[contents.platform]` 键（apphost-prefix/swiftpm/npm/gradle）都不描述「单 zpkg payload」。
**选项：** A — 复用 `kind="workload-tooling"`（`runtimes=[]`）+ 新增 `[contents.payload]` 段；
B — 新 `kind="workload-payload"`。
**决定（User）：** **A**。理由：① 所有现有 workload 已用 `workload-tooling`，语义 = 「非 runtime 的
工具 tar」，test payload 属之；② desktop 已示范 `runtimes=[]`（无 runtime 依赖），install/bedding 天然
跳过，无需新分支；③ 只需新增一个 contents 描述段（`[contents.payload]`），不改任何判 kind 的分支代码
（install/index/header 全部继续认 `workload-tooling`）。**代价**：`workload-tooling` 现在语义略宽（含
「payload」子形态），由 `[contents.*]` 段区分——可接受。

### D6a: [contents.payload] 键名

**决定（DRAFT 提议，待 User 确认）：** manifest 段
```toml
[contents.payload]
payload = "z42.testagent.zpkg"
host    = ["*"]
```
`payload` = tar 内 payload zpkg 的文件名（install 后置于 `workloads/test/` 供 test harness 定位）。
`host=["*"]` = 平台无关。**与 desktop 的 `[contents.platform]` 平级但键不同**，install 侧只读
`host`/`runtimes`（来自 index），manifest 的 `[contents.*]` 目前 install 不解析（仅 SDK 内部用），故
键名选择主要影响未来消费方与可读性，取直白的 `payload`。

### D6b: 无 merge 步

**问题：** desktop 有 `_releaseAssembleDesktop` merge 4 个 per-RID piece。test 要不要类似 merge？
**决定：** **不要**。test workload 无 per-RID piece（平台无关单 zpkg），`_buildTestWorkload` 一步产出的
`z42-workload-<v>-test/` 直接 tar 即最终发布产物。CI 的 publish 步对 test 跳过「assemble single
workload」merge，直接用 build 产物。→ index 校验的 archive 名 `z42-workload-<label>-test.tar.gz`。

### D6c: 描述泛化措辞

`launcher_cli.z42`：
- `:304`/`:310` 「install / list / uninstall platform workloads」→「install / list / uninstall
  workloads (platform tooling or capabilities like test)」
- `:312` 「install a platform workload (tooling + runtime)」→「install a workload (platform tooling +
  runtime, or a capability like test)」
- `:313` positional `<ios|android|wasm|desktop>` →`<ios|android|wasm|desktop|test>`
- `:250` 「also list installed platform workloads」→「also list installed workloads」
`launcher_workload.z42:3` 「A workload = a platform's tooling」→「A workload = a platform's tooling,
or a platform-agnostic capability (e.g. test)」。

## Implementation Notes

- **`_buildTestWorkload(root, version, profile)`**（`xtask_package_test.z42`，仿
  `xtask_package_desktop.z42:_buildDesktopWorkload`）：① 用注入编译器 build
  `workload/test/agent/z42.testagent.z42.toml --release` → `z42.testagent.zpkg`（可复用
  `_ensureTestAgent` 等价构建，但走 package 侧的 build，产物落 `z42-workload-<v>-test/`）；② 写 payload
  manifest（`_workloadPkgHeader` 写头 + 追加 `[contents.payload]`）；③ 返回 workload dir。tar 由
  release/CI 步做（与 desktop per-RID 一致）。
- **`packages.toml` 注册**：加 `[package.workload-test]`：`artifact="z42-workload-{version}-test"`、
  `manifest="workload-test"`、无 `include`（无 apphost stub）。
- **`_dispatchPackage`（`xtask_cli.z42`）**：`package workload test [version]` → `_buildTestWorkload`；
  保持现有 `package workload <label>`（desktop merge）/ `package workload`（desktop build）语义不破坏
  —— 加一个 `test` 子形态分派（design：`package workload test` 显式，不与 desktop 的 label-merge 冲突）。
- **`_releaseGenIndex`**：workload 名单 `["ios","android","wasm","desktop"]` → 加 `"test"`；SHA256SUMS
  校验循环含 `z42-workload-<label>-test.tar.gz`；`_jWorkload("test", host=["*"], runtimes=[])`。
- **CI**：`release.yml` package job 的 host RID（如 macos-arm64 primary）跑 `package workload test`
  一次（平台无关，任一 host 产同一 zpkg —— 选 primary host 建，避免 4 份重复）；archive
  `z42-workload-<v>-test.tar.gz`；publish job 的 SHA256SUMS + index 自动含（glob `z42-*` 已覆盖上传）。
  `ci.yml` publish-nightly 同构 + release notes 的 Workloads 表加 test 行。

## Testing Strategy

- **本地可验**：`xtask package workload test nightly` 产出 `z42-workload-nightly-test/` 布局正确
  （含 `z42.testagent.zpkg` + manifest，manifest 段 `[contents.payload]` 正确）——可本地跑（需种子编
  agent）。描述泛化：`z42 workload install --help` / `z42 workload list --help` 文案含 test（reuse 种子
  编 launcher 验证）。
- **本地不可全验**：完整 release/publish/index 上传链（tar + SHA256SUMS + release-index.json +
  gh release）→ 交 CI（release.yml / ci.yml publish-nightly）。
- **单测**：payload manifest 生成 + `_releaseGenIndex` test 条目 JSON 形态（`{archive,sha256,host,
  runtimes}`）解析/断言。
- **GREEN gate**：`xtask test` 完整（改动主要在 scripts/package + launcher + CI + docs，无 stdlib/编译器
  语义变更 → 关注 compiler 自举字节不动 + 现有 package 相关测试不回归）。
