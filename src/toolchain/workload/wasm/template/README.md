# wasm / template

工程脚手架模板 —— `export` 时由 appbuilder（`WasmWorkload`）渲染进用户工程，
链已 bed 的 `z42vm.wasm` + wasm-bindgen glue + `app.zpkg`。

待办（模板文件尚未入库，目标内容）：
- `index.html.tmpl` —— 由 `[platform.wasm]`（title）渲染
- `index.js.tmpl` —— 经 JS ZpkgResolver 回调加载 `app.zpkg`
- `package.json.tmpl` —— name / version

> 与 `platform/`（编成 npm pkg 的 TS + wasm-bindgen 源）区分。
