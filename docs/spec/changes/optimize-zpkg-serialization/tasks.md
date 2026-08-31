# Tasks: 优化 zpkg 序列化（ByteWriter 批量写）

> 状态：🟡 进行中 | 创建：2026-08-31 | 类型：perf（byte-identical，无格式/行为/API 变更）

**变更说明：** z42.ir 的 `ByteWriter` 逐字节 `WriteU8`（每字节 2 次解释执行方法调用 WriteU8+_ensure）+ 多级嵌套 `AppendWriter` 重复复制，使 zpkg 序列化（`WritePackedWithSidecar`）在解释执行下 ~18s（占单包编译 51%）。改为**一次性 ensure 容量 + 紧凑内联拷贝/直写**，消除 per-byte 方法调用开销。纯 z42 优化，不加 native builtin（避两-nightly），输出**逐字节不变**。

**原因：** 相位插桩实测：zpkg 序列化 ~18s 是单包编译最大相位（>codegen 8.6s）。见 [[compiler-parallel-heavy-phases-investigation]]。

**文档影响：** ByteWriter 是内部实现，无对外行为变更 → 仅目录 README（若功能索引提及）+ 可能 z42.ir 机制页注记；byte-identical 由自举 gen1==gen2 保证。

## Scope
| 文件 | 变更 | 说明 |
|------|------|------|
| `src/libraries/z42.ir/src/BinaryFormat/ByteWriter.z42` | MODIFY | AppendWriter 批量拷贝；_ensure 一次扩容；WriteU16/U32/I64 ensure-once+直写；WriteUtf8Bytes ASCII 快路径；ToBytes 直拷 |
| `src/libraries/z42.ir/tests/<name>/` | NEW（若无覆盖）| ByteWriter 单测：各 Write 方法字节等价 + AppendWriter 等价 + 增长边界 |

## 阶段 1: 优化实现
- [ ] 1.1 `AppendWriter`：`_ensureCap(other._len)` 一次 + 紧凑 `while` 直写 `_buf[_len+i]=other._buf[i]` + 一次性 `_len += other._len`（去 per-byte WriteU8/_ensure）
- [ ] 1.2 `_ensure`：扩容判定按目标总量一次到位（`while cap < needed: cap*=2`），拷旧内容一次
- [ ] 1.3 `WriteU16/U32/I64`：`_ensure(2/4/8)` 一次 + 直接 `_buf[_len..]=...` 直写（去多次 WriteU8 调用）
- [ ] 1.4 `WriteUtf8Bytes`：ASCII（cp<128）快路径——`_ensure(n)` 预判 + 直写；多字节回落逐 scalar
- [ ] 1.5 `WriteVarint` / `WriteStr` / `Patch32` / `ToBytes` / `ToHex`：按需 ensure-once 直写（保持字节等价）

## 阶段 2: 验证
- [ ] 2.1 ByteWriter 单测：每方法输出与优化前逐字节一致（含空/单字节/跨扩容边界/多字节 UTF-8/AppendWriter 嵌套）
- [ ] 2.2 rebuild z42.ir.zpkg（nightly z42c）→ 用它建 z42c.semantics，**byte-identical**（与优化前产物逐字节 diff）
- [ ] 2.3 性能：编 z42c.semantics 写相位墙钟（插桩或整体）—— 确认序列化 ~18s 大幅下降
- [ ] 2.4 完整 GREEN：`cargo build` + `xtask test`（e2e/cross-zpkg/stdlib/compiler/vscode-syntax）；**自举 gen1==gen2 byte-identical**（序列化改动的正确性门）
- [ ] 2.5 文档同步（若触发矩阵命中）

## 备注
- byte-identical 是硬门：ByteWriter 输出任何漂移 = 自举断链。每个方法优化后单测逐字节对比。
- 不加 native builtin（`__array_copy` 等）——那会踩两-nightly（z42.ir 是 z42c 运行期自依赖）。留作后续更大优化。
- 测量配方（种子/libsdir）见 [[compiler-parallel-heavy-phases-investigation]]。
