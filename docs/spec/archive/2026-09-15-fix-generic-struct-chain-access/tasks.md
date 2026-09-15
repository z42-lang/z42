# Tasks: fix-generic-struct-chain-access

> 状态：🟢 已完成 | 创建：2026-09-15 | 完成：2026-09-15

**变更说明：** 泛型 struct 套 struct 的链式字段读写（`t.Item1.Item2` / `pp.First.Y = v`）只在链节**真内联**时累加字节偏移，擦除成 `T` 的字段断链取句柄。
**原因：** `_structChainRoot` / `_structChainOffset` 只看「容器是 blob struct」就累加偏移；泛型 struct 布局按定义算、`T` 字段是引用叶子（存另一块 blob 的句柄），累加 = 在外层 blob 上读写 ⇒ 读到外层同偏移字段（静默错值）/ 装箱崩 / 写进外层别的字段。book `tuples.md` 把它登记成「限制」，实为 bug。
**文档影响：** `docs/book/src/runtime/struct-value-semantics.md`（嵌套字段节 + 收敛面与延后）、`docs/book/src/language/tuples.md`（删限制条）、`docs/roadmap.md`（Deferred 索引加一行）。

## 实测（修复前，origin/main 674610ffa）

| 形态 | interp / jit |
|---|---|
| `((int,string),int) t; t.Item1.Item1` | `__box_prim: expected integer value, got StructRef` |
| `t.Item1.Item2` | `2`（外层 Item2；应为 `"a"`） |
| `Pair<Pair<int,long>,string> pp; pp.First.Second` | `__box_prim … got Str("s")`（外层 Second） |
| `pp.First.Y = 5L`（`Pair<P2,string>`） | 写进外层别的槽，`pp.First.Y` 仍为旧值 |
| `var x = t.Item1; x.Item2` / 外层是 class / 单字段泛型 struct | 正确（不走累加链或单节取句柄） |

## 任务

- [x] 1.1 `AccessEmitter._isInlineChainLink(m)`：容器 blob struct 且 `Layouts.FieldIsStruct(容器, 成员)`；`_structChainRoot` / `_structChainOffset` 两处改用它（非内联节落到 `_ee.Emit(e)` 取句柄、偏移 0）
- [x] 1.2 golden `src/tests/types/generic_struct_chain.z42`：嵌套元组 2/3 层读、用户泛型 struct 套泛型 struct 读 + 链式写 + `+=`、擦除节后接内联节（`Pair<Line,int>.First.To.Y`）、纯内联链不变、`struct[]` 元素上的擦除链；interp + jit 均过；阴性对照（修复前编译器编同一文件）崩在第一条断言
- [x] 1.3 文档同步：struct-value-semantics.md 嵌套字段节补「链节必须真内联」+ 收敛面 ✅ 条；tuples.md 删限制条
- [x] 1.4 发现的 Scope 外问题登记 Deferred（见备注）：struct-value-semantics.md 收敛面与延后 + roadmap Deferred 索引
- [x] 2.1 `xtask test` 完整 GREEN（base main 6d57a694f，全 stage ✅ 5m44s；首轮 cross-zpkg `struct_cross_pkg` 红 → 前置修复见备注）

## 备注

- **前置修复（同 PR 先一个 commit）`fix-crosspkg-nested-struct-layout`**：首轮 GREEN `cross-zpkg/struct_cross_pkg` 红——新判据读 `FieldIsStruct`，而导入 struct 的嵌套 struct 字段此前被误判成引用叶子（FQ 拼写查不到裸名键）；旧链式代码不看 kind、靠嵌套 struct 恰好 8B 读对。根因修在导入侧拼写，见该 change。

- **Scope 外发现（未修，已登记 Deferred `generic-struct-erased-slot-value-copy`）**：struct 值存进 `T` 槽（泛型 struct / 泛型 class 字段、构造实参）按句柄存不复制，复制外层泛型 struct 浅拷句柄 ⇒ 别名。实测：`new Pair<P2,int>(inner,1)` 后改 `inner.Y`、`new CBox<P2>(inner)` 后改 `inner.Y`、`pp.First = inner` 后改 `inner.Y`、`var q = pp; q.First.Y = 9` 均被另一方看到；`Id<P2>(inner)` 返回值有复制、不受影响。
  本修复后链式**写穿**写的是擦除槽指向的 blob，于是 `q.First.Y = 9` 这类写会被别名看到——修复前这句写进外层别的字段（更糟），现为「位置对、值语义缺」。根治属值语义设计（存入复制 / 外层深拷 / 读出复制 + 禁写穿），独立 change。
