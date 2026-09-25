# Tasks: `FieldGet` 接受装箱值 struct

## 实施

- [x] `corelib/reflection/accessors.rs`：`boxed_struct_field_get` 可见性 `pub(super)` → `pub(crate)`
      （+ 注明它现在被 `field_get` 复用）；`reflection/mod.rs` 的 `mod accessors` → `pub(crate) mod`
- [x] `interp/exec_object.rs` `field_get`：新增 `Value::BoxedStruct` 臂（落在兜底 `bail!` 之前）
- [x] `jit/helpers/object_field.rs` `jit_field_get`：对称臂（错误经 `set_exception` 回传，不 panic）
- [x] `field_set` 不动（D3）

## 验证

- [x] 四种形态皆通：`id(v).X` / `id<Vec2>(v).X` / `H.id<Vec2>(v).X` / `Holder.keep(v).Y`
- [x] 四种叶子皆正确：基元 42 / 引用 `"hi"` / **bool（`==true` 真且 `==false` 假**，排除静默坏值）
      / 嵌套 `In.A=3` `In.B=4`
- [x] 对照未被改坏：`id(v).Sum()` = 16（vcall 路）、`Vec2 w = id(v); w.X` = 7（`struct_fget_prim` 路）
- [x] JIT：`Z42_JIT_PROFILE=1` 确认 `osr-compile HotSum` + `lazy-compile id`，200k 次循环结果正确
- [x] **阴性对照（撤回修复本身，非改期望值）**
      - 撤 interp 臂 → `FieldGet: not an object or known value type`（interp 措辞）红
      - 撤 JIT 臂  → `FieldGet: expected object`（**JIT 措辞**）红 ⇒ 证明 JIT 臂真被走到
      - ⚠️ 第一次阴性对照**假绿**：只跑了 `xtask build sdk`（不重编 Rust）⇒ 验的是上一轮 `z42vm`。
        配方固定为 `build runtime` **再** `build sdk`
- [x] e2e golden `src/tests/generics/erased_return_blob_field.z42`（flat 模式，assert-only，
      每个读出的值都被 Assert 用掉 —— 否则 DCE 会消掉整条 `field_get`，用例变摆设）
- [x] `cargo test --lib`（debug、**不带名字过滤**）：1394 passed / 0 failed（+ compression 21）
- [x] `xtask test` 全仓：**✅ GREEN，8m16s，零 `✗` / 零 `failed`**
- [x] 确认新用例**真被跑到**（不是零命中）：`xtask test e2e --file …` → interp 1 passed + jit 1 passed
- [x] 量了代价：500 万次 `id(v).X` = 2.29s vs 同规模普通对象字段读 0.78s ⇒ 每访问 ~300ns
      布局重算（`struct_reflect::compute` 无缓存）。**不是回归**（此前这条路是崩），
      登记 `cache-struct-reflect-layout`

## 文档

- [x] `docs/reference/src/language/generic-constraints.md`：该格从「运行期必崩」改为正常，
      并**订正原文记错的根因**（原文写「根因是泛型特化」——实测与特化无关）
- [x] `docs/internals/src/runtime/struct-value-semantics.md`：新增「`field_get` 接受装箱 struct」节
      （机制 + 两条臂 + 阴性对照 + `build runtime`/`build sdk` 的坑）
- [x] `docs/roadmap.md`：本条落地 + 登记三条 Deferred

## 登记的 Deferred

- `substitute-generic-call-return-type` —— 把（显式/推断的）类型实参代换进调用点返回类型。
  含一行真 bug：`MethodTypeArgSubst.ForExplicitTypeArgs` 用 `TypeParamNames.Length`（= 解析器
  `new string[4]` 的**容量**）当型参个数 ⇒ 本地声明的泛型方法/自由函数上显式 `<T>` 代换**从未生效**
  （跨包导入的因元数据数组精确长度反而生效）。**不可单独修**：代换一生效就撞 sret 不匹配
  （= 坑点 ④a），需与泛型边界的物理约定统一同刀，参 `#814 add-iface-return-bridge` 桥接范式。
- `reject-assign-to-erased-call-result` —— `id(v).X = 5` 应编译期报错（今天运行期崩 FieldSet）。
- `cache-struct-reflect-layout` —— 按类型名缓存 `ComputedLayout`（布局按类型不变 ⇒ 缓存安全），
  本刀把它从冷路径（反射）拉上了热路径。
