# Tasks: review §2.1 崩溃/状态损坏簇（Dictionary / BigInt / String）

> 状态：🟢 已完成 | 创建：2026-07-12 | 完成：2026-07-12 | 类型：fix（最小化模式）
> 子系统：stdlib（短占——converge DRAFT 预留、阻塞在 compiler 锁；User Continuity 授权续做
> 批次 A 高危 fix，归档即释放）

**变更说明：** review §2.1 排名 #3 崩溃/状态损坏簇。三个独立 fix，各自单独 commit：

- [x] 1.1 `Dictionary.FindSlot` 哈希取负溢出：`if(h<0)h=-h` 对 `GetHashCode()==int.MinValue`
      仍为负（−MinValue 溢出）→ 负槽 trap。改 `h & 0x7FFFFFFF`。
      测试 `dictionary_hash_edge.z42`（int.MinValue 哈希键 Set/Get 不 trap）
- [x] 1.2 `BigInt.Equals(object)` 无类型检查强转：`(BigInt)other` 对非 BigInt/null 崩。
      改 `if (other is BigInt b)` 模式，非 BigInt→false。测试 `bigint_basic.z42`
      （`Equals("1")`/`Equals(null)`→false，同类型正确）
- [x] 1.3 `String.Substring(start,length)` 无边界检查：`Substring(2,-1)`→`new char[-1]` trap。
      加 `start<0 || length<0 || start>n-length`（避 start+length i32 溢出）抛 Exception。
      测试 `op_edge_cases.z42`（负 length / 越界 / 合法范围）

**已放弃（不在本批）：**
- [~] Assert.Equal(null,...) 空安全（review §2.1）——**放弃**：反复 FieldGet-on-Null 崩，根因在
      VM/编译器层（`object?` 空值经两参静态调用 `Equal` 的 FieldGet 派发），非 stdlib 空检查能修。
      `Assert.Null(object?)` 用 `!= null` 可用而 `Equal` 同款结构崩，差异在 VM 派发。按 philosophy.md
      根因/设计完整性原则停下——需 VM 层独立调查（单开 change），不在本 stdlib fix 批强塞。已回退所有
      Assert 改动。

**文档影响：** 无对外 API 新增/删除；Dictionary/String 行为「静默损坏/trap→抛异常」、BigInt.Equals
      「崩→返 false」。collections/numerics README 功能索引条目不变，无需改；无 book 机制变更。

- [x] 1.4 GREEN：`xtask test` **全绿**（e2e 197/0 + stdlib 全 274 文件/22 库 0 failed（含 3 fix 测试）
      + 自举不动点 7/7 + vscode-syntax；C#-free）。注：并发会话多次 clobber z42c.driver.zpkg 致假失败，
      等 4-streak 安静窗口得干净全绿
- [x] 1.5 三独立 commit + 归档 + 释放 ACTIVE.md stdlib 锁

## doc-check
- [x] 触发矩阵：无对外 API/依赖/入口变化；行为「崩溃/损坏→干净报错」。README 功能索引/核心文件表
      条目不变；无 book 机制变更（边界/类型检查非新机制）
- [x] 本次触及文档相对链接可解析
