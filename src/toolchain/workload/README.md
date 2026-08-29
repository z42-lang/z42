# toolchain/workload — 平台相关能力束（按需下载）

## 职责

承载**一切平台相关**的应用工程能力：把 runtime 产的平台无关 `app.zpkg` + 原始库，包装成各平台可发布/可导出的工程与产物。按 dotnet workload 模型，**按需 `z42 workload install <plat>`** 下载。

> **也含一个平台无关的能力 workload**（2026-08-29 起）：`workload/test/`（`z42 workload install test`）——跑测试流程时按需下载的**共享**件（on-device test-agent，平台无关一份字节码）。它不随某平台下载、也不进 SDK 恒在核心；平台专属的嵌入 host 壳仍住各 `workload/<plat>/`。见下「目标结构」。

立柱（见 [platform-export-lifecycle.md](../../../docs/design/toolchain/platform-export-lifecycle.md)）：**`z42 build` 一次产平台无关 `app.zpkg`，零 workload；`export`/`publish`/on-platform `test` 才分叉并门控对应平台 workload。**

与 `runtime/` 的区别：runtime = 平台无关核心 + **嵌入 API**（VM + Tier1 C ABI + **Tier2 host-api** + 头 + per-RID 原始库）；本模块 = 平台相关工程化（appbuilder 发布管线 + template 脚手架 + tests 契约 + platform 原生绑定 Tier3）。
> host-api（Tier2 人因 Rust）原在本模块，**已决定随 Tier1 C ABI 内聚到 `runtime/`**（User 决策 2026-06-18，落实于 B 阶段）。
与 `launcher/`（SDK）的区别：launcher = `z42` CLI core（install/build/run...），引导关键、baked-in；本模块 = 平台命令（publish/export/工程生成），目录发现、按需装。

不做：VM 执行引擎（归 `runtime/`）；CLI core（归 `launcher/`）；SDK installer / 应用打包基础设施（归 `packager/`，另议）。

## 目标结构（平台优先，2026-06-18 定 —— 取代旧「关注点优先」方案）

每个平台一个独立目录直接挂 `workload/` 下（**去掉 `platforms/` 中间层**）；平台内按关注点分子目录：

```
workload/<plat>/          # ios / android / desktop / wasm
├── appbuilder/   # z42 workload handler（: WorkloadBase）—— 发布管线的平台实现
├── template/     # 工程脚手架（export 渲染进用户工程，包住 runtime pack + app.zpkg）
├── tests/        # R1–R7 嵌入契约测试（dogfood）
└── platform/     # 原生绑定 Tier3（Swift / Kotlin / TS + rust → 编成 runtime pack；原 facade）
```

> 四**平台** workload：`desktop`（仅 publish/export，复用宿主 runtime，**无 `platform/`**）/ `ios` / `android` / `wasm`（含 target runtime pack）。分发模型见 [runtime-workload-distribution.md](../../../docs/design/toolchain/runtime-workload-distribution.md)。
>
> 另有一个**非平台的能力 workload**（不套上面平台模板）：
>
> ```
> workload/test/            # 「测试运行」能力 workload（z42 workload install test；平台无关）
> └── agent/                # on-device test-agent（一份字节码全平台共享；见 test/README.md）
> ```
>
> 它是 **payload-only 形状**（只有 agent zpkg、无 per-RID runtime pack、`host:["*"]`）——install CLI
> 无需改（名 manifest 驱动、通配 host 已支持），打包/发布落地为后续阶段（change
> `unify-test-pipeline-z42b` design D6）。
>
> 旧「关注点优先」方案（`host-api/` `facades/` `templates/` `apphost/` `conformance/` 顶层）**作废**。host-api（Tier2）→ `runtime/`；facade → 各平台 `platform/`；conformance → 各平台 `tests/`。
>
> **迁移状态（✅ 完成）**：`platforms/` 中间层已去除——四平台目录（`ios` / `android` / `desktop` / `wasm`）直接挂 `workload/` 下，各含 `appbuilder/`·`platform/`·`template/`·`tests/`（`desktop` 无 `platform/`）；`host-api`（Tier2）已迁入 `runtime/crates/z42-host`；`facade → platform`、`pipeline → appbuilder` 重命名到位。归档见 change `consolidate-platform-into-workload` / `build-workload-subsystem`。

## 依赖关系

- 依赖 `runtime/`（原始库 + C ABI 头）；被 `launcher` 的目录发现注册为平台命令。
- 迁移路线见 `docs/spec/changes/consolidate-platform-into-workload/`。
