# Tasks: interp frame-lock 瘦身

- [x] DRAFT → User 确认（方案 B：发布长度原子 + skip-if-unchanged；实测天花板 3.9%）
- [x] `VmContext` 加 4 个 `AtomicUsize` 发布长度字段 + 2 构造器初始化
- [x] 加 4 个 alloc 包装方法（`stack_alloc_obj` / `stack_alloc_arr` / `struct_alloc` / `transient_alloc`），
      在 arena 锁内发布新长度
- [x] 重写 `push_frame`（无锁读原子取 base）+ `pop_frame`（skip-if-unchanged truncate + 重发布）
- [x] 13 个 alloc 调用点改走包装（interp 6 + jit helpers 2 + tests 2 + address/call 等）
- [x] alloc 漏斗 + truncate 站点全仓核对（grep：无遗漏、无绕过）
- [x] 正确性：`--dump-bound` 输出逐字节一致（sha 74d11f13…）
- [x] A/B 实测：4.757→4.617s = 2.9%；profile push/pop 掉样
- [x] `cargo test --release --lib`（926 pass）+ `--tests`（集成全过；`signal_handler_e2e` 为环境性
      pre-existing 失败，非本改动、不在 xtask gate）
- [x] `xtask test` 全 stage 绿：e2e interp 249/0 · cross-zpkg 11/0 · stdlib 全 0-fail · z42c[Test] ·
      **自举 5/5 gen1==gen2 逐字节（--workspace, C#-free）** · vscode-syntax → **GREEN**
- [x] 机制文档落 `docs/design/runtime/vm-architecture.md`
- [x] 归档 + PR
