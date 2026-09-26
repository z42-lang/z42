# Tasks: 单字段 struct 的值语义（坑点 ⑤）

- [x] `StructLayout.IsBlobStruct`：`FieldCount < 2` → `< 1`（+ 注释写明为什么推翻 Phase B 塌缩）
- [x] VM 镜像：`interp/exec_array.rs::try_struct_backed` 同步翻（全 VM 唯一一份镜像）
- [x] `StubEmitter._emitNativeStubSret`：extern 返回 blob 值 struct 的 sret 通道
      （`builtin` → `as_cast` → `struct_copy` → `ret`，置 `MethodFlagSret`）
- [x] `corelib/gc.rs`：`make_gc_handle` 产装箱 struct；`extract_gc_handle_slot` 吃三种承载
- [x] golden `src/tests/types/single_field_struct_value_semantics.z42`（10 组）：
      赋值复制 / **bool（`==true` 与 `==false` 各钉一格）** / long / string（引用叶子）/
      ctor·方法·属性 / 传参 copy-in / 类的内联字段 / 数组元素 / `==` 值相等 / 装箱身份 + `is`/`as` /
      `Std.Guid`（stdlib 的单字段 struct）
- [x] 既有 `gc_handle` golden 保持绿（它是 native 那条路的唯一守门）
- [x] 文档：`docs/learn/src/types/structs-records.md`（「两个坑」→「一个坑」+ 历史注）/
      `docs/reference/src/language/structs.md` / `memory-model.md`「已知偏差」→「已修」
- [x] 活示例 transcript 更新：`examples/types/structs-records/gaps/run.console`
      （`单字段: a.X=50` → `a.X=1`）—— ⚠️ 它正是把坏行为钉住的那道门，**全仓唯一因本刀判红的用例**
- [x] `xtask test examples` 74 transcripts / 122 steps ✅
- [ ] `CompilerFingerprint` bump（合并前按当时的 main 现查取号）
- [ ] `xtask test` 全仓 + `cargo test --lib`
- [ ] `docs/roadmap.md` 行从「待 D3 裁决」改为已落地

## 过程中逼出的两条既存缺陷（都不是 ⑤ 引入的）

1. **跨包静态调用漏传 sret** ⇒ 独立成刀 `fix-crosspkg-static-sret`（#851）。
   判据：把闸门翻回去重建、双字段 `Pair.Make` 跨包**仍然崩** ⇒ 与 ⑤ 无关。
2. **`extern` 桩不支持 blob 返回** ⇒ 在本刀内修（`_emitNativeStubSret`）。
   之所以从没响：全仓唯一这种形状是 `GCHandle.Alloc`，而 GCHandle 翻门前不是 blob。
