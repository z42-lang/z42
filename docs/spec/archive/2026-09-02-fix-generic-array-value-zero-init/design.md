# Design: 泛型数组值类型零初始化根修（方案 C：显式操作数）

## Architecture

```
new T[n]  (T = 泛型形参, 绑值类型)
   │  codegen: ExprTyper._bindArrayNew 解析形参归属 + 索引（复用 default(T) 逻辑）
   ▼
ArrayNewInstr{ elemTag=Unknown, elemName="T",
               TypeParamKind=1(method)|2(class), TypeParamIndex=idx }   ← 新操作数
   │  zbc 编码：ArrayNew opcode 尾部加 kind(u8) + index(varint)  ← 格式 bump 1.37/0.42
   ▼
VM array_new (exec_array.rs)
   │  kind==0 → 原路径（default_value_for_tag(elem_tag)），行为不变
   │  kind!=0 →
   │    ① concrete = kind==1 ? frame.method_type_args[idx]        (method 级)
   │                        : receiver.Object.type_args[idx]       (class 级，镜像 DefaultOf)
   │    ② default = default_value_for(concrete)      ← 现成（int→I32(0) 等）
   │    ③ backing = pack_backing(concrete, [default; n]) / try_struct_backed(concrete)  ← 现成
   │    ④ ArrayObj.element_type = concrete            ← 修正反射元素类型
   ▼
ArrayObj{ element_type="Std.Int32"(具体), backing=I32([0;n]) }   ← 正确零值 + 非擦除元素类型
```

VM 侧「产零值 + 打包 + struct-backed 布局解析」能力已完全具备（`default_value_for` /
`pack_backing` / `try_struct_backed` + 运行期 `method_type_args`/`type_args` 解析，`default(T)` 已用）。
本变更只补「把泛型形参的 (kind, index) 从 codegen 显式传到 array_new」这条缺失连接线。

## Decisions

### Decision 1: 显式操作数（C） vs 哨兵字符串（A） vs codegen 填充（B）

**问题**：如何让 VM 在 array_new 时知道泛型元素引用哪个类型参数？

| 方案 | 机制 | 优 | 缺 | 结论 |
|------|------|----|----|------|
| **A 哨兵字符串** | codegen 把 `elemName` 写成 `$mtp<idx>`/`$ctp<idx>` 塞进既有 element_type 字符串通道；VM 解析哨兵 | 零格式-bump | element_type 引入需解析的 `$` 哨兵约定；**是被格式-bump 封锁逼出的权宜之计、天带日后重构成操作数的返工** | ❌ 否决 |
| **B codegen 填充** | `new T[n]` 后 emit `for(i) a[i]=default(T)` 循环 | 零格式-bump、不改 VM | 症状级（element_type 仍擦成 "T"，反射/打包不修）；O(n) 冗余写；字节码膨胀；**违背根因修复**（正是 Resize 现有绕过的形态） | ❌ 否决 |
| **C 显式操作数** | `ArrayNewInstr` 加 (kind, index) 操作数，与 `default(T)` 同构 | **数据模型最干净**、与 `default(T)` 平行、顺带修反射元素类型、无返工 | zbc/zpkg 格式 bump | ✅ **User 2026-09-02 裁决** |

C 曾唯一的代价（格式 bump 撞 CI 两代自举回归）**已消除**：回归由 #383 修复并经探针 #385 复验，格式 bump
走两代自举自动过。故 C 无前置阻塞。

### Decision 2: 方法级 vs 类级类型参数（镜像 default(T)）

`default(T)` 已分两条，本设计一一对应：

| 层级 | `default(T)` 指令 | VM 取值 | 本设计 kind | array_new 取值 |
|------|-----------------|---------|-----------|---------------|
| 方法级 `<T>` on method | `MethodDefaultInsn(reg, ParamIndex)` | `frame.method_type_args[idx]` | `kind=1` | `frame.method_type_args[idx]` |
| 类级 `<T>` on class | `DefaultOfInstr(reg, ParamIndex)`（`ParamIndex>=0` gated） | 接收者 `Object.type_args[idx]` | `kind=2` | 接收者 `Object.type_args[idx]` |

codegen 在 `_bindArrayNew` 判元素形参归属（method 还是 class 级）与索引，**复用 `default` 绑定节点已有的
ParamIndex 来源**（ExprTyper 里 default 的解析路径），不新写解析逻辑。

**类级 kind=2 的接收者获取**：`array_new` 需拿到 `this`/接收者以读其 `type_args`。参照
`exec_address.rs` 的 `default_of` 如何取接收者 type_args（`DefaultOfInstr` 消费点）；若 array_new 当前
frame 无直接接收者句柄，实施时从 frame 的 self 槽取（与 default_of 同源）。**若实现复杂度过高，先只落
method 级（kind=1，覆盖泛型方法主场景），class 级留 TODO** —— 见 Decision 5。

### Decision 3: elemTag 保持 Unknown；VM 只对**基元**类型参数改默认值（narrow）

emit 端 `elemTag` 仍走 `ToIrType(泛型形参)=Unknown`（不改现有类型→tag 映射）。VM array_new 见
`kind!=0` 时解析具体类型名，**仅当它是基元值类型**（`default_value_for` 返回非 Null）才把**每槽默认值**
换成该基元零值；数组的 **backing 与 `element_type` 保持擦除不变**（reference-backed）。这复刻已验证的
`default(T)` 赋值行为（reference-backed 数组、槽为基元零值），是修 `__box_prim` bug 的最小改动。

**为什么不改 backing / 不走 struct-backing**（2026-09-02 实测收敛）：把已解析的 **struct** 类型参数走
`try_struct_backed` 会把泛型容器的 `new T[n]`（T=struct）变成值打包 struct 数组 → `arr[i]` 物化
`StructRefHeap`，而泛型容器（`Dictionary<P,int>`/`List<P>`）按引用/装箱存 struct、对元素发 boxed
`VCall` → `VCall: expected object, got StructRefHeap`（`struct_generic_container` 回归）。故 struct / 引用
类型参数**一律保持原擦除路径**，只有基元受影响。`kind==0`（非泛型）完全走原路径，逐字节不变（自举安全）。

> **舍弃的次要收益**：不再顺带修「泛型数组反射元素类型 `"T"`→具体」（那需要改 element_type/backing，
> 与上面的 struct 安全冲突）。反射元素类型修正若需要，另起 change 专门处理 struct-backing 的兼容。

### Decision 4: 移除 Array.Resize 绕过

根修落地后 `Array.Resize` 尾部显式 `for(i) result[i]=default(T)` 冗余，删除，交回 `new T[n]` 自身
零初始化。回归测试确认 Resize 尾部仍为值类型零值。

### Decision 5: class 级可选降级（实施期决策点）

若 class 级接收者 type_args 在 array_new 入口获取成本高，允许**首版只落 method 级**（泛型方法内
`new T[n]`，覆盖 `Array.Resize`/`ConvertAll` 等主场景），class 级（泛型类字段 `new T[n]`）留后续。
但**格式已按含 kind 设计**（kind=2 预留），不二次 bump。实施时先探 default_of 取接收者的难度再定；
倾向一次做全（method+class）。

## Implementation Notes

- **VM 热路径开销**：仅 `kind!=0` 才走解析分支；非泛型数组（绝大多数）`kind==0` 零额外开销。
- **越界/缺失兜底**：`kind!=0` 但 `method_type_args` 为空或 idx 越界（理论上 codegen 不会发）→ 回落
  `Value::Null`（同当前行为，不 panic）。
- **struct 值类型元素**：解析出的具体类型可能是 struct → 复用 `try_struct_backed(ctx, concrete, n)`
  （`exec_array.rs:25-39` 已按 element_type 字符串解析 struct 布局），天然覆盖。
- **具体类型名格式**：`method_type_args` 里存的是 FQ 还是短名？实施时确认（`default(T)` 的
  `default_value_for(&str)` 接受什么），保证 `pack_backing`/`try_struct_backed` 能解析。
- **反射元素类型**：ArrayObj.element_type 存**解析后的具体类型名**（非 "T"、非操作数）；操作数只存在于
  zbc 指令与 array_new 入口。

## Testing Strategy

- **VM 单测**（`exec_array_tests.rs`）：构造 `ArrayNewInsn{ kind=1, index=0 }` + `frame.method_type_args=["int"]`
  → 断言 backing=I32、元素=0；kind=0 老路径不变。
- **端到端 [Test]**（`generic_array_zero_init.z42`）：
  - 泛型方法 `T[] make<T>(int n) => new T[n];`，`make<int>(3)[0]` == 0（传 object 触发装箱不报错）。
  - 各值类型 int/bool/char/double + 值 struct。
  - 泛型类字段 `new T[n]`（kind=2，若首版含 class 级）。
  - `Array.Resize<int>` 去绕过后尾部仍 0（回归）。
  - 引用类型 T（string）未写槽仍 null（不回归）。
  - 泛型数组反射元素类型为具体类型（非 "T"）。
- **GREEN**：`cargo build --release` + 完整 `xtask test`（含自举不动点——**kind=0 编码不变保证 gen1==gen2**；
  格式 bump 走两代自举，本地 warm 验，冷路径以 CI 为准）。
