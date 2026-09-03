# Tasks: 类初始化按需触发（defer-class-initialization）

> 状态：🔴 待 User 审批（阶段 6.5 gate 未过）| 创建：2026-09-04 | 类型：vm（完整流程）

## 阶段 7：实施

- [ ] 1.1 `LazyLoader`：新增 `pending_static_inits: Vec<String>`，`load_zpkg_file` 注册函数时压入
- [ ] 1.2 `LazyLoader`：新增 `init_state: FxHashMap<String, InitState>`（`Running(ThreadId)` / `Done`）
- [ ] 1.3 `LazyLoader::resolve_type`：原生类型关键字名守卫（P4），不进 Fallback-B
- [ ] 2.1 `VmContext::run_pending_static_inits()`：锁外排空 + 重入/并发状态机
- [ ] 2.2 `try_lookup_function` / `try_lookup_type` 两处查找后调用排空（T1/T2）
- [ ] 2.3 `static_fields_clear()` 同步清空 `init_state`
- [ ] 3.1 `metadata::resolver`：解析 `StaticGet`/`StaticSet` 名字时，所属类不在 registry 则入队（T3）
- [ ] 3.2 模块执行前排空 T3 队列（`try_lookup_type` 逐个触发）
- [ ] 4.1 `interp::init_static_fields`：去掉 `force_load_all_declared`，只跑主模块 + 已加载包
- [ ] 4.2 `jit/mod.rs` 同步（镜像逻辑）
- [ ] 5.1 `app::build_declared_candidates`：单段根命名空间不参与候选路由（P3）
- [ ] 5.2 **必查**：确认无 stdlib 包「只声明根命名空间、无更具体命名空间」（design.md P3 风险项）

## 阶段 8：验证

- [ ] 6.1 `cargo test --lib` 全绿
- [ ] 6.2 `cargo check --target wasm32-unknown-unknown --no-default-features --lib` 0 error
- [ ] 6.3 `xtask test` GREEN（重点：cross-zpkg `generic_field_carry` / `static_field_access`）
- [ ] 6.4 **REPL 专项**（design.md「REPL 影响」四项）：`xtask build toolchain` 后
      - [ ] 单表达式 `z42i -c '1 + 2'`
      - [ ] 多轮会话：引用新包的表达式
      - [ ] 多轮会话：读取未加载包的静态字段
      - [ ] 多轮会话：第 N 轮改静态值 → 第 N+k 轮触发新包加载 → 值不被重置
- [ ] 6.5 循环初始化器用例（包 A ↔ 包 B 互引静态字段）
- [ ] 6.6 并发用例：两线程同时首次触达同一包
- [ ] 7.1 A/B 对比数据（同机 hyperfine ≥ 50 runs + peak RSS）：hello / 用 Regex 的程序 / z42c 编译
- [ ] 7.2 确认 z42c 自编译产物**字节相同**

## 阶段 9：归档

- [ ] 8.1 类初始化时机上浮 `docs/book/src/runtime/`
- [ ] 8.2 `docs/design/runtime/vm-architecture.md` 同步
- [ ] 8.3 归档到 `docs/spec/archive/`
