# Design: struct 值相等（`==` / `!=` 逐叶子值比较）

> 本变更是「struct 值类型完备化」工作流的 **PR1**（值相等地基）。PR2（struct→object 健全装箱 +
> boxed struct 的 Equals/ToString/GetHashCode/GetType/is/as）在本 PR 落地后单独 DRAFT——其
> `Equals` 直接复用本 PR 确立的**逐叶子值相等**语义。User 已裁决：保留「unboxed struct 无 vtable、
> 编译器合成值方法」的既有设计（`z42.core/Object.z42`），struct 当 object 用靠**装箱**而非形式继承。

## Architecture

```
p1 == p2  (BoundBinary, op="==", 两操作数均 blob 值 struct)
      │  ExprEmitter._emitBinary
      ├─ a = Emit(p1)   ← 操作数各求值一次（StructRef 句柄）
      ├─ c = Emit(p2)
      └─ _emitStructEquality(wantEqual, a, c, structName)
             │  枚举展平叶子（镜像 _copyRegion 的递归）
             └─ 每叶子:  la = StructFieldGetPrim(a, off, tag)
                          lc = StructFieldGetPrim(c, off, tag)
                          cmp = Eq(la, lc)                 ← 复用现有 Eq
                          BrCond(cmp, 下一叶子块, fail 块)   ← 短路
             全叶子相等 → result = ConstBool(wantEqual)
             fail       → result = ConstBool(!wantEqual)
             (result-reg-in-branches 汇合，镜像 _emitConditional 三目范式)
```

无新 IR 指令、无 zbc/zpkg 格式 bump。

## Decisions

### Decision 1: codegen 逐叶子脱糖（不新增 IR 指令，也不做 AST 层脱糖）

**问题：** blob struct 的 `==` 需值比较，现有单条 `Eq` 比 `StructRef` 句柄身份恒不等。

**选项：**
- A — 新 `StructEq`（0xC4）IR 指令，VM 遍历叶子。**缺点**：VM 只有引用位图（无全部基元叶子表）→
  正确实现要对基元区间 memcmp（float NaN 会误判等）+ 跳过 ref 区间；要做到正确反而得再扩 TYPE section
  序列化全部叶子；且触发 zbc1.31→1.32/zpkg0.36→0.37 全套 version-bump + 两代自举 + 10 fixture 重生。
- B — 编译器把 `==` 脱糖为逐叶子比较，复用现有 `StructFieldGetPrim` + `Eq`。**缺点**：IR 略长
  （struct 通常 2–8 字段，可忽略）。
- C — AST 层脱糖成 `p1.x==p2.x && …`。**缺点**：`p1`/`p2` 是复杂表达式（`f()==g()`）时会被**重复求值**
  → 副作用/性能 bug。

**决定：** 选 **B**（User 已在探索阶段裁决）。在 codegen 层对**已各求值一次**的操作数寄存器 `a`/`c`
做重复叶子读，操作数求值一次即安全；与 ①「嵌套整字段复制逐叶子分解」同范式；语义更正确（每叶子走
现有类型化 `Eq`）；零格式 bump。

### Decision 2: 短路合取用 BrCond 分支（不用 BitAnd）

**问题：** N 个叶子布尔结果如何合取成一个 bool。

**选项：** ① `BitAndInstr` 链——但 VM `bit_and` 走 `int_bitop` 返回 `Value::I64`（非 bool），类型不符；
且无可从 zbc 到达的逻辑 `And` opcode（`Instruction::And` 仅 superinstr 内部）。② `BrCond` 分支短路。

**决定：** 选 **②**。镜像现有 `_emitConditional`（三目）/`_emitShortCircuit`（`&&`）的 block API
（`_ctx.Fresh`/`StartBlock`/`EndBlock`/`BrCondTerm`/`BrTerm`）。首个不等叶子即跳 fail 块，天然短路。
`result` 寄存器在「全等」与「fail」两分支各写 `ConstBool`，end 块读——与三目 result-reg-in-branches 一致。

### Decision 3: 叶子比较语义完全复用现有 `Eq`

**决定：** 每叶子发射现有 `EqInstr`，语义由 VM 现有 `PartialEq` 提供：
- 基元叶子（int/bool/char/float，经 `StructFieldGetPrim` 字节 codec 解码）→ 值相等；**float NaN → false**
  （符合 `==` 运算符语义，A 方案 memcmp 做不到）。
- `string` 叶子（`Arc<str>`）→ **内容相等**（`Arc` deref 比较）。
- `object`/`array` 叶子（`GcRef`）→ **引用相等**（符合 z42 对象 `==` 默认 + C# `ValueType.Equals`
  对引用字段调其 `Equals`=默认引用相等）。**不递归深比较堆对象**。

嵌套 struct 字段：`_emitLeafEqChecks` 递归展平（镜像 `_copyRegion` 对 `StructLeafKind.Struct` 的递归），
最终每个真叶子一次 `Eq`。

> **衔接 PR2**：本决定确立的「逐叶子值相等」就是未来 struct 合成 `Equals`（C# `ValueType.Equals`）的
> 语义。PR2 应把 `_emitLeafEqChecks` 抽成共享 helper 供合成 `Equals` body / 装箱值相等复用，避免二义。

### Decision 4: 仅拦截 `==`/`!=`，且两操作数均为 blob struct

**决定：** `_emitBinary` 在 `op=="=="||op=="!="` 且 `_isBlobStruct(b.Left.Type()) && _isBlobStruct(b.Right.Type())`
时分流到 `_emitStructEquality(op=="==", a, c, _blobStructName(b.Left.Type()))`；否则原 `_emitCompare` 不变。
`<`/`<=`/`>`/`>=` 对 struct 无序（类型检查器本就不允许）→ 不拦截。非 blob struct（基元/引用类型/单叶子
wrapper）→ 原路径不变。

## Implementation Notes

- **新增两个私有方法**（`ExprEmitter.z42`，紧邻 `_copyRegion`/`_tagToIrType`）：
  - `_emitStructEquality(bool wantEqual, TypedReg a, TypedReg c, string structName) -> TypedReg`：
    分配 `result`(Bool) + `failL`/`endL`；调 `_emitLeafEqChecks`；全等块写 `ConstBool(wantEqual)` + `Br(endL)`；
    `failL` 块写 `ConstBool(!wantEqual)` + `Br(endL)`；`StartBlock(endL)`；返回 `result`。
  - `_emitLeafEqChecks(TypedReg a, TypedReg c, int off, string structName, string failL)`：镜像
    `_copyRegion` 遍历 `LayoutOf(structName)`；`StructLeafKind.Struct` 字段递归（`off+fo`）；否则
    `tag=Tag.FromName(FieldTypeNames[i])`、两个 `StructFieldGetPrim` + `EqInstr` + `BrCondTerm(cmp, contL, failL)` +
    `StartBlock(contL)`。
- **块结构安全**：在表达式中途开块是既有成熟范式（三目/switch/短路都这么做）；`a`/`c` 在进入分支前已在
  当前块算出，寄存器函数级作用域，跨块有效。
- **边界**：`IsBlobStruct` 保证 `FieldCount>=2 && Size>0`，恒有 ≥2 叶子（自引用空布局 struct 不是 blob →
  走原引用语义，不崩，与 ① 一致）。

## Testing Strategy

- **Golden e2e**（`src/tests/types/struct_equality.z42`，断言自检 + EXIT=0，范式同 `struct_nested.z42`）：
  - 扁平 `P{int x;int y;}`：相等 `==`→true/`!=`→false；不等→false/true。
  - 嵌套 `Line{P a;P b;}`：全叶子相等→true；任一嵌套叶子不同→false。
  - 含 `string` 叶子 `Tagged{P pt;string name;}`：同内容不同 `Arc` 实例→true（内容相等）；不同内容→false。
  - 短路负控制（首叶子已不等，结果 false / `!=` true）。
- **回归**：非 struct `==`（int/string/object）行为不变——由现有 e2e / stdlib / self-host 覆盖。
- **VM 验证**：完整 `xtask test` GREEN gate（不传 `Z42_HOME=<下载种子>`，见记忆教训——否则 regen 用旧
  z42c 编新 golden 造假 FAIL）。self-host 5/5 gen1==gen2（输出因新增 emit 改变属正常，D7 一代自愈）。
