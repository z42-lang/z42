# Fix: JIT builtin 解析容错 —— ext builtin 编译期不可解析时按名回落到调用期

## Why

#350 把 JIT tier-up 默认阈值降到 1（`jit_threshold==1`）后，函数在**第一次调用即编译**，
早于 interp 首次执行它。`resolve_function_tokens`（填 `builtin_tokens`，含 native-ext facade
如 `__deflate_compress` 的 `ext_builtin_id_of` 解析）此前由 interp 在 `exec_function` 惰性调用；
现在也会在 JIT 编译期跑。若该 VM 此刻 ext 注册表尚未就绪 / 该 facade lib 未加载，
`resolver.rs:392` 的 `unwrap_or_else(panic)` 直接**崩溃整个 VM**——CI bench 的
`compression_bench` 在 z42b runner 下即因 `unknown builtin __deflate_compress` panic 红。

interp 早有容错：`Some(id) => by_id, None => exec_builtin(name)`——按名在**调用期**再查一次
ext 注册表。JIT 缺这条回落，且 resolver 直接 panic 而非留 UNRESOLVED。

## What Changes

编译期 builtin 解析改为**非致命**，两个消费方都在**调用期**按名回落（此时 ext 必已就绪）：

- `metadata/resolver.rs`：无法绑定的 builtin 存 `UNRESOLVED` 而非 panic。
- `interp/exec_call.rs::builtin`：`Some(UNRESOLVED)` 与 `None` 同样走 `exec_builtin(name)`。
- `jit/helpers/call.rs::jit_builtin`：新增 `name_ptr/len` 参数；`id==UNRESOLVED` 时按名
  `exec_builtin(name)`（热路径仍按 id 零 hash 分派）。
- `jit/translate/call.rs`：发射 name 指针；token 缺失 / UNRESOLVED → 发 UNRESOLVED（去掉编译期
  static-only panic）。
- `jit/helpers/registry.rs`：`jit_builtin` 声明补 name 参数。

## 验证

- `cargo test --lib -p z42`：1035 passed / 0 failed。
- 本地：compression（Deflate/Zstd）`--mode jit` 正常；ext dylib 全移除后 → 优雅
  `Std.Exception: unknown builtin`（可 catch），**不再 VM panic**。
- 全量 GREEN（bench + stdlib-jit + bootstrap）交 CI。

## Out of Scope

- 同短名跨命名空间类型在 runtime type_registry first-wins 冲突（`new A.Foo` 得到 `B.Foo` 身份）——
  独立的既有 correctness bug，见调查笔记，另行处理。
