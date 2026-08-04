# Tasks: add-offline-symbolication

> 状态：🟡 待确认（6.5 gate）| 创建：2026-08-04
> 类型：vm（栈格式行为）+ toolchain（z42d）。走完整流程。**不 bump 格式**（D2）。

## 进度概览
- [x] 阶段 1：offset 换算 SoT（metadata）
- [ ] 阶段 2：运行期栈格式（interp + jit + exception）
- [ ] 阶段 3：z42 侧 .zsym MDBG reader（z42.ir）
- [ ] 阶段 4：z42d 激活 + symbolicate 引擎
- [ ] 阶段 5：测试 + 文档

## 阶段 1：offset 换算 SoT
- [x] 1.1 `metadata/bytecode.rs`：`Function::linear_offset` + `offset_to_site`（D1 公式，含终结子槽）
- [x] 1.2 单测：`code_offset_roundtrip` 往返一致（Rust，pass）

## 阶段 2：运行期栈格式
- [ ] 2.1 `exception/mod.rs`：`VmFrame`/`FrameSnapshot` 加 `offset`；`format_stack_trace` line==0&&offset → `+0x<off>`
- [ ] 2.2 `interp/exec_instr.rs` + `interp/mod.rs`：update_caller_line / throw 路径记 offset（复用 1.1）
- [ ] 2.3 `jit/translate.rs`：set_exception/行解析两处记 offset（同规则）
- [ ] 2.4 golden：release 剥离 + 无相邻 .zsym → 栈出 `+0x<off>`；回归 debug/sidecar-在旁 不变

## 阶段 3：z42 侧 .zsym MDBG reader
- [ ] 3.1 `z42.ir`：SymOnly sidecar reader（META+STRS+MDBG+BLID）→ `{build_id, fqn→IrLineEntry[]}`
      （对照 Rust `parse_zpkg_sidecar`）
- [ ] 3.2 z42 侧 offset↔(block,instr) 镜像换算（与 1.1 字节一致）
- [ ] 3.3 单测：z42 reader 读回 + 换算一致

## 阶段 4：z42d 激活 + symbolicate
- [ ] 4.1 `devtools_cli.z42`：注册 `symbolicate`（positional trace + `--syms`）+ dispatch
- [ ] 4.2 `symbolicate.z42`（NEW）：扫 trace → 匹配 `at F +0x<hex>` → 查 sidecar → 还原 → 重写；缺失透传+警告
- [ ] 4.3 把 `z42.devtools` 登记进 workspace/xtask 构建（现 PARKED；过自举纪律 API 面检查）

## 阶段 5：测试 + 文档
- [ ] 5.1 symbolicate 往返 e2e：release 崩溃栈 + 归档 .zsym → 还原 == debug 档栈位置
- [ ] 5.2 缺符号/build_id 不符 → 尽力而为（保留+警告）测试
- [ ] 5.3 `xtask test` 完整 GREEN（含 z42d 激活后的构建）
- [ ] 5.4 文档：docs/design/runtime/{zbc,zpkg}.md + book 机制页（offset 栈格式 + 离线流程 mermaid）+ devtools README 六段

## 备注
- 不 bump 格式（D2）；offset 派生自 (block,instr)，.zsym MDBG 已含数据。
- 自举纪律：z42d 源新用 stdlib API 须已随 nightly 发布（bootstrap-seed.md 轴②）。
