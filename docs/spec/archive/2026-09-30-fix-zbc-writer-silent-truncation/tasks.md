# Tasks: zbc 写端静默截断 / 静默回落 → 响亮失败；>255 元素数组字面量可用

> 状态：🟢 已完成 | 创建：2026-09-30 | 完成：2026-09-30 | 归档：2026-09-30
> 分支/worktree：`fix-zbc-writer-silent-truncation` | 基于：origin/main `e4c90c6f6`（#954）
> 类型：`fix`（**无格式 bump**：只改写端校验与 IrGen 下沉，读端 / VM 不动）

**变更说明：**

- **A**：`ByteWriter.WriteU8/WriteU16` 超宽即抛（此前 `v & 0xFF` / `v & 0xFFFF` 静默截断）；
  `WriteVarint` 负值即抛（此前钳到 0）。
- **B**：`ZbcStringPool.Idx` 未命中即抛（此前返回 -1、写成 `0xFFFFFFFF`）；
  `ZbcInstr._idx` 分支目标不在块表即抛（此前返回 0 = 跳回 entry 块）；
  `WriteInstr` / `WriteTerm` 遇到没有编码的指令 / 终结子即抛（此前静默跳过，少一条指令）。
- **C**：`BoundArrayLit` 超过 255 个元素（非 blob struct）改发 `array_new` + 逐元素 `array_set`。
- **D**：indexed 写包的 SIGS 预扫（`ZpkgIndexedWriter._internSigStrings`）补上漏掉的两项——参数名缺省
  占位 `"?"` 与方法级型参名（此前写成 0xFFFFFFFF，消费方读包越界；B 之后会变成写包当场失败）；
  删掉 `ZbcWriter` 里类 / 方法型参个数的 `> 255 → 255` 静默钳位（交给 A 的宽度检查）。

**原因：** 全仓编译器审查（2026-09-30）发现写端「宽度不够 / 查表未命中」一律静默回落，产出坏字节流
而不是当场报错。实测复现：300 元素的 `int[]` 字面量编译成功，加载时
`cannot parse binary zbc: string pool index 2949164 out of range (pool size 12)` —— elem 个数被写成
`300 & 0xFF = 44`，读端把后续字节当下一条指令解码。查表常量是这种字面量的真实形态。

C 选下沉而不是加宽格式：`args` 的 u8 个数是全部调用类指令共用的编码，加宽要 zbc bump + VM 读端联动
（`ir` 类变更）；而 >255 个实参的调用不现实，真实触发点只有数组字面量，下沉即可、零格式影响。

**文档影响：** `docs/internals/src/formats/ir.md` §Arrays（u8 个数上限 + 下沉 + 写端宽度检查）。

## 任务

- [x] 1 复现：300 元素 `int[]` 字面量 → zbc 不可解析（修前）
- [x] 2 A：`ByteWriter` 宽度检查（u8 / u16 / varint）
- [x] 3 B：`ZbcStringPool.Idx` / `ZbcInstr._idx` / `WriteInstr` / `WriteTerm` 未命中即抛
- [x] 4 C：`ExprEmitter` 的 `BoundArrayLit` 分支 >255 元素下沉
- [x] 4b D：indexed 预扫补 `"?"` / 方法级型参；删型参个数钳位
- [x] 5 回归：`src/tests/basic/array_literal_large.z42`（int / string / class 三种元素 × 300，
      外加恰好 255 的边界；带 `opt_all`）+ `z42.package/tests/zpkg.z42` 的 indexed SIGS 往返单测
- [x] 6 文档：`ir.md` §Arrays
- [x] 7 GREEN：`xtask test` 全绿
