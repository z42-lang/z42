# Tasks: fix-host-static-init

> 状态：🟢 已完成 | 创建：2026-09-14 | 完成：2026-09-14

**变更说明：** 宿主 API（`z42_host_load_zbc` → `z42_host_invoke`）执行合并包的 `__static_init__`，并与 `app::run` 共用合并后的启动步骤。
**原因：** 宿主路径手抄了一份旧版启动序列，从不跑静态字段初始化器 ⇒ 带初始化器的静态字段读出类型默认值（`OSKind.Wasm` 为 0，`Platform.IsWasm()` 恒假），所有嵌入（C ABI / iOS / Android / wasm）都受影响；同时缺 cctor 登记、lazy loader 种子、`available!` 折叠、FuncRef 槽 / token 预解析，且丢弃了依赖包的 impl pairs。wasm-browser 修复（#644）期间发现。
**文档影响：** `docs/book/src/runtime/static-ctor-init.md`（新增「两条启动路径共用一份启动步骤」）、`src/runtime/README.md`（核心文件表加 `app.rs` / `boot.rs`）。

- [x] 1.1 复现：`host_tests::invoke_sees_initialized_static_fields` + fixture `embedding_hello` 加 `CoreStatic` / `UserStatic`，修前 native 上 `OSKind.Wasm` 读出 0
- [x] 1.2 新增 `src/runtime/src/boot.rs`：`boot_context`（availability 折叠 → with_module → cctor 登记 → lazy loader + 种子）、`prepare_execution`（FuncRef 槽 + resolve_module）
- [x] 1.3 `app.rs` / `vm.rs` 改调 `boot`（纯搬移，行为不变）
- [x] 1.4 `host/ops.rs`：收集 impl pairs、改用 `boot_context` + `prepare_execution`；`invoke_impl` 首次调用在 sink 守卫内跑一次 `init_static_fields`（`OnceLock`，失败粘滞）+ 每次调用后取 `take_static_init_error`
- [x] 1.5 `host/state.rs`：`HostModule` 去掉独立 `module` 字段（改由 ctx 持有），加 `static_init`；`host/mod.rs` 跟进
- [x] 1.6 文档同步（见上）
- [x] 1.7 验证：`cargo test --lib --features z42-test-fixtures` 全绿；wasm Node 实测 `IsWasm()`；`xtask test` GREEN；CI
