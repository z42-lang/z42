# Tasks: fix-crosspkg-static-call-cctor

> 状态：🟢 已完成（User 2026-09-15 确认）｜ 创建：2026-09-15 ｜ 叠在 #650（`fix-ctor-init-silent-bugs`）之上

- [x] 1.1 修前复现固化：新夹具 `static_ctor_crosspkg_static_call`，修前 VM interp/jit 均红
- [x] 2.1 `cctors` → `Arc<CctorRegistry>`；加载器经 `set_cctor_registry` 持句柄（唯一生产安装点 `install_lazy_loader_with_deps`；未改 `LazyLoader::new` 签名）
- [x] 2.2 加载器 `insert_type` 单一入口，两条加载路径改走它，入口内登记
- [x] 2.3 删 `try_lookup_type` 两处登记；更新 `register_cctor_of` / `CctorRegistry` 注释（登记点 + 锁顺序）
- [x] 2.4 **（实施中发现，D5）** interp / JIT 静态调用屏障挪到解析之后
- [x] 3.1 Rust 单测 `inserting_a_loaded_type_registers_its_static_ctor`
- [x] 3.2 恢复 #643 两个 cross-zpkg 夹具的静态 ctor 写法（修前红：`level=0`）；新增守卫夹具 `static_ctor_crosspkg_field_first`；README 登记
- [x] 4.1 文档：book `static-constructors.md` 删已知缺陷段；`runtime/static-ctor-init.md` 加「登记点必须早于第一次使用」一节（含时序图 + 屏障位置）
- [x] 5.1 两后端手验；全量 `cargo test`（1406 passed）；全量 `xtask test` GREEN（`GREEN_EXIT=0`，基于 `df74a7b36` = #650）；`xtask test stdlib --mode jit`（3267 passed）
