# 内联缓存（PIC）：为什么一条 entry 必须是一个原子量

> SoT：`src/runtime/src/metadata/resolver/ic.rs`。
> 由 `fix-field-ic-publication-race`（2026-09-17）确立。

## 这个缓存是什么

`FieldGet` / `FieldSet` 和 `VCall` 都是热路径：前者要按字段名查 `TypeDesc.field_index`（哈希），
后者要查虚表。两者都用**每调用点一份的 4 槽多态内联缓存（PIC）**把「接收者类型 → 结果」记下来：

| 缓存 | 键 | 载荷 |
|---|---|---|
| `FieldIC` | 接收者 `TypeId` | 字段槽位 `slot` |
| `VCallIC` | 接收者 `TypeId` | 目标函数下标 `fn_idx` |

查找是 4 槽线性扫描，遇到 `UNRESOLVED` 提前退出；未命中就走权威查表再 install；
槽位满了按 `round_robin` 驱逐。解释器与 JIT **共用**同一组 `*_ic_lookup` / `*_ic_install`。

## 铁律：(TypeId, 载荷) 必须成对发布

**每条 entry 是一个 `AtomicU64`**：高 32 位 TypeId、低 32 位载荷。安装写一次、查找读一次。

```rust
const fn pack(type_id: u32, payload: u32) -> u64 { ((type_id as u64) << 32) | payload as u64 }
```

因为这一对**永远同生同死**，撕裂在结构上不可能，所以 `Relaxed` 就够——
这里要的不是跨线程的先后顺序，而是「这一对不许被拆开」。

## 为什么不能用两个原子量 + 内存序

这不是假设，是一个真实存在过、并且**静默**了几周的 bug：

entry 原本是两个独立的 `AtomicU32`，install 先写载荷、再写 TypeId，注释写着
"write type_id LAST"。**在 ARM 上这不构成发布顺序**（两个 `Relaxed` 存储可以被另一个核乱序观察到），
于是并发线程能看到：

> 新的 TypeId ＋ 还没写入的载荷（`UNRESOLVED` = `u32::MAX`）

`field_value(u32::MAX)` 越界，而它对越界槽位**静默返回 `Value::Null`**——没有任何报错。
Null 顺着数据流走下去，表现成三种毫不相干的错：

| 读到 Null 的字段 | 最终抛出的 |
|---|---|
| `bool` | `BrCond expects bool, got Null` |
| `int` | `ArraySet index: expected non-negative integer, got Null` |
| 数组 | `ArraySet: expected array, got Null` |

旧注释断言这个撕裂「无害、会收敛」。**对 `VCallIC` 碰巧成立**——`vcall_ic_hit` 有
`fn_idx == UNRESOLVED` 的哨兵回落；**对 `FieldIC` 不成立**：它把错槽位直接交给调用方。

### 光把 `Relaxed` 换成 `Release`/`Acquire` 也不够

Release/Acquire 只保住「先写载荷、后发布 TypeId」这一个方向。**驱逐方向仍然坏**：

```text
entry 原为 (tidA, payloadA)，现在要装 (tidB, payloadB)
  写者：payload.store(payloadB)  →  tid.store(tidB)
  读者：读 tid 拿到 tidA（还没看到新 tid）  →  读 payload 拿到 payloadB
       ⇒ 用 tidA 的身份拿到了 tidB 的载荷
```

要靠内存序修，就得引入「先置 `UNRESOLVED` → 写载荷 → 再发布 TypeId」+ 读侧复读 TypeId 的
seqlock 协议。**打包成一个原子量让这些协议全都不必要**，而且读侧从 2~3 次 load 降到 1 次。

## 顺带删掉的死载荷

`VCallICEntry` 原本存三个字段 `(type_id, slot, fn_idx)`。`slot`（虚表槽位）**没有任何消费者**——
唯一的读取点写的是 `let (_slot, fn_idx) = …`。留着它就得凑够 96 位、没法单原子发布，所以删了。

## 守这条不变量的东西

| 门 | 位置 | 说明 |
|---|---|---|
| debug 断言 | `assert_field_ic_slot`（`ic.rs`） | PIC 命中后拿权威 `field_index` 复核，不符即 panic。**它就是抓到本 bug 的那条**（形态 `cached at slot 4294967295`） |
| debug 断言 | `assert_pic_target`（`interp/vcall_resolve.rs`） | VCall 侧同款：命中的目标必须属于该接收者 |
| loom 模型 | `tests/ic_publication_loom.rs` | 穷举交错验证打包协议不撕裂；**自带阴性对照**（旧的双原子协议，`--ignored` 手动跑，必红） |
| 并发压力单测 | `metadata::resolver::resolver_tests` | 多写多读跑 300ms，断言命中的载荷永远配对 |

> ⚠️ **给这条热路径加探针/开关时**：必须用 `OnceLock` 在启动时读一次环境变量。
> 调查期间我把开关写成在 lookup 里调 `std::env::var`，它自己要拿全局锁、把路径串行化，
> 结果**对照组和实验组都变成 0 失败**——探针把要测的竞态掩盖掉了。
