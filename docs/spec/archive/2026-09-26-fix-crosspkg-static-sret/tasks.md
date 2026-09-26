# Tasks: 跨包静态调用返回 blob struct 时漏传 sret

- [x] `CallEmitter._emitCall` 的 DepIndex **static** 分支改走 `_emitCallSretAwareG`
- [x] `CallEmitter._emitStaticAccessor` 的依赖分支改走 `_emitCallSretAware`
- [x] 新 fixture `src/tests/cross-zpkg/single_field_struct_cross_pkg/`（双字段正面用例 + 单字段前瞻）
- [x] 阴性对照：修复前该 fixture 红在 `Pair.Make$2$long$long ... takes 3, the call passes 2`
- [x] `xtask test e2e --dir cross-zpkg`：82 passed / 0 failed
- [ ] `xtask test` 全仓 + `cargo test --lib`
- [x] `CompilerFingerprint` → 28（理由与让号实录见 `CacheStore.z42`）
- [x] 文档：`docs/internals/src/runtime/missing-symbol.md`（sret 的 SoT 页）补「三条捷径里静态那条漏了」
- [ ] `docs/roadmap.md` 落地行

## 调查笔记（值得留着的两条）

- ⚠️ `--dump-ir` / `--dump-bound` **不加载 stdlib/依赖** ⇒ 任何「跨包调用点发了什么」的结论
  都不能靠它们；要么在编译器里打点，要么看运行期措辞。我曾据此得出「loose VCall」的错误结论。
- ⚠️ 手工复现跨包 fixture 时，消费方**只扫 SDK 自己的 `libs/`**（`Z42_LIBS` 对装好的 z42c 无效）
  ⇒ 要把生产方 zpkg 拷进 `artifacts/.z42/libs/`；且**改完要清消费方 `artifacts/`**，
  否则缓存命中、探针一行都不打（我第一次就这么空跑了一轮）。
