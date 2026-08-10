# Design: struct 合成对象协议方法（PR2b）

> PR2a 的收口。落地 `z42.core/Object.z42` 契约「compiler synthesises value-semantic Equals/GetHashCode/
> ToString」。无新 opcode、无格式 bump（复用 PR1 叶子指令 + 现有 builtin 派发）。

## Architecture

```
IrGen 类成员循环（IrGen.z42:186-327）末尾，对 blob 值 struct 注入（未显式声明才合成）：
  {FQ}.Equals$1(object other)   ← FunctionEmitter.EmitSynthStructEquals
  {FQ}.GetHashCode$0()          ← FunctionEmitter.EmitSynthStructHashCode
  {FQ}.ToString$0()             ← 单块 ConstStr(typeName)（StubEmitter 风格）
        │ _pushFunc → funcs → build_func_index 按名注册（loader.rs:1139）
        ▼
boxed struct o.Equals(x) / GetHashCode() / ToString():
  exec_vcall.rs / jit vcall 的 BoxedStruct 臂：
    GetType → builtin_obj_get_type（保留精确类型，特判）
    否则 → unbox this(BoxedStruct→arena StructRef) → 候选 [{type_name}.{m}$arity, {type_name}.{m},
                                                        Std.Object.{m}$arity, Std.Object.{m}] → exec
```

## Decisions

### Decision 1: 合成 z42 方法体（编译期有全叶子信息），非 VM native 逐字节
**问题：** boxed struct 的 Equals 值比较在哪做？
**选项：** A VM native 逐字节 memcmp——NaN 误判 + VM 只有引用位图无全叶子表（同 PR1 方案 A 老问题）；
B 编译器合成方法体，复用 PR1 `_emitLeafEqChecks`（有全叶子 offset+kind，NaN 精确）。
**决定：** **B**。与 PR1 一致、语义精确。GetHashCode 同理逐叶子合成。

### Decision 2: 注入点 = IrGen 类成员循环末尾（合成 ctor 同位）
**决定：** `IrGen.Generate` 遍历 `c.Members` 后，若 `c.Kind=="struct"` ∧ `owner` 是 blob struct ∧ 未在
`owner.Methods` 显式声明该方法键 → 合成并 `_pushFunc`。按名注册（`{FQ}.Equals$1` 等），运行时零额外管线。
镜像 IrGen.z42:311-327 合成 ctor 块。

### Decision 3: Equals body = is-check + unbox + 叶子比较
**决定：** `EmitSynthStructEquals`（FunctionEmitter 脚手架，reg0=this StructRef、reg1=other object）：
```
if (other is P) { P o=(P)other; return leafEq(this,o); } else return false;
```
IR：`IsInstance(t, reg1, FQ)` → `BrCond(t, doL, falseL)`；doL：`AsCast(u, reg1, FQ)`（VM 拆箱 BoxedStruct→
StructRef）+ `_emitStructEquality(true, reg0, u, name)` → ret；falseL：`ConstBool(false)` → ret。复用 PR1 私有
helper（改 internal 或经 ExprEmitter 新 public 入口）。

### Decision 4: GetHashCode = 逐叶子 FNV 合并
**决定：** `EmitSynthStructHashCode`（reg0=this）：`h=seed(2166136261)`；逐叶子（镜像 `_emitLeafEqChecks`
遍历）：基元叶子 `StructFieldGetPrim`→值直接混入（`h=(h^leaf)*16777619`，用 Mul/BitXor 指令）；引用叶子
（string/object）→ `VCall leaf.GetHashCode()` 混入；嵌套 struct 递归。结果 `& 0x7fffffff` 非负（`__str_hash_code`
同款，Dictionary 契约）。同值恒同 hash（无随机）。

### Decision 5（D5）：`==` on boxed = 值相等；`.Equals()` = 合成叶子方法
**决定：** 延续 PR2a `PartialEq`（值相等：type_name∧bytes∧refs），不改为 C# 引用语义——z42 boxed 是 owned
Box 非共享 GcRef，引用语义 ill-defined，且值相等与 ② 一致、确定。`.Equals()` 走合成方法（叶子 `Eq`，float
NaN 精确）。**边角**：`==` 对 float NaN 按位判等 vs `.Equals` 用浮点== → 极少数含 NaN 的 struct 二者微差，
文档标注，pre-1.0 可接受（要完全一致须让 `==` 也走 vcall Equals，代价不值）。

### Decision 6: ToString = 类型名（C# ValueType 默认）
**决定：** 合成 `ToString$0` 单块 `ConstStr(短类型名)` + ret（StubEmitter 风格，无 EmitContext）。C#
`ValueType.ToString()` 默认返回类型名、非字段 dump；字段 dump 留后续（Out of Scope）。

## Implementation Notes

- **VM boxed vcall unbox this**：BoxedStruct 臂调 PR2a 的 `unbox_struct(ctx, frame, b)` 得当前帧 StructRef 作
  `call_args[0]`，再按候选名 exec。interp（`exec_vcall.rs`）+ JIT（`jit/helpers/vcall.rs`）对称。
- **显式声明检测**：`owner.Methods.ContainsKey("Equals$1")` 等——已声明则跳过合成（用户版优先）。
- **暴露 PR1 helper**：`_emitStructEquality`/`_emitLeafEqChecks` 现 `private`；加 `ExprEmitter` public 入口
  （如 `EmitStructEqualityInto`）或改 internal，供 FunctionEmitter 合成路径调。
- **ExportedTypeExtractor**：struct 分支注入 `ObjectMethods.Four()`（或至少 Equals/GetHashCode/ToString）签名，
  使反射/SIGS 一致；不影响 VCall（VCall 靠 func_index 按名）。

## Testing Strategy

- Golden `src/tests/types/struct_object_methods.z42`：Equals 同值/异值/异类型/嵌套/string 叶子；GetHashCode
  同值同 hash；`Dictionary<P,int>` 存取命中；`==`/`!=` on boxed；ToString 类型名（断言自检 EXIT=0）。
- 完整 `xtask test` GREEN（不传 Z42_HOME）+ self-host 5/5 + `cargo test --lib`（VM vcall 改动）。
