# Spec: offline-symbolication

## ADDED Requirements

### Requirement: 剥离档栈帧携带 code offset
当某帧无可用行号（release 剥离内联 DBUG 且无相邻 `.zsym` 合并）时，栈跟踪必须给出可离线反查的
稳定位置 key，而非丢弃位置。

#### Scenario: release 剥离 + 无相邻 .zsym
- **WHEN** 一个 `--release`（DBUG 剥离）程序在无相邻 `.zsym` 时抛出未捕获异常
- **THEN** 栈跟踪每帧输出 `at <func> +0x<offset>`（offset = 函数内线性化指令位置的十六进制）
- **AND** offset 在同一函数内单调、唯一

#### Scenario: debug 档不受影响
- **WHEN** debug 构建（内联 DBUG）抛异常
- **THEN** 栈跟踪仍输出 `at <func> (file:line:col)`，与现状字节一致（无 offset）

#### Scenario: sidecar 在旁自动合并不受影响
- **WHEN** release 程序旁有 build_id 匹配的 `.zsym`
- **THEN** loader 自动 merge 行表，栈跟踪出 `(file:line:col)`（现状行为不变）

### Requirement: offset ↔ (block,instr) 换算单一 SoT
#### Scenario: 往返一致
- **WHEN** 对任意 (block, instr) 计算 offset 再反算
- **THEN** 得回原 (block, instr)
- **AND** Rust 运行期与 z42（z42d）两侧换算对同一函数产出**完全一致**的 offset

### Requirement: z42d symbolicate 离线还原
#### Scenario: 用归档 .zsym 还原剥离栈
- **WHEN** `z42d symbolicate crash.txt --syms app.zsym`，crash.txt 含 `at F +0x2c`，app.zsym 含 F 的行表
- **THEN** 该行重写为 `at F (file:line:col)`，与同源 debug 档栈的位置一致
- **AND** 非 `at ... +0x` 行原样透传

#### Scenario: 符号缺失/不符 → 尽力而为
- **WHEN** `.zsym` 不含该 func，或 build_id 与 trace 隐含的不符，或 `.zsym` 读失败
- **THEN** 保留原 `+0x` 行不变 + 向 stderr 打印警告；命令不崩溃（退出码 0）

### Requirement: z42d 激活（symbolicate 子命令可用）
#### Scenario: 命令注册
- **WHEN** `z42d symbolicate --help` 或 `z42 symbolicate --help`
- **THEN** 打印 symbolicate 的用法（positional trace-file + `--syms`），不再是 "planned"
- **AND** fmt/doc/dbg/prof/lint 仍显示 planned（本 change 不动）

## MODIFIED Requirements

### Requirement: 栈跟踪格式（format_stack_trace）
**Before:** 帧无行号（line==0）时仅输出 `at <func>`（或 `at <func> (<file>)`），位置信息丢失。
**After:** 帧无行号但 offset 有效时输出 `at <func> +0x<offset>`；有行号仍 `(file:line:col)`；
两者皆无仍仅函数名。

## IR Mapping
无新 IR 指令、无 zbc/zpkg 格式变更（offset 由现有 (block,instr) 派生，`.zsym` MDBG 已含数据）。

## Pipeline Steps
- [ ] VM interp：栈帧记 offset + format_stack_trace 出 `+0x`
- [ ] VM jit：set_exception/行解析同规则记 offset
- [ ] metadata：offset↔(block,instr) 换算 SoT
- [ ] z42.ir：z42 侧 `.zsym` SymOnly sidecar（MDBG）reader
- [ ] toolchain：z42d 激活 + symbolicate 子命令 + 引擎
- [ ] 测试：换算往返 + 剥离栈 golden + symbolicate 往返 + 回归
