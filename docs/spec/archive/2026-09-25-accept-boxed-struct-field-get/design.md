# Design: `FieldGet` 接受装箱值 struct

## D1：修在 VM 还是修在编译器？

**选 VM（补指令臂）**，不在本刀动编译器。

| | VM 补臂（选中） | 编译器让静态类型变具体 |
|---|---|---|
| 改动面 | interp 1 臂 + JIT 1 臂，复用反射现成函数 | 类型层代换 + 调用约定桥接 |
| 字节漂移 | **零**（编译器一字不改） | 全仓（凡泛型调用的结果被使用处） |
| 自举 | 不需要两代 | 需要 |
| 能否独立落地 | ✅ | 🔴 **不能** —— 实测代换一开，`id4<…>` 立刻崩 sret 不匹配（见 proposal「不做什么」） |

两者不互斥：编译器那条做完后，本臂变成**跨包/兜底**路径（跨包泛型体今天不特化，
仍会有盒流到通用 `field_get`），不是死机制。

## D2：语义取哪一份

取**反射 `FieldInfo.GetValue`** 那一份（`accessors::boxed_struct_field_get`），不另写布局复刻：

- 基元叶子 → `decode_prim(struct_bytes, byte_off, width, tag)`
- 引用叶子 → `struct_refs[ref_index(byte_off)]`
- 嵌套 struct 叶子 → 拷出新盒（**值语义**：改返回值不动父）
- 入口先 `validate_against(delivered struct_layout)` 三层对账（抓布局复刻漂移）

一致性论证：`struct_fget_prim` 对同一个盒读的是「同一批字节 + 同一张 ref 侧表」，
只是偏移由编译期烘焙、这里由运行期按名算；两条路对同一字段必然同值，
且反射那条已有 golden + 单测护着。

## D3：为什么不动 `field_set`

`id(v).X = 5` 里的盒是**临时值**——写进去之后没有任何人能观测到（`v` 不受影响）。
补 `field_set` 臂 = 把「必然无效的写」从响一声变成**静默丢弃**，方向相反。
今天它崩 `FieldSet: expected object, got BoxedStruct`（实测），比静默好；
正解是编译期拒绝 ⇒ 登记 `reject-assign-to-erased-call-result`。

⚠️ 这条与「不留半用的机制」不冲突：本刀交付的是**读**这一条完整语义，
不是「读一半写一半」。

## D4：两条臂必须同时补

`jit_field_get` 与 interp `field_get` 是同一语义的两份实现，历史上已因只补一侧栽过
（`fix-jit-field-get-stackarray`：`arr.Length` 在 OSR 下崩，注释就在那条臂上）。
本刀两条都补，并各自做阴性对照 —— **撤 interp 臂 ⇒ interp 措辞红；撤 JIT 臂 ⇒ JIT 措辞红**。
后者尤其重要：它是「热循环真的 tier-up 进了 JIT」的唯一证据，
`--mode jit` 跑通本身不算证据（小用例可能全程解释执行）。

## D5：格式与缓存

- 无新指令、无 opcode 变更、无元数据字段 ⇒ **zbc / zpkg 格式不变**，无 fingerprint bump
- 编译器零改动 ⇒ 无字节漂移、无两代自举
- 纯运行期行为变化（原先抛异常的路径现在返回值）⇒ 不需要清缓存
