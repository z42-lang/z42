# Tasks: VM 全惰性加载

> 状态：🔴 DRAFT（待 6.5 确认）| 创建：2026-07-24 | 类型：vm + ir（完整流程；含格式 bump）

## 进度概览
- [x] 阶段 1: 统一 jit/interp 惰性加载路径（无格式改动）——本地全绿，e2e/golden gate 以 CI 为权威
- [ ] 阶段 2: FIDX/TIDX 格式 + 函数/类型按需 parse
- [ ] 阶段 3: 验证 + 文档

## 阶段 1: 统一加载路径（无格式 bump，可独立验证不回归）
- [x] 1.1 `jit/lazy.rs`：`compile_one` 抽出 `compile_fn(&mut self, &Function)`，可编译任意外部 `&Function`（非仅 `self.module.functions[idx]`）
- [x] 1.2 惰性解析 + 每调用点 IC（**设计 B**）：`resolver.rs` `ResolvedTokens.call_jit_ic: Vec<AtomicU32>`（每 Call 点缓存 id）；`frame.rs` `LazyTable`/`LazySlot`（地址稳定 Box 槽）+ `resolve_id_by_name`（merged→func_index id，否则 `try_lookup_function` 物化 + 校 `jit_unsupported_reason` → 注册合成 id `merged_len+i`）+ `resolve_fn_by_id` 按 `merged_len` 路由（merged 槽 / lazy 槽双检编译）；`translate.rs` `call_jit_ic_ptr_at` emit IC 指针进 `jit_call`；`call.rs` `jit_call` 三级（method_id→IC→resolve_id_by_name 编译并回填 IC，miss→`cross_zpkg_via_interp`）
- [x] 1.3 `main.rs`：`is_eager` 仅 AOT；JIT 与 interp 统一走 lazy loader、不 BFS merge 闭包
- [x] 1.4 `jit/mod.rs`：`JitModule::run` static-init 二阶段镜像 interp `init_static_fields`（merged eager sorted + `collect_lazy_static_init_names` force-load）；`run_fn` interp 兜底补 `try_lookup_function`（lazy 不可翻译函数）
- [x] 1.5 `jit/helpers/value.rs`：`jit_const_str` idx 越界 merged pool → `try_lookup_string`（dep 字符串在 lazy loader 溢出池，已 remap 为绝对索引）
- [x] 1.6 `jit/lazy_load_tests.rs`（NEW）：`resolve_id_by_name` merged 映射 / name→id→entry 编译 / 合成 id 越界安全 None / 无 loader miss 为 None（4 单测）。**e2e「只加载碰到的 zpkg / lazy stdlib 走 native / static-init 正确」需 on-disk zpkg fixture → golden 套件（CI 权威）**

## 阶段 2: 函数 + 类型按需 parse（格式 bump）
- [ ] 2.1 `ZbcFormat.z42`：定义 `FIDX`/`TIDX` section + `ZbcVersion.Minor++`；`ZpkgWriterZ.Minor++`
- [ ] 2.2 `ZbcWriter.z42`：写 FUNC/TYPE 记 offset，emit FIDX（name→off/len/sig 序）+ TIDX（name→off/len）
- [ ] 2.3 `zbc_reader.rs`：读 FIDX/TIDX；拆 `read_one_func(off)` / `read_one_type(off)`；strict-pin minor 同步 + changelog
- [ ] 2.4 `bytecode.rs`：`Module` 存原始 FUNC/TYPE 字节 + 索引 + `Vec<OnceLock<Function>>` / 类型 OnceLock 槽
- [ ] 2.5 `loader.rs`：`load_artifact` 不 parse-all；`build_type_registry` 拆按需建单类型（base-first 递归 + 增量继承修复，复用 `try_fixup_inheritance`）
- [ ] 2.6 `lazy_loader.rs`：`resolve_function`/`resolve_type` 由"加载整个 zpkg"改"按索引 parse 单个"
- [ ] 2.7 无索引旧格式回落路径（bump 分阶段窗口；support 先行纪律 design D5）
- [ ] 2.8 `lazy_load_tests.rs`：只 parse 碰到的 K 函数/J 类型 / 未碰不 parse / 跨 zpkg 继承 base-first / FIDX-TIDX 往返 + 旧格式回落
- [ ] 2.9 格式 bump checklist（version-bumping.md）：两端 minor、zbc/zpkg fixture 重生、changelog、自举不动点

## 阶段 3: 验证 + 文档
- [ ] 3.1 `cargo build --release` 无错零警告
- [ ] 3.2 `xtask test` 完整 GREEN gate（e2e interp+jit / cross-zpkg / stdlib / compiler 自举 / vscode）——**输出逐字节不变 + gen1==gen2**
- [ ] 3.3 spec scenarios 逐条覆盖
- [ ] 3.4 性能佐证：`Z42_LOAD_PROFILE` 观测 parse 数骤降；CI test-host/test-vm-jit 墙钟回落（CI 权威）
- [ ] 3.5 `docs/book/src/runtime/lazy-loading.md`（NEW）+ SUMMARY；`docs/design/runtime/zbc.md`/`zpkg.md` 格式段；runtime/jit README；ACTIVE 释放

## 备注
- **阶段 1 收益有限**（z42.core 仍整个 zpkg 加载），价值在"统一架构 + 删残留 eager 合并 + 铺骨架"，且可独立验证零回归。
- **阶段 2 是真收益**，但含**格式 bump**（自举分阶段纪律 D5）+ 类型按需物化风险（D4，过复杂则先只做函数按需、类型按需拆后续）。
- 全惰性后 fork-per-case 本身变快 → 批处理 runner 非必需（Out of Scope）。
- CI 墙钟收益本地不可完整验证 → 以 CI 为权威。
