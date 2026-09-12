# Tasks: 打包成 zpkg 的测试丢失 TIDX 字符串

> 状态：🟢 已完成 | 完成：2026-09-12
> 变更类型：`fix`（最小化模式）

**变更说明：** zpkg 路径上 `[ShouldThrow<E>]` / `[Skip(reason)]` 等**只被 TIDX 引用**的字符串
被解析成 null —— 加载器解析了两次，第二次对着 merge 后的**重建**池，覆盖掉第一次的正确结果。

**原因：** 两处不匹配，各自都足以致命：
1. TIDX 的 `*_str_idx` 指向 **raw** 字符串池，而 `read_zbc` 的 `rebuild_string_pool`
   **只保留被代码引用的串** —— 只被 TIDX 引用的（`[ShouldThrow<E>]` 的类型链、
   `[Skip(reason)]` 文案）在重建时就没了。
2. 打包 zpkg 的 TIDX 索引**已经是全局的**（写入端 `ZpkgWriter` 用 `remap[i] = pool.Intern(...)`
   映射进共享池），比重建后的合并池长 —— 越界即 None。而聚合处还在按
   `module.string_pool.len()` 叠加「累计字符串偏移」，把本来对的索引又推错一次。

**为什么藏了这么久：** 走裸 `.zbc` 的路径恒对（单模块 + 对着 raw 池解析），而 GREEN gate 的
stdlib 测试一直用 `z42c --emit-zbc` 产裸 zbc。只有当测试改由 z42b 编成 zpkg 来跑
（`xtask-forward-tests-to-z42b` 步骤 D1）才暴露 —— 症状是每个 `[ShouldThrow]` 用例都报
「expected throw null」而判失败（实测 z42.test 的 dogfood 单元 7 个失败）。

**文档影响：** `docs/book/` 的 TIDX / 测试发现机制页（若已有对应段落）。

- [x] 1.1 `read_mods_section`（打包态）：就地对**全局 raw 池**解析，元组第三项从裸字节
      改为**已解析条目**
- [x] 1.2 `load_zpkg_indexed`（索引态）：每个散装 `.zbc` 对**自己的 raw 池**解析
      （`read_test_index_resolved`）
- [x] 1.3 `aggregate_zpkg_test_index`：只补 `method_id` 的累计函数偏移，**不再动字符串索引**
- [x] 1.4 `assemble_zpkg_artifact`：删掉那次对重建池的**二次解析**（正是它覆盖成 null 的）
- [x] 1.5 回归夹具：`src/tests/z42b/dev-target-internal/tests/dirunit/` 加一个
      `[ShouldThrow<TestFailure>]` 用例 —— 该夹具由 z42b 编成**打包 zpkg** 再跑，直击这条路
- [x] 1.6 GREEN：`xtask test` 全 13 stage 绿 + `cargo test --lib` 1229 + 21 全过
