# tasks: struct-copy-no-alloc

> 类型：**perf / refactor**（去掉每次值 struct 复制的两次堆分配）｜ 创建：2026-09-27
> 出身：结构审计 2026-09「其它高价值单点」的「性能首推一刀」。

## Why

`StructArena::copy_into` 是**值语义的复制点** —— 每个 `S b = a;`、每次按值传参、每次按值
返回都走它。而它的实现是：

```rust
let (src_bytes, src_refs): (Vec<u8>, Vec<Value>) = { … (s.bytes.to_vec(), s.refs.to_vec()) };
let d = self.slots.get_mut(dst_idx)…;
d.bytes[..n].copy_from_slice(&src_bytes[..n]);
```

**两次无条件堆分配**，纯粹为了绕借用检查（src 与 dst 是同一个 `Vec` 的两个下标）。
代价与 struct 大小无关 —— 一个 4 字节单字段 struct 也照付。

改法：先各查一次做校验，再 `split_at_mut` 拿两个不相交的借用，**就地复制、零分配**。

## 🔴 实测：收益是「struct 密集的用户代码」，**不是**编译器变快

必须把两个结果都摆出来，别只说好的那个。

| 负载 | old | new | 差 |
|---|---|---|---|
| struct 复制密集的 micro（`Vec4` × 200 万次复制 + 传参），`--mode interp` | 0.670s | 0.630s | **−6.0%** |
| 同上，`--mode jit` | 0.610s | 0.550s | **−9.8%** |
| **真实编译负载**（编 `z42.project`，30 个文件，每次清缓存，各 6 次） | 中位 1.113s | 中位 1.116s | **+0.26%（stdev 1.7% ⇒ 噪声内，无可测差异）** |

micro 的方差 ~1.6%、效应 6~10% ⇒ 结论稳。真实编译负载**量不出收益**，原因是
**z42c 自己热路径上几乎不用多字段值 struct**（审计已实测：`z42.core` 里一个多字段 struct 都没有）。

⇒ **不要把这条写成「编译器提速」**。它提速的是「用了值类型的用户代码」——而那正是值类型
存在的理由（审计原话：值 struct 目前比堆对象更贵，与值类型的存在理由相反）。

A/B 用**两个二进制**交替跑（本仓踩过四次性能误判，见 [[z42-forward-tests-and-gc-park]] 的铁律）。

## What Changes

`src/runtime/src/interp/struct_arena.rs::copy_into`：
1. 先各 `.get()` 一次校验 src / dst（下标 + `frame_id`）；
2. `src_idx == dst_idx` 提前返回（`split_at_mut` 无法给出同一元素的两个借用）；
3. 按较大下标 `split_at_mut`，两侧各取一个借用，就地 `copy_from_slice` + `clone_from_slice`。

### 三处容易改坏的地方，都配了判据

| 风险 | 判据 |
|---|---|
| 两个方向走**不同分支**（`split_at_mut` 在较大下标切） | `copy_into_works_in_both_index_directions`：src<dst 与 dst<src 各一格 |
| self-copy 新增了提前返回，**校验可能被跳过** | `copy_into_self_is_noop_but_still_validated`：no-op 且 stale/越界仍报错 |
| 校验从「get + get_mut」变成「get + get」，某一侧可能漏检 | `copy_into_rejects_either_side_stale`：四种坏输入逐个判红 + 正常输入仍成功 |

## Scope（允许改动的文件）

- `src/runtime/src/interp/struct_arena.rs`
- `src/runtime/src/interp/struct_arena_tests.rs`

## Tasks

- [x] `copy_into` 改 `split_at_mut`，零分配
- [x] 3 条新单测（两方向 / self-copy 仍校验 / 两侧 stale 各自判红）
- [x] `cargo test --locked --workspace --lib`（**无过滤**）：1370 passed, 0 failed
- [x] 两个二进制 A/B 实测（micro + 真实编译负载，结论如上表）
- [x] `xtask test e2e` 全绿（738 passed, 0 failed）
- [ ] GREEN：CI 全矩阵绿

## 不做（Out of Scope）

- **不动 `unbox_struct` / `copy_array_elem_out` 里的 `to_vec`**。它们的 src 是**堆对象 / 数组元素**、
  dst 是新 arena 槽 —— 不是同一个 `Vec` 的两个下标，`split_at_mut` 用不上；要去掉那两处得换别的
  手法（先 alloc 再从堆侧直接写），是另一刀，且它们不在「每次赋值」这条路上。
- **不碰 `Value` 的表示**。审计里「值 struct 比堆对象更贵」还有另外两项（`new S(...)` = 3 次堆分配
  + 1 锁 + 1 RwLock + 1 哈希），那些要动分配路径与类型查找，属独立的刀。
- **不声称编译器提速**（见上）。
