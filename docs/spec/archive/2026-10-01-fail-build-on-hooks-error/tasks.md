# Tasks: 声明了 `[build] hooks` 却失败 ⇒ 构建失败

> 状态：🟢 已完成（User 2026-10-01 裁定「编译失败要构建失败」） | 创建：2026-10-01

## 背景
`_loadProjectHooks` 失败（目录缺失 / 编译错 / 找不到 `Build.ProjectHooks` / 类型不符 / 加载异常）只打诊断、
返回 null；四个调用方都把 null 当成「无 hook」继续走，构建退 0：
- dev 构建 `_orchestrate`：少跑了 hook；
- publish apphost 路：悄悄落回 `Z42_APPHOST_TEMPLATE`；
- publish `--self-contained` 路：hook 产出缺失，后面才以「embed apphost not available」之类的误导性信息失败；
- publish native 依赖路：`ProvideNative` 失败返回 0 ⇒ 应用缺着 native 库出包。
publish 三条路还把 hook **运行时**异常（`BeforeAssets` / `ProvideNative`）也吞成了降级。

## 任务
- [x] 1 修复前红：fixture `src/tests/z42b/build-hooks-broken/`（hook 源故意类型错误 + `[platform.desktop] apphost`）
      + smoke `_smokeBuildHooksBroken`（`z42b build` / `z42b publish` 都必须非 0、输出含 hooks 编译诊断、
      publish 不得走到部署行）。修复前：build 退 0；publish 回落模板后部署，最后才因模板不是真 stub 失败。
- [x] 2 `builder.z42`：`h == null` ⇒ `return 1`
- [x] 3 `builder_publish.z42`：`_pubHookApphostStub` / `_pubRunHooks` 失败返回 null（"" / 空数组仍表示「无 hooks」），
      调用方判失败；`_pubRunDepProvideNative` 失败返回 1（调用方已透传 rc）
- [x] 4 保留：hook 成功运行但未登记 `apphost-stub` ⇒ 落回 workload apphost（合法配置，不是失败）
- [x] 5 文档：`docs/internals/src/toolchain/z42b.md` hooks 一节；`builder_hooks.z42` 契约注释
- [x] 6 全量 `xtask test` GREEN
