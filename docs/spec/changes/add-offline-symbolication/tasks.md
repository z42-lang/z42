# Tasks: add-offline-symbolication

> 状态：🟡 待确认（6.5 gate）| 创建：2026-08-04
> 类型：vm（栈格式行为）+ toolchain（z42d）。走完整流程。**不 bump 格式**（D2）。

## 进度概览
- [x] 阶段 1：offset 换算 SoT（metadata）
- [x] 阶段 2：运行期栈格式 —— interp + JIT + exception 全完成，perf-neutral（同会话 mono_vcall +0.8% 噪声内）
- [ ] 阶段 3：z42 侧 .zsym MDBG reader（z42.ir）
- [ ] 阶段 4：z42d 激活 + symbolicate 引擎
- [ ] 阶段 5：测试 + 文档

## 阶段 1：offset 换算 SoT
- [x] 1.1 `metadata/bytecode.rs`：`Function::linear_offset` + `offset_to_site`（D1 公式，含终结子槽）
- [x] 1.2 单测：`code_offset_roundtrip` 往返一致（Rust，pass）

## 阶段 2：运行期栈格式
- [x] 2.1 `exception/mod.rs`：`VmFrame`/`FrameSnapshot` 加 `offset`；`format_stack_trace` line==0&&offset → `+0x<off>`
- [x] 2.2 `interp/exec_instr.rs`(update_caller_line 体) + `interp/mod.rs`(throw) 记 offset；`vm_context` 加 `update_top_frame_offset`
- [x] 2.3 `jit/translate.rs`(4 站点 bake offset) + jit/helpers(call/vcall/call_indirect/throw 加 caller_offset 参数)：JIT 帧记 offset
- [x] 2.4a 单测：format `+0x` / line 优先（Rust，pass）+ JIT 抛异常 trace sanity（debug 出 file:line）
- [ ] 2.4b golden：release 剥离 + 无相邻 .zsym → 栈出 `+0x<off>`；回归 debug/sidecar-在旁 不变（放阶段5 一起）

> **性能关键教训（本 change 实测）**：offset 记录必须 ① O(1)（打包 `(block<<16)|instr`，
> 不用 O(blocks) 前缀和——实测 dispatch ~+5%）② 折叠进 `update_top_frame_pos` 同一把锁
> （独立第二把锁实测 +8%）。定稿后同会话 mono_vcall +0.8%（噪声内），perf-neutral。

## 阶段 3：.zsym MDBG 嵌 FQN（格式变更，B）+ 读端 + zpkg minor bump
- [ ] 3.1 写端 `ZpkgWriter.BuildMdbg`（z42.ir）：MDBG per-module 改 `{ns_idx, funcCount, per-func {fqn_idx, 行表}}`
- [ ] 3.2 Rust 读端 `zbc_reader.rs::read_mdbg_section` + `ZpkgSidecarData`：读 fqn；loader merge 仍按 index（兼容）
- [ ] 3.3 z42 侧 `SidecarReader`（z42.ir，NEW）：接受 SymOnly，读 META+STRS+MDBG(含 fqn)+BLID → `{build_id, fqn→IrLineEntry[]}`
- [ ] 3.4 **zpkg minor bump**（version-bumping.md 步骤 6–9）：`ZpkgWriterZ.Minor++`、`ZPKG_VERSION_MINOR`、
      zpkg.md changelog、regen zpkg fixtures（`src/tests/zpkg-format/*` + sym-only-sidecar 基线）
- [ ] 3.5 单测：Rust `sidecar_tests` + z42 侧 reader 读回 fqn→行表；build_id 校验

## 阶段 4：z42d 激活 + symbolicate（多目录搜索，D7）
- [ ] 4.1 `devtools_cli.z42`：注册 `symbolicate`（positional trace + 可重复 `--syms <file|dir>`）+ dispatch
- [ ] 4.2 `symbolicate.z42`（NEW）：扫所有 --syms 路径（目录递归 *.zsym）→ 建全局 `FQN→(行表,build_id)` 索引；
      逐行匹配 `at <FQN> +0x<hex>` → 解包 (block,instr) → 行表二分 → 重写 file:line:col；缺失透传+warn
- [ ] 4.3 把 `z42.devtools` 登记进 workspace/xtask 构建（现 PARKED）

## 阶段 5：测试 + 文档
- [ ] 5.1 symbolicate 往返 e2e：release 剥离崩溃栈（`+0x`）+ 归档 .zsym → 还原 == debug 档栈位置
- [ ] 5.2 多目录 + 缺符号/build_id 不符 → 尽力而为（保留+warn）测试
- [ ] 5.3 `xtask test` 完整 GREEN（含 zpkg bump 后 regen + z42d 激活构建）
- [ ] 5.4 文档：docs/design/runtime/zpkg.md（MDBG+fqn 格式 + Minor changelog）+ book 机制页（offset 栈 + 离线流程 mermaid）
      + devtools README 六段（symbolicate 用法，参考 addr2line/Breakpad）

## Deferred
- 崩溃栈携带 module build_id（更强的跨构建消歧；v1 靠 FQN + 首个匹配 + warn）
- debuginfod 式符号服务器 / .zsym 压缩

## 备注
- 不 bump 格式（D2）；offset 派生自 (block,instr)，.zsym MDBG 已含数据。
- 自举纪律：z42d 源新用 stdlib API 须已随 nightly 发布（bootstrap-seed.md 轴②）。
