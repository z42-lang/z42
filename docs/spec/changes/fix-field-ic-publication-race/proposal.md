# DRAFT：FieldIC / VCallIC 的发布竞态 —— 多线程下静默读错字段（Null flake 真凶）

- **类型**：vm（改 VM 执行期数据结构的并发协议）
- **状态**：已确认（User 2026-09-17 拍板：两个 bug 一起修），实施中
- **日期**：2026-09-17

## 1. 问题

`src/runtime/src/metadata/resolver/ic.rs` 的多态内联缓存，每条 entry 把 `type_id` 与其载荷
（`FieldIC`: `slot`；`VCallIC`: `slot` + `fn_idx`）存成**各自独立的 `AtomicU32`，全部 `Relaxed`**。
install 的注释写着「write type_id LAST」，但两个独立的 Relaxed 存储**在 ARM 上不构成发布顺序**，
读侧两个 Relaxed 载入之间也没有 acquire 屏障。于是并发线程可以看到：

> **新的 `type_id` + 还没写入的 `slot`（= `UNRESOLVED` = `u32::MAX`）**

`field_get` 拿着这个槽位调 `field_value(u32::MAX)`，而 `metadata/types/object.rs:231` 对越界槽位
**静默返回 `Value::Null`** —— 没有任何报错，读错的字段值就这样流进程序。

文件顶部注释把这个竞态判定为「无害、会收敛」（*"bounded to got wrong cached entry … subsequent reads
converge to a valid state"*）。这个判断对 `VCallIC` **碰巧**成立——`vcall_ic_hit` 有
`if fn_idx == UNRESOLVED { return None; }` 的哨兵回落；对 `FieldIC` **不成立**，它直接把 slot 返回给调用方。

## 2. 证据（三条独立，互相印证）

1. **debug 断言抓现行**（既有的 `assert_field_ic_slot`，720 次运行命中 12 次，形态 100% 一致）：
   ```
   FieldIC mis-hit: receiver `Std.Net.Http.HttpHeaders` (TypeId 632) field `_count`
   cached at slot 4294967295, but its field_index says Some(3).
   ```
   `4294967295` 正是 `UNRESOLVED` —— 不是「别的类型的槽位」，而是**尚未写入的载荷**。
   ⚠️ 断言文案把成因归给「两个类型撞 TypeId」，那条早已由 #535（TypeId 全局唯一化）堵死，**文案需要一并修正**。

2. **A/B（交替两轮）**：给两个 lookup 加 `Z42_IC_OFF` 开关（`OnceLock` 启动读一次）让 PIC 永远 miss：

   | 组 | Null |
   |---|---|
   | IC 开 | 26/1200、41/1200 |
   | IC 关 | **0/1200、0/1200** |

3. **GC 被排除**：每次运行开 `Z42_GC_TRACE`，45 次失败**全部发生在整个运行 0 次 GC 的进程里**
   （用例小到触发不了收集）。本 flake 与 GC 无关。

## 3. 症状与影响面

三种历史签名全都是「字段读出 Null」的下游表现：

| 读到 Null 的字段类型 | 抛出的错 |
|---|---|
| bool | `BrCond expects bool, got Null` |
| int | `ArraySet index: expected non-negative integer, got Null` |
| 数组 | `ArraySet: expected array, got Null` |

**影响面远不止 z42.net 的测试**：任何「多个线程首次走到同一个字段访问点」的场景都可能静默读错字段。
之所以只在测试里频繁暴露，是因为危险窗口只在**进程冷启动、IC 首次安装**时存在——每个 test target 是一个新进程。

## 4. 方案

**把每条 entry 的 (type_id, 载荷) 合并成单个 `AtomicU64`**，发布天然原子，不需要任何内存序协议：

```rust
// 现在
pub struct FieldICEntry { type_id: AtomicU32, slot: AtomicU32 }
// 改为（高 32 位 type_id，低 32 位 slot）
pub struct FieldICEntry { packed: AtomicU64 }
```

- **`VCallIC` 同样收敛成一个 `AtomicU64`**：其 `slot` 字段**已是死载荷**——唯一消费点
  `interp/vcall_resolve.rs:79` 写的是 `let (_slot, fn_idx) = …`，JIT 侧共用同一个 `vcall_ic_hit`。
  因此改成 (type_id, fn_idx) 打包即可，顺手删掉一个没人读的字段。
- 读侧一次 `load`、解包、比 tid —— 比现在的两/三次 load **更省**，热路径不退化。
- `UNRESOLVED` 哨兵语义不变（空 entry = tid 为 `UNRESOLVED`）。

### 为什么不是「把 Relaxed 改成 Release/Acquire」

Release/Acquire 只修「先写载荷、后发布 tid」这一个方向。**驱逐方向仍是坏的**：entry 原本是
(tidA, slotA)，为 tidB 安装时先覆盖 slot 再改 tid，一个正在找 tidA 的读者可能读到 tidA + slotB。
要靠内存序修就得引入「先置 UNRESOLVED → 写载荷 → 再发布 tid」+ 读侧复读 tid 的 seqlock 协议，
比打包成一个原子量更复杂、更容易再出错。**打包让撕裂在结构上不可能。**

## 5. 实施记录

- `ic.rs`：`FieldICEntry` / `VCallICEntry` 各收敛成一个 `AtomicU64`（高 32 位 TypeId、低 32 位载荷）；
  `VCallIC` 的死载荷 `slot` 一并删除（唯一消费点写作 `_slot`），`vcall_ic_lookup/install` 签名相应收窄。
- 文件头的「撕裂无害、会收敛」整段注释重写为发布协议说明 + 为什么 Release/Acquire 不够。
- `assert_field_ic_slot` 的 panic 文案补上第二种成因（此前只归因于 TypeId 撞号，那条已由 #535 堵死）。
- 机制页 `docs/internals/src/runtime/inline-cache-publication.md`（已接 SUMMARY）。

## 6. 验证计划

- **退回对照**：改前 67/2400，改后应为 **0/2400**（同一台机器、同一复现器、交替跑）。
- **debug 断言**：`assert_field_ic_slot` 保留；改前 720 次命中 12 次，改后应 0 次。
- 单测：补一条「并发 install/lookup 不会观察到 tid 与载荷不配对」的用例（`loom` 或压力式）。
- `cargo test` 全量 + `xtask test` GREEN。
- 性能：IC 是热路径，跑 bench 确认不退化（预期持平或略好——载入次数变少）。

## 7. 复现器（本次调查的产出，供回归用）

直接跑测试目标已编好的 `app.zpkg`，单次 0.06 秒、可安全并行（无共享 staging）：

```bash
VM=artifacts/build/runtime/release/z42vm
LIBS=artifacts/.scratch/alllibs/release
APP=src/libraries/z42.net/artifacts/test-targets/http_server_threaded/build/app.zpkg
Z42_LIBS=$LIBS $VM artifacts/build/toolchain/builder/z42.builder.zpkg -- test $APP
```

⚠️ 给热路径加探针/开关**必须**用 `OnceLock` 只读一次：第一版 A/B 我在 lookup 里调 `std::env::var`，
它自己要拿全局锁、把路径串行化，**两组都变 0/1200**，控制组被自己的探针污染。
