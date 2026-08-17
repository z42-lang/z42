# Tasks: 瘦化 jit_vcall IC 命中路径（延迟 from_utf8 方法名解码）

> 状态：🟢 已完成 | 创建：2026-08-17 | 完成：2026-08-17 | 类型：perf（单文件，最小化模式）

**变更说明：** `jit_vcall` 把方法名 `from_utf8` 解码从函数入口下移到 IC fast-path 之后——
IC 命中（Object/primitive 缓存命中）路径提前 return、根本不需要方法名，却在入口无条件解码。

**原因：** mono-vcall JIT profile 显示 `from_utf8` 占 vcall 成本约 6-9%，且 100% 是命中路径的浪费
（方法名只在 IC miss 的 `resolve_virtual` / 基元-盒 / 装箱路径才用到）。纯重构、外部行为逐字节不变。

**文档影响：** 无（纯内部热路径实现调整，不改机制/接口/行为；docs/book jit 页无需更新——IC 语义不变）。

- [x] 1.1 `jit/helpers/vcall.rs`：删入口 `let method = from_utf8(...)`，
      在 IC fast-path 块之后、BoxedStruct 路径之前重新解码 `method`
- [x] 1.2 `cargo build --release --bin z42vm` —— 无错
- [x] 1.3 `cargo test --release --lib` —— 927/0（+2 ignored）+ z42_compression 21/0
- [x] 1.4 `./xtask test all --skip zzz` —— GREEN 全绿（e2e interp+vm-jit-consistency + stdlib + 自举 5/5 gen1==gen2 逐字节 + vscode）
- [x] 1.5 性能对照：mvcall jit **1.470s→1.345s = 8.5% 快**（jit-vs-interp 1.66×→1.74×）；poly jit 754.8ms→696.5ms = 7.7% 快；三 bench 输出逐字节不变
- [x] 1.6 归档 + PR + memory

## 备注
- 正确性：IC 命中路径（vcall.rs:58-96）不引用 `method`（grep 证实首用在 line 105）；miss/基元/装箱/vtable
  路径在下移点之后解码，逐字等价。`method_ptr`/`method_len` 是函数参数，全程在作用域内。
