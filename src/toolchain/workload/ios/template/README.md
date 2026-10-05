# ios / template

工程脚手架模板 —— `export` 时由 appbuilder（`iOSWorkload`）经 `ctx.Template(name)` +
`ctx.RenderTemplate(...)` 渲染进用户工程，包住已 bed 的 `Z42VM.xcframework` + `app.zpkg`。

待办（模板文件尚未入库，目标内容）：
- `Info.plist.tmpl` —— 由 `[platform.ios]`（bundle_id / display_name / min_ios / device_families）渲染
- `App.entitlements.tmpl` —— app 能力
- `main.swift.tmpl` —— 入口 stub（调 `z42_run_app`）
- `MyApp.xcodeproj` / `Package.swift` 骨架

> 与 `platform/`（编成 xcframework 的原生绑定源）区分：template 渲染进用户工程；
> platform 编成 runtime pack。
