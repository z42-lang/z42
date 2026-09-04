# Tasks: 类初始化按需触发（defer-class-initialization）

> 状态：🟢 实施+验证完成（含并发竞态修复），待合并 | User 过 gate：2026-09-04| 创建：2026-09-04 | 类型：vm（完整流程）

## 阶段 7：实施（含实施中发现的 5 个主干潜伏 bug，见 design.md）

- [x] 1.1 `LazyLoader`：新增 `pending_static_inits: Vec<String>`，`load_zpkg_file` 注册函数时压入
- [x] 1.2 `LazyLoader`：新增 `init_state: FxHashMap<String, InitState>`（`Running(ThreadId)` / `Done`）
- [x] 1.3 `LazyLoader::resolve_type`：原生类型关键字名守卫（P4），不进 Fallback-B
- [x] 2.1 `VmContext::run_pending_static_inits()`：锁外排空 + 重入/并发状态机
- [x] 2.2 `try_lookup_function` / `try_lookup_type` 两处查找后调用排空（T1/T2）
- [x] 2.3 `static_fields_clear()` 同步清空 `init_state`
- [x] 3.1 `metadata::resolver`：解析 `StaticGet`/`StaticSet` 名字时，所属类不在 registry 则入队（T3）
- [x] 3.2 排空点：`init_static_fields` 开头（主模块，先于自身初始化器）+ `exec_function_body` 首次解析后（每个懒加载函数）
- [x] 4.1 `interp::init_static_fields`：去掉 `force_load_all_declared`，只跑主模块 + 已加载包
- [x] 4.2 `jit/mod.rs` 同步（镜像逻辑）
- [ ] 5.1 ~~`app::build_declared_candidates` 根命名空间不参与路由（P3）~~ —— **实施中改为不做**：
      候选集不再被 force-load，多余候选只剩一次 `ZpkgCandidate::build` 的开销；
      而收窄候选集会与「候选集需为传递闭包」直接冲突（见 design.md 新增章节），风险大于收益。
      单独立项：`ZpkgCandidate::build` 目前为读 NSPC 段把整个 zpkg 文件读进内存。
- [x] 5.3 **新增（实施中发现）**：`resolve_function`/`resolve_type` 的 Fallback-B 扫到**不动点**
      —— 候选集是增量的，单轮扫描会漏掉「刚成为候选」的包（root cause，见 design.md）
- [x] 5.4 **新增**：`obj_new` 合成空 `TypeDesc` 时对跨包类名 `tracing::warn!`（原为静默数据损坏）
- [x] 5.5 **新增**：`FieldGet` 错误信息补字段名

## 阶段 8：验证

- [x] 6.1 `cargo test --lib` 全绿（993 + 21 passed；新增 3 个守卫/状态机单测）
- [x] 6.2 `cargo check --target wasm32-unknown-unknown --no-default-features --lib` 0 error
- [x] 6.3 `xtask test` ✅ GREEN（283 + 14 + 2 passed，0 failed，日志 0 个 ✗）（重点：cross-zpkg `generic_field_carry` / `static_field_access`）
- [x] 6.4 **REPL 专项**（`xtask build toolchain` 出 z42i；四项输出与 main 基线**逐字一致**）（design.md「REPL 影响」四项）：`xtask build toolchain` 后
      - [x] 单表达式 `z42i -c '1 + 2'` → 3
      - [x] 多轮会话：`using Std.Regex` / `Std.Text` → Regex.Compile / StringBuilder 均正确
      - [x] 多轮会话：`BigInt.LIMB_BITS`=31 / `BigInt.BASE`=2147483648（**T3 关键验证**，不是 null）
      - [x] 多轮会话：counter=42 / keep=7 在后续新包加载后保持不变
- [x] 6.5 跨包静态初始化顺序用例 `src/tests/cross-zpkg/static_init_cross_pkg`
      （main **只读静态字段、不调 dep 任何函数** → 只有 T3 能覆盖；`Table.Count`=3 → `Derived.Doubled`=6）。
      注：**真正的「循环」初始化器在本 harness 里表达不出来**——三包 target→ext→main 是 DAG，
      包之间不能互相依赖，语言层面也不支持循环包依赖。同线程重入由单元测试覆盖状态机本身。
- [x] 6.6 并发用例 `src/tests/cross-zpkg/static_init_concurrent`（两工作线程同时首次触达同一包）。
      **抓到真竞态**（见 design.md「并发排空的收尾判据」）；修后 jit 40/40 + interp 40/40。
- [x] 7.1 A/B 对比数据（见 design.md「实测收益」：REPL 6.41×、hello 1.97×、regex 3.19×、RSS −41~47%）
- [x] 7.1b 专项用例已补（6.5 / 6.6），并在 6.6 中抓到一个真竞态（同机 hyperfine ≥ 50 runs + peak RSS）：hello / 用 Regex 的程序 / z42c 编译
- [x] 7.2 z42c 自编译产物**字节相同**（`--emit-zbc hello.z42` cmp 通过）

## 阶段 9：归档

- [ ] 8.1 类初始化时机上浮 `docs/book/src/runtime/`
- [ ] 8.2 `docs/design/runtime/vm-architecture.md` 同步
- [ ] 8.3 归档到 `docs/spec/archive/`
