# Tasks: `[native]` 声明面 + build-hook 产 native + 传递复制

> 状态：🟢 已完成 | 创建：2026-09-01 | 完成：2026-09-01 | 单 PR（阶段 0 核实）
> GREEN：`xtask test` 全绿（e2e 275/275 + cross-zpkg + stdlib + z42c 自举 gen1==gen2 3/3 + vscode-syntax）。

## 进度概览
- [x] 阶段 0: 两-nightly 核查 → 单 PR（种子域不消费新 API）
- [x] 阶段 1: 声明面（z42.project 模型 + 解析）—— 3 native 单测 PASS
- [x] 阶段 2: 产出相位（z42.build `ProvideNative` + z42.repl hook）
- [x] 阶段 3: 传递复制（builder_publish 填实）—— 端到端验证 native 入 payload
- [x] 阶段 4: 删 xtask 特殊处理（4.4 改为保留排除，见备注）
- [x] 阶段 5: 文档同步（native-libraries/extensions + 3 README）；GREEN gate 运行中

## 阶段 0: 两-nightly 核查
- [ ] 0.1 `grep -rn "\.Natives\|ProvideNative\|_parseNative" src/compiler scripts/*.z42` —— 确认无种子域
  （xtask源/z42c源）消费新 API。命中 → 改走 support/use 两 PR；未命中 → 单 PR 继续。

## 阶段 1: 声明面（z42.project）
- [ ] 1.1 NEW `src/libraries/z42.project/src/NativeSpec.z42` —— `sealed class NativeSpec { string Name; }`（受限子集）
- [ ] 1.2 MODIFY `ProjectManifest.z42` —— 加 `NativeSpec[] Natives; int NativeCount;`（构造后填字段，仿 Analyzers）
- [ ] 1.3 MODIFY `ManifestLoader.z42` —— `_parseNative(root, pm)`：读 `[native]` 子表键为 name，按名稳定序，填 pm
- [ ] 1.4 单测 `z42.project/tests/manifest_native.z42` —— 单/多/缺省三场景（spec R1）

## 阶段 2: 产出相位
- [ ] 2.1 MODIFY `src/libraries/z42.build/src/BuildHooks.z42` —— 加 `virtual void ProvideNative(IPipelineContext ctx) { }`
- [ ] 2.2 平台后缀派生工具（rid 族 → `lib<name>.<suffix>`）—— 放 NativeSpec 或 builder helper；单测各 rid 族
- [ ] 2.3 MODIFY `z42.repl.z42.toml` —— 加 `[build] hooks = "hooks"` + `[native.z42_repl]`
- [ ] 2.4 NEW `repl/hooks/hooks.z42` —— `ProjectHooks : BuildHooks`，override `ProvideNative`：
      `ctx.Exec("cargo", ["build","-p","z42-repl","--release","--manifest-path", <runtime Cargo.toml>, (cross: --target)])`
      → 拷 `libz42_repl.<suf>`（从 warm cargoOut）到 `ctx.Dirs.Dist/<rid>/` → `ctx.AddOutput("native", …)`；失败 Warn 不抛
- [ ] 2.5 验证：单独 `z42 publish` z42.repl → `dist/<rid>/libz42_repl.*` 存在（spec R4）

## 阶段 3: 传递复制
- [ ] 3.1 MODIFY `builder_hooks.z42` —— 加"按 dep toml 载入 hooks 并跑其 `ProvideNative`（dep-scoped ctx）"辅助
- [ ] 3.2 MODIFY `builder_publish.z42` —— 填 `_pubBundleProjectNativeDeps`：走 path-dep 闭包 → 对声明 `[native]`
      的 dep 跑 ProvideNative → 取目标 rid `<dep-dist>/<rid>/lib<name>.<suf>` → 平铺进 payloadDir（spec R3）
- [ ] 3.3 e2e fixture：带 `[native]` + 假 ProvideNative 的 lib + path-dep 它的 exe → publish 断言 native 平铺 + rid 正确

## 阶段 4: 删 xtask 特殊处理
- [ ] 4.1 MODIFY `xtask_stage_components.z42` —— 删 `_pkgStageReplCdylib` 定义 + `_pkgBuildAndStageRuntime` 的 `cargo build -p z42-repl`
- [ ] 4.2 MODIFY `xtask_package_desktop.z42` —— 删 `_pkgStageReplCdylib(cargoOut, z42iStage)`（L175）
- [ ] 4.3 MODIFY `xtask_test_stage_components.z42` —— 删 `_pkgStageReplCdylib` 调用（L45）
- [ ] 4.4 MODIFY `xtask_package.z42` —— 删 `_copyNativeLibs` 的 repl 排除分支（L254）
- [ ] 4.5 `grep -rn "_pkgStageReplCdylib\|cargo build.*z42-repl" scripts/ docs/` 清零

## 阶段 5: 测试 + 文档同步
- [ ] 5.1 `cargo build --release`（z42vm，不应受影响）
- [ ] 5.2 `xtask build toolchain` → `programs/z42i/libz42_repl.*` 存在 + z42i REPL 冒烟 `1+1=2`（替代原特殊路径）
- [ ] 5.3 `xtask test` 全 stage 绿 + z42c 自举 gen1==gen2 不动点
- [ ] 5.4 spec scenarios 逐条覆盖确认
- [ ] 5.5 文档：`native-libraries.md §3`（Deferred→实况）+ z42.project/z42.build/repl 三 README
- [ ] 5.6 packaging：`xtask package sdk` + `xtask test dist`（本地）/ 冷路径交 CI

## 备注
- rid staging 分目录、payload 平铺（design D5）；运行期 `resolve_native_beside` 不变。
- cargo 与 VM 同源：裸 cargo + `_cargoTargetFor` 规则（design D3）。
- **阶段 0 结果**：`grep` 种子域（src/compiler + scripts/*.z42）无 `.Natives`/`ProvideNative`/`_parseNative`/`NativeSpec` 消费 → **单 PR**。
- **4.4 偏差（事实校正）**：`_copyNativeLibs` 的 repl 排除**保留不删**。原因：hook 用 host-reuse 无 `--target`，`cargo build -p z42-repl` 落进**共享** cargoOut（`artifacts/build/runtime/release/`）→ libz42_repl 会出现在 cargoOut → 排除防其漏进 `<sdk>/native/` 触发 eager scanner WARN。只更新注释。
- **3.3 e2e**：由**真实 dogfood** 覆盖（z42.repl 携带 native → z42.interactive 消费 → `xtask build toolchain` 端到端验证 `publish/programs/z42i/libz42_repl.dylib` 落位），强于合成 fixture，不另写。
- **端到端已验证**：`xtask build toolchain`（走 z42b publish）→ hook `cargo build -p z42-repl` → `distDir/<rid>/libz42_repl.dylib` → 平铺 `distDir/libz42_repl.dylib` → `_pubCopyDistDeps` → `publish/programs/z42i/libz42_repl.dylib`。
- **Windows 统一命名**（User 裁决）：`<prefix><name><suffix>`，`<prefix>`=DLL_PREFIX（Windows 空）；`resolve_native_beside` 不变（本就用 DLL_PREFIX），无需改 runtime。
- 待办：GREEN gate 全绿确认 → commit → PR。
