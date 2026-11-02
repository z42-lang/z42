# profiler/

## 职责

CPU 采样、堆摘要、V8 堆快照与保留链诊断。GC 提供基础遍历与分配采样回调。

## 功能索引

| 文件 | 职责 |
|------|------|
| `cpu.rs` | safepoint 采样、栈聚合及 folded / trace 输出 |
| `heap/summary.rs` / `heap/types.rs` | 按类型聚合数量和字节及摘要数据 |
| `heap/snapshot.rs` | V8 图构建与 JSON 导出 |
| `heap/retention.rs` | 反向引用图、直接引用者与保留根查询 |

## 基础用法

`cpu::Sampler` 由 VM 持有并在 safepoint 轮询。`heap::take_snapshot` 使用 GC 的存活过滤；
V8 导出使用同一 live-value 语义。保留链经 `gc::walk` 枚举对象、数组及分类根，
调用方沿用完整回收和线程暂停协议。锁内遍历回调不得重入堆。

## 如何测试验证

```bash
(cd src/runtime && cargo test --lib profiler)
./xtask test
```

## 关联文档

[诊断与剖析](../../../../docs/internals/src/runtime/diagnostics.md)、
[堆保留诊断](../../../../docs/internals/src/runtime/heap-diagnostics.md)、
[GC](../../../../docs/internals/src/runtime/gc.md)。
