---
name: add-ir-op
description: 向 z42 IR 添加新的指令（opcode）。在用户说"新增 IR 指令"、"加操作码"、"实现 xxx 指令" 时触发。
user-invocable: true
allowed-tools: Read, Edit, Grep
argument-hint: <instruction-name>
---

# 添加 IR 指令：$ARGUMENTS

## ⚠️ 先读这一节：多数情况下你不该加 opcode

新增 IR opcode 要改 **~20 个文件**，并**必然 bump zbc + zpkg 格式**（strict-pin：minor 不等
直接 bail），连带 zbc 6 个 + zpkg 4 个 fixture regen（后者无一键 regen）、golden hex 单测重截、
触发 ci-bootstrap 两代自举。

**先评估这两条更便宜的路**：

| 路线 | 成本 | 何时适用 |
|---|---|---|
| **复用 `BuiltinInstr` + 常量字符串参数** | ~3 文件，**零格式 bump** | 新语义能表达成「一次 builtin 调用 + 常量串区分」。`BuiltinId` 不进 wire（是进程内下标），表尾追加即可。先例：`__box_prim`（`TypeOpEmitter._emitBox`）、`__sym_available`（`_emitSymAvailable`） |
| **attr-ref 哨兵 + 编译期消解** | ~5 文件，**零格式 bump** | 新特性能在编译期完全消解成既有构造。骑既有 SIGS/param attr-ref blob 通道（zbc 1.11/1.15 起就有），未标注符号不追加 → 老 golden 逐字节不变。先例：`$Deprecated` / `$Default` / `$Caller:*` |

项目历史上三次同类需求**全部**选了哨兵而非新 flag 位/opcode，注释反复强调「避开格式-bump
两代自举回归」。**拿不准就先问，别默认加 opcode。**

## 真要加 opcode：完整清单

> 下面的路径是 2026-09 复核过的。旧版本本文件曾指向 `src/runtime/src/bytecode.rs` /
> `interp.rs` —— 那些路径**早已被重构掉**，且完全没提 version bump，照着做会严重低估成本。

### 编译器侧（z42.ir + z42c.semantics）

1. `src/libraries/z42.ir/src/IrInstr*.z42` — 新 `sealed class XxxInstr : IrInstr`
2. `src/libraries/z42.ir/src/BinaryFormat/ZbcFormat.z42`（`static class Op`）— 新 opcode 常量
3. `.../ZbcInstr.z42` — 编码分支（带字符串则同时补 `InternInstrStrings`）
4. `.../ZbcReaderInstr.z42` — 解码分支 + 寄存器上界/重映射
5. `src/compiler/z42c.semantics/src/IrOptInfo.z42` — **4 处**：Dst / args 计数 / args 替换 / Dst 改写
6. `src/compiler/z42c.semantics/src/IrEscapeAnalysis.z42` — args 逃逸标记
7. 发射点（各 `*Emitter.z42`）

### 运行时侧（Rust）

8. `src/runtime/src/metadata/bytecode/instruction.rs` — `Instruction` variant + `written_reg()`
9. `src/runtime/src/metadata/zbc_reader/opcodes.rs` — `OP_XXX` 常量
10. `src/runtime/src/metadata/zbc_reader/instr_decode.rs` — 解码
11. `src/runtime/src/metadata/resolver.rs` — 若需 per-site token 解析
12. `src/runtime/src/interp/exec_*.rs` — 执行语义。**`exec_instr` 的 match 无 `_` 兜底，必须显式处理**
13. `src/runtime/src/jit/translate/*.rs` — **要么翻译，要么**登记进
    `src/runtime/src/jit/translate/unsupported.rs` 的 `unsupported_reason`（单一真相表）

### 格式 bump（强制链，9 步）

完整 checklist 见 [`.claude/rules/version-bumping.md`](../../rules/version-bumping.md)。要点：

14. `ZbcFormat.z42` 的 `ZbcVersion.Minor++` + 常量旁 changelog
15. `src/runtime/src/metadata/zbc_reader/versions.rs` 的 `ZBC_VERSION_MINOR` + changelog
16. `docs/design/runtime/zbc.md` Minor changelog 表加行
17. regen `src/tests/zbc-format/*/source.zbc`（`xtask build test`）
18. `src/compiler/z42c.semantics/tests/zbc/zbc_tests.z42` 内嵌 hex 串重截
19. **联动 zpkg**：`ZpkgWriter.z42` 的 `Minor++` + Rust `ZPKG_VERSION_MINOR` + `zpkg.md` changelog
    + **手工** regen `src/tests/zpkg-format/*`（无一键 regen）

### 文档

20. `docs/design/runtime/ir.md`、`src/libraries/z42.ir/README.md`、`src/runtime/src/interp/README.md`

## 自举纪律

若 **z42c 自身的源**要使用新 opcode：按
[`bootstrap-seed.md`](../../rules/bootstrap-seed.md) 的「support 先行、晚一个 nightly 再 use」
拆两个 PR 跨两个 nightly。只加编解码/执行能力而不发射 → 无字节变化 → 可单 PR
（`ZbcFormat.z42` 里 0xC0–0xC3 的注释是这个做法的先例）。

## 验证

```bash
cd src/runtime && cargo build --release --bin z42vm
./xtask build compiler
./xtask test              # 全 stage
./xtask test bootstrap    # 改了语法/格式能力必跑
```
