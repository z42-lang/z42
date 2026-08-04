# Proposal: add-offline-symbolication

## Why
z42 已有 `split-debug-symbols`（2026-05-11）：`--release` 从主 zpkg 剥离内联行表（DBUG 段），
把行表放进旁挂 `.zsym` sidecar（含 MDBG 行表 + BLID build_id）；运行时 loader 探测**相邻**
`.zsym`、build_id 匹配则 merge 回行表 → 栈跟踪出 `file:line:col`。

但这套只支持"**sidecar 与主包同目录、运行时自动合并**"。真实发行场景需要的是：
**部署包不带 `.zsym`（最小、不泄源码结构），把 `.zsym` 单独归档；线上崩溃后，用崩溃日志 +
归档 `.zsym` 离线还原 `file:line:col`。** 现状做不到——因为 `.zsym` 不在旁时，剥离后的栈帧
**只有函数名、没有可反查的稳定位置 key**（`format_stack_trace` 在 line==0 时省略位置），
崩溃日志把位置信息丢了。

## What Changes（叠加在现有 `.zsym` 基础设施上的小增量，非重造）
1. **栈帧 code offset**：当某帧无内联/合并行号（release 剥离且 `.zsym` 未随部署）时，
   `format_stack_trace` 输出 `at <func> +0x<offset>`（offset = 函数内**线性化指令位置**）。
   有行号时（debug，或 `.zsym` 在旁被 merge）行为**完全不变**，仍出 `file:line:col`。
2. **`z42d symbolicate <trace> --syms <a.zsym>`** 离线工具：把日志里的 `at <func> +0x<off>` +
   归档 `.zsym`（MDBG 行表 + 同一 offset 换算）→ 重写为 `at <func> (file:line:col)`。
   非匹配行透传；`.zsym` 缺该 func / build_id 不符 → 保留原行 + stderr 警告（不静默、不崩）。
3. **激活 z42d muxer**：z42d 现为 scaffold（未登记进 workspace/xtask/CI），本 change 把它登记进
   构建 + 加 `symbolicate` 子命令（其余 fmt/doc/dbg/prof/lint 仍 planned）。
4. **行号生成开关**（用户放宽②）：默认生成行号（现状）；给显式开关关闭内联生成
   → 走现有 release 剥离路径。**大概率复用现有 `--release`/`StripSymbols`，不新增格式**（design 确认）。

## Scope（允许改动的文件）

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `src/runtime/src/exception/mod.rs` | MODIFY | `format_stack_trace`：无行号帧输出 `+0x<offset>`；`FrameSnapshot`/`VmFrame` 携带 offset |
| `src/runtime/src/interp/exec_instr.rs` | MODIFY | `update_caller_line` 兼记 offset（或由 (block,instr) 现算） |
| `src/runtime/src/interp/mod.rs` | MODIFY | throw 路径同样记 offset；offset↔(block,instr) 换算 SoT |
| `src/runtime/src/metadata/bytecode.rs` | MODIFY | offset↔(block,instr) 线性化换算工具函数（供 interp/jit/format 共用） |
| `src/runtime/src/jit/translate.rs` | MODIFY | JIT 侧 set_exception/行解析按同规则记 offset |
| `src/toolchain/devtools/core/devtools_cli.z42` | MODIFY | 注册 `symbolicate` 子命令 + dispatch |
| `src/toolchain/devtools/core/symbolicate.z42` | NEW | 离线还原引擎：读 trace + `.zsym` MDBG + offset→line |
| `src/libraries/z42.ir/src/ZpkgReader.z42` | MODIFY | （若缺）z42 侧 `.zsym` MDBG 读取，供 z42d 用 |
| `scripts/xtask*.z42`（devtools 登记处） | MODIFY | 把 z42.devtools 纳入 workspace/build |
| `docs/design/runtime/zbc.md` / `zpkg.md` | MODIFY | DBUG/sidecar 节补 offset 栈格式 + 离线还原流程 |
| `docs/book/src/...`（栈跟踪/调试符号机制页） | MODIFY | 机制与流程（含 mermaid） |
| `src/toolchain/devtools/README.md` | MODIFY | symbolicate 子命令 + 六段同步 |
| `src/tests/.../symbolicate*/` 或 `src/runtime/src/*_tests.rs` | NEW | offset 格式 golden + symbolicate 往返测试 |

**只读引用**：`src/runtime/src/metadata/zbc_reader.rs`（sidecar/MDBG 解析参照）、
`src/runtime/src/metadata/loader.rs`（现自动合并逻辑）、`src/libraries/z42.ir/src/ZpkgWriter.z42`（.zsym 产出）。

## Out of Scope
- 重造 `.zsym` 格式 / 全新 full/split/none 三档体系（已被现有 split-debug-symbols 覆盖）
- DAP debugger / 断点单步（0.8.x；z42d dbg 仍 planned）
- z42d 其余子命令（fmt/doc/prof/lint 保持 planned）

## Open Questions
- [ ] D1：offset 精确定义（线性化指令序号）确认——运行期 format 与 z42d 必须共用同一换算
- [ ] D2：是否需 bump zbc/zpkg 格式？倾向**否**（offset 派生、.zsym 已含 MDBG）→ 则本 change 为 `vm`+`toolchain` 类，非 `ir`
- [ ] D3：行号开关是否需独立 toml 旋钮，还是纯复用 `--release`/`StripSymbols`
- [ ] D4：z42 侧是否已有 `.zsym` MDBG 读取（ZpkgReader），还是需新增
