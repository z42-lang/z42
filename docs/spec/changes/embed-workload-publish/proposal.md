# 提案:自包含嵌入发布 —— `z42 publish --self-contained`（workload 生成嵌入运行时 app）

> 状态:**IN PROGRESS**(desktop static 已验证)。对齐 2026-08-06。分支 `embed-workload-publish`(基于 #96)。
> 上游:[mature-embed-testhost](../mature-embed-testhost/proposal.md) 把 workload 排在测试管线之前;本 change
> 是那条"先做跨平台 app 构建"的第一步落地 —— desktop 自包含嵌入发布。

## 动机

G6(#96)建了**嵌入原语**(`z42::app::run` + `z42_host_run_app` + 四平台 shell),但只接在测试 harness 上。
`z42 publish` 现状是 **spawn apphost**(拉外部 z42vm),不嵌入。本 change 把嵌入原语接进 publish:
`z42 publish --rid <desktop> --self-contained` → **自包含 app**(链 libz42,进程内跑,无外部 z42vm),
和用户发布自己 app 同一条路径。这是"embed + workload = 跨平台 app 构建"的地基,测试管线随后建其上。

## 设计(desktop)

- **通用 embed apphost**(`workload/desktop/shell/apphost_embed.c`):解析自身 exe 目录 → 跑同目录
  `app.zpkg` 对 `./libs`,调 `z42_host_run_app`。**预构建、workload 分发**(不在 publish 时编译)。
- **`_pubDesktopSelfContained`**(`builder_publish.z42`):`--self-contained` 分支 —— 编 app.zpkg → 拷预构建
  embed apphost 为 exe → 装配 `app.zpkg` + `libs/`(stdlib + app 依赖) → 产出自包含 app 目录。
- **static / dynamic**(`[platform.desktop] link`,平台允许才可选):static 把 libz42 编进 apphost;dynamic
  另发 `libz42.<dyn>` 于 exe 旁(apphost rpath `@loader_path`)。
- **provisioning(A 方案)**:embed apphost + stdlib +(dynamic)libz42 由 **desktop workload 打包**;本仓 dev
  flow 经 env(`Z42_EMBED_APPHOST` / `Z42_EMBED_LIBS` / `Z42_EMBED_DYLIB`)供给,镜像 spawn 的
  `Z42_APPHOST_TEMPLATE`。后续接 build-hook 自动供给。

产出 app 目录:`<app>/ { <name>[.exe], app.zpkg, libs/*.zpkg [, libz42.<dyn>] }`。

## 验证

- **desktop static**:`z42b publish --self-contained` 对 hello → 自包含 app 独立运行(无 z42vm、无 env)→
  `hello, world`。✅
- **desktop dynamic**:`--link=dynamic` → app 目录含 `libz42.dylib`;**relocatable**(app 目录整体移走仍跑)。✅
  - **前提**:shipped `libz42.dylib` 的 install-name 必须是 `@rpath/libz42.dylib`(workload 构建 cdylib 时
    `-install_name @rpath/libz42.dylib`,或 `install_name_tool -id`),apphost 以 `-rpath @loader_path` 链接。
    否则链绝对构建路径、不可移。

## 后续(排入 mature-embed-testhost 计划)

- build-hook 自动供给 embed apphost + libz42(免手设 env / 免装 workload);
- 泛化到 ios/android/wasm 的 export 生成器(填 run 桩为 `z42_host_run_app`,用 #96 的 Swift/JNI/wasm shell);
- Trim 相位(只 stage 用到的 stdlib,对齐 .NET trimming);
- 测试管线(test-agent 作为一个 workload app)建于其上。
