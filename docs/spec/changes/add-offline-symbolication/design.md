# Design: add-offline-symbolication

## Architecture

```
现有 split-debug-symbols（复用，不改）                 本 change 新增
────────────────────────────────                     ──────────────
--release 剥内联 DBUG → .zsym（MDBG 行表+BLID）         运行期：.zsym 缺席帧
loader 探测相邻 .zsym → build_id 匹配 → merge 行表        format 出 `at F +0x<off>`
                                                       （有行号仍出 file:line:col）
                                                              │
                                                              ▼ 崩溃日志（带 offset）
                                                       z42d symbolicate <log> --syms a.zsym
                                                         读 .zsym MDBG（新 z42 侧 reader）
                                                         offset →(同换算)→(block,instr)→ line
                                                         重写 `+0x<off>` → `(file:line:col)`
```

## Decisions

### D1: code offset 定义（运行期 format 与 z42d 共用同一 SoT）
z42 是"块 + 块内指令"模型，无线性字节码地址。定义：
> **offset(block, instr) = Σ_{b<block}(len(block_b.instructions) + 1) + instr**（每块 +1 计入终结子）

- 单调、函数内唯一；`+0x` 十六进制打印。
- 反向 `offset → (block, instr)`：z42d 按同公式在函数块长前缀和上二分/线性定位。
- **实现 SoT**：`metadata::bytecode` 加 `fn linear_offset(func, block, instr) -> u32` +
  `fn offset_to_site(func, off) -> (u32,u32)`；interp/jit/format 与 z42d（z42 侧镜像同公式）共用。

### D2: 不 bump zbc/zpkg 格式
offset 是 (block,instr) 的**派生量**，`.zsym` 的 MDBG 段已存 (block,instr,line,col)。
运行期只在 format 时把已记的 (block,instr) 折算成 offset；z42d 用 MDBG 的 (block,instr) 反折。
→ **无新 section、无 wire 变化、不 bump minor**。change 类型 = `vm`（栈格式行为）+ `toolchain`（z42d）。

### D3: 行号生成开关
默认生成（现状）。关闭 = 现有 `--release`（剥内联 DBUG → .zsym）。本 change **不新增 toml 旋钮**——
用户放宽②"有 .zsym 默认生成、给选项可关"已被 `--release`/`StripSymbols` 覆盖。若后续要 debug 档也能
单独关，再开独立 change。（design 定：复用现有开关，不扩面。）

### D4: z42 侧 .zsym MDBG 读取（新增）
现状：z42 侧 `ZbcReader` 能读 zbc 的 DBUG 段（→ `IrLineEntry[]`），但 `ZpkgReader` **拒收 SymOnly
`.zsym`**（sidecar 是 SymOnly zpkg：META+STRS+MDBG+BLID）。z42d symbolicate 要在 z42 里读 `.zsym`
的 MDBG（per-module per-func 行表）+ BLID。
→ 在 `z42.ir` 加一个 **SymOnly sidecar reader**（复用 STRS/MDBG 解码；Rust 侧 `parse_zpkg_sidecar`
是对照实现）。返回 `{ build_id, per_func: { fqn → IrLineEntry[] } }` 供 z42d 查。

### D5: 运行期栈格式（format_stack_trace）
- `VmFrame`/`FrameSnapshot` 现存 `line/column`。新增 `offset`（Cell<u32> / u32）。
  `update_caller_line` 与 throw 路径在记 (line,col) 的同时记 offset（由当前 (block,instr) 折算，
  复用 D1 的 `linear_offset`）。
- `format_stack_trace`：`line>0` → 现状 `(file:line:col)` 不变；`line==0 && offset 有效` → `+0x<off>`；
  两者皆无 → 仅函数名（现状兜底）。
- JIT：`translate.rs` 的 set_exception/行解析两处按同规则折算 offset。

### D6: z42d symbolicate 命令
- `devtools_cli.z42` 注册 `symbolicate`（positional `trace-file` + required `--syms <a.zsym>`）。
- `symbolicate.z42`（新）：逐行扫 trace，正则/前缀匹配 `at <func> +0x<hex>` → 查 sidecar reader 的
  `per_func[func]` → `offset_to_site` → 行表二分 → `(file:line:col)`，重写该行；其余行透传。
- 缺 func / build_id 校验失败 / .zsym 读失败 → 保留原行 + stderr 警告；退出码 0（尽力而为）。
- **激活 z42d**：把 `z42.devtools` 登记进 workspace build（现 PARKED）；launcher 已有 `z42 <verb>` 透传。

## Implementation Notes
- offset 换算的 Rust 与 z42 两份实现必须字节一致（同公式）；用一个 golden 往返测试钉死。
- `.zsym` 缺席才出 offset —— debug 与"sidecar 在旁"路径零变化，现有异常栈 golden 不动。
- z42d 激活要过自举纪律：devtools 源新用的 stdlib API 必须已随 nightly 发布（见 bootstrap-seed.md 轴②）。

## Testing Strategy
- 单测：`linear_offset`/`offset_to_site` 往返（Rust）+ z42 侧镜像换算一致。
- Golden：release 剥离 + 无相邻 .zsym → 崩溃栈出 `+0x<off>`（新 e2e）。
- 往返：release 崩溃栈 + 归档 .zsym → `z42d symbolicate` 还原出的 file:line:col == 同源 debug 档栈。
- 回归：debug 档 / sidecar-在旁 档 现有异常栈 golden 不变。
- GREEN：`xtask test`（+ z42d 激活后其构建纳入 compiler/toolchain stage）。

## Deferred
- debug 档单独关行号的 toml 旋钮（D3；需要再开独立 change）。
- `.zsym` 压缩 / 多包合并符号包（future）。
- z42d dbg/prof/lint/fmt/doc（roadmap 各自分期）。
