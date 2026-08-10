# Design: struct 泛型容器装箱（P3a）

> 格式中立，复用 PR2a/2b 的 `__box_struct` + `AsCast` 拆箱 + boxed-vcall 臂。P3b（真内联+写屏障）后随。

## Architecture

```
存入（box）：                                    取出（unbox）：
 list.Add(p) / d[k]=v / ctor(p)                   P p = list[i] / d[k]  /  foreach(P p in list)
   参数目标 = Z42GenericParamType(T/K)                get_Item 结果（V/T subst = blob struct P）
        │ TypeChecker.BoxIfNeeded（erasesS+type-param）    │ 包 BoundConvert(getItem, P)
        │ / _bindAssign set_Item 手动 BoxArgs               │ ExprEmitter._emitConvert（P blob ∧ src 非blob）
        ▼                                                    ▼
   BoundBox → __box_struct → Value::BoxedStruct       AsCast → unbox_struct → 当前帧 arena StructRef（值副本）
        │ 存进容器 TKey[]/T[]（堆稳定）                       │
        ▼                                                    ▼
   容器 key.GetHashCode()/Equals() → boxed-vcall 臂     P p 得独立值 struct
   （PR2b：native hash / 合成 Equals$1）
```

## Decisions

### Decision 1: 装箱在泛型边界（type-param 目标），非改容器 ABI
**决定：** 容器 backing 仍 `TKey[]/TValue[]/T[]`（运行期擦除为 object 槽）——**不改容器源码 / ABI**。只在
**编译器 coercion 边界**把 struct→type-param 装箱。`BoxIfNeeded` 的 `erasesS` 谓词加 `|| target is
Z42GenericParamType`（type-param 目标 = 擦除边界，同 object/接口）。**格式中立**（探查确认）。

### Decision 2: indexer-set 单独补装箱（绕过了 BoxArgs）
**问题：** `d[key]=v` 由 `ExprTyper._bindAssign` 手搭 `set_Item` BoundCall，不走 `BoxArgs`。
**决定：** 该分支查 `aCls.Methods.Get("set_Item")` 的 `Signature.ParamTypes`，对 index/value 各调
`BoxIfNeeded`（等价 BoxArgs）。使 `d[structKey]=v` / `d[k]=structVal` 装箱。

### Decision 3: 取出用隐式 UnboxIfNeeded，复用 `_emitConvert` 的 AsCast 拆箱臂
**问题：** `get_Item` 现把 `V/T` 擦成 `Unknown`（`ExprTyper._bindIndex`），`P p = d[k]` 无拆箱 → BoxedStruct
落进 P 槽 → 字段访问崩。
**决定：** 新增 `TypeChecker.UnboxIfNeeded(value, target)`（对称 BoxIfNeeded）：`target` 是 blob 值 struct ∧
`value.Type()` 非同 struct（Unknown / boxed / type-param）→ 包 `BoundConvert(value, target)`。codegen 的
`_emitConvert` 已有臂（`_isBlobStruct(target) && !_isBlobStruct(src) → AsCast`）→ 运行期 `unbox_struct` 拆回
当前帧 arena StructRef。插入点：`get_Item`（`_bindIndex`，让结果类型 = subst 后 struct 或 Unknown 都能触发）、
var-decl/assign（`StmtBinder`）、foreach writeback（`FunctionEmitter`）。**优先在 retrieval 站点就近包**（get_Item/
foreach），使拆箱与来源绑定、不误伤普通 struct 赋值。

### Decision 4: 只装箱、非字节内联（P3b 边界）
**决定：** P3a 容器存**boxed struct**（堆对象），非字节内联进容器 backing。密度收益（字节 backing / 对象内联
struct 字段）留 P3b（格式 bump + 写屏障）。P3a 目标 = **正确性**（不崩、不悬垂、值语义），非密度。

## Implementation Notes

- **非 blob struct / 基元 / 引用类型 → type-param**：`_emitBox` 对非 blob 源已透传（不误装箱）；`BoxIfNeeded`
  的 type-param 分支只在 `sct.IsStruct` 且（codegen 再核 `IsBlobStruct`）时真装箱 → `Dictionary<string,V>` /
  `List<int>` / `Dictionary<object,V>` 不受影响。
- **get_Item 类型**：现擦 `Unknown`。可保持 Unknown 但在 blob-struct-元素 时包 BoundConvert(→ subst struct)；
  或让 `_bindIndex` 用 `_substGeneric` 求元素类型、若 blob struct 则包拆箱。取更小 diff。
- **foreach**：`FunctionEmitter` 的 foreach 发 `VCall get_Item` 到 elemReg 后 writeback 循环变量——在 writeback
  前，若循环变量类型 blob struct，对 elemReg 发 AsCast 拆箱。
- **值语义**：拆箱 `unbox_struct` 已在当前帧 arena 新分配 + 拷 bytes/refs（PR2a）→ 取出的 P 是独立副本，改它
  不动容器。

## Testing Strategy

- Golden `src/tests/types/struct_generic_container.z42`：`Dictionary<P,int>` 存取/ContainsKey/覆盖；`List<P>`
  Add/index/foreach/Contains；取出值独立性；`Tagged{string}` 键内容相等；非 struct 泛型回归（`List<int>`/
  `Dictionary<string,int>`）。断言自检 EXIT=0，interp+jit。
- 完整 `xtask test` GREEN（不传 Z42_HOME）+ self-host 5/5 + `cargo test --lib`（虽 VM 无改动，防回归）。
