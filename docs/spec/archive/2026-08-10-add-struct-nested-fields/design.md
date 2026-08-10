# Design: 嵌套 struct 字段

> 状态：DRAFT。基于已归档、已裁决的 `add-struct-value-semantics` design（Decision β 嵌套递归展平 +
> Decision 3a 原地可变 + P1 golden `line.a.x=3`）。本 change = 兑现其 Deferred 的嵌套字段项。

## 布局：已就绪，仅放宽准入

`StructLayout._compute`（StructLayout.z42）对 `StructLeafKind.Struct` 字段已递归展平：
`Line{a:P,b:P}` 得 `{a:@0(P,size8), b:@8(P,size8)}`，嵌套 P 的引用叶子按偏移平移并入 Line 的引用位图。
故 `FieldByteOffset(Line,"a")`、`FieldByteOffset(P,"x")` 都是现成的。

`IsBlobStruct` 改动（唯一布局侧变更）：

```
public bool IsBlobStruct(string typeName) {
    if (!this.IsStructType(typeName)) return false;
    StructLayoutInfo li = this.LayoutOf(typeName);
    if (li.FieldCount < 2) return false;
    if (li.Size == 0) return false;   // 自引用/环 → 空布局兜底，不准入（防 0 字节 blob 越界）
    return true;                       // 嵌套 struct 字段现予接受（去掉原 reject 循环）
}
```

## Codegen：成员链累积 offset（纯 layout 数学 + 单次根发射）

嵌套访问 `line.a.x` 的 AST = `Member(Member(Ident line, a), x)`。**不能**对 `m.Target`（`line.a`）直接
`Emit`——那会把 struct 字段 `a` 误当叶子发射 `StructFieldGetPrim`（运行时按 struct tag 解码基元 → 崩）。
改为两个不发射码的辅助 + 一次根发射：

```
// 根 blob 句柄（沿链下降到根 lvalue，只 Emit 一次）
_structChainRoot(e):
    if e is Member m and _isBlobStruct(m.Target.Type()): return _structChainRoot(m.Target)
    return Emit(e)                       // 根：局部 ident / this / 返回 struct 的调用……

// 表达式在其根 blob 内的累积 byte offset（纯 layout 查表，零 codegen）
_structChainOffset(e):
    if e is Member m and _isBlobStruct(m.Target.Type()):
        return _structChainOffset(m.Target) + FieldByteOffset(name(m.Target.Type()), m.MemberName)
    return 0                             // e 即根 → 0
```

读 `_emitMember`（blob 分支前移到 `Emit(m.Target)` **之前**）：

```
if _isBlobStruct(m.Target.Type()):
    cont = name(m.Target.Type()); base = _structChainRoot(m.Target)
    off  = _structChainOffset(m.Target) + FieldByteOffset(cont, m.MemberName)
    if FieldIsStruct(cont, m.MemberName):        // P p = line.a：整字段读出
        cp = StructAlloc(fieldStruct, size); _copyRegion(base, off, cp, 0, fieldStruct); return cp
    tag = Tag.FromName(FieldTypeName(cont, m.MemberName))
    dst = Alloc(ToIrType(m.Type())); Emit(StructFieldGetPrim(dst, base, off, tag)); return dst
```

写 `_emitAssign`（同样前移到 `Emit(tm.Target)` 之前）对称：叶子 → `StructFieldSetPrim(base, off, tag, val)`；
整字段（`line.a = p`）→ `_copyRegion(val, 0, base, off, fieldStruct)`。

整字段递归叶子复制（复用现有指令，无区间指令）：

```
_copyRegion(srcBase, srcOff, dstBase, dstOff, structName):
    for each direct field f@fo of structName:
        if kind(f)==Struct: _copyRegion(srcBase, srcOff+fo, dstBase, dstOff+fo, typeName(f))   // 递归
        else:
            tag = Tag.FromName(typeName(f)); tmp = Alloc(...)
            Emit(StructFieldGetPrim(tmp, srcBase, srcOff+fo, tag))
            Emit(StructFieldSetPrim(dstBase, dstOff+fo, tag, tmp))
```

**为何两遍（offset 纯算 + root 单发射）不会重复 codegen**：`_structChainOffset` 全程只查布局表、不 Emit；
`_structChainRoot` 只对根表达式 Emit 一次。即使根是有副作用的 `getLine().a.x`，根也仅发射一次。

**this 拥有者 struct 的嵌套（`this.a.x`）**：`_structChainRoot(this)` 落到 `Emit(this)` = 接收者句柄
（reg0），链式累积同理生效；单层裸字段 `this.x` 仍走既有 `_emitIdent`/`_emitAssign` 的 owner 分支，不受影响。

## 自引用值 struct：Size==0 防护（本 change）+ E0438 诊断（follow-up）

值 struct 直接/间接以自身为**值**字段 → 无限大小。`StructLayout.LayoutOf` 的 `_inProgress` 环检测已能
识别并置 `ErrorType`（返回空布局 Size 0）。

**本 change 只做防护**：`IsBlobStruct` 加 `Size==0` 门 → 环 struct 不准入 blob 路径，退化为引用语义
（**与今日行为一致**：A-use 本就排除所有嵌套 struct，故自引用 struct 今日即引用语义、不崩），杜绝 0 字节
blob 的字段访问越界。**不新增崩溃面，也不改现有行为**。

**E0438 显式诊断留 follow-up**：在 **SymbolCollector**（sealed/const/partial 诊断同址，Pass1，有
`DiagnosticBag`）对"struct → 其 struct 类型值字段"图做 DFS 环检测发 `E0438`（`StructValueCycle`），
把自引用 struct 变成编译错误。与嵌套字段 codegen 正交，故先落核心能力、诊断随后补。

> 诊断码：design 1.3 原拟 E0416，但 E0416 已被 add-const-keyword（ConstNeedsInit）占用 → 用 **E0438**
> （DiagnosticCodes 当前最高 E0437，取下一个）；号已在 DiagnosticCodes.z42 以注释预留。

## 运行时 / 格式：均不动

- 运行时：`StructFieldGetPrim/SetPrim` 已接受任意 `byte_off`；整字段复制是这些指令的序列 → **z42vm 零改动**。
- 格式：无新指令、无 TYPE section 变化（嵌套引用叶子早已在 A-use 的引用位图里）→ **zbc1.31/zpkg0.36 不动**。
- 自举：源码不使用嵌套 struct → self-host 字节不动点不受影响（新 codegen 路径对现有源零触发）。

## Testing

- **Golden** `src/tests/types/struct_nested.z42`：`Line{a:P,b:P}`，验 `line.a.x` 读、`line.a.x=3` 原地写、
  `P p = line.a` 值复制独立性（改 p.x 不动 line.a.x）、`line.a = q` 整字段写、嵌套引用叶子（含 string 字段）复制共享。
- **E0438** 负测试：`struct Node { Node next; }` 报 E0438（读 SymbolCollector.Diags，见
  [[semanticdump-errorcount-skips-collector-diags]]：SemanticDump.ErrorCount 不含 collector diags）。
- **--dump-ir**：确认 `line.a.x` 发射单条累积-offset Get/SetPrim、整字段发射逐叶子序列。
- **两代自举**：gen1==gen2（本 change 对 z42c/stdlib 源零触发，应平凡不动点）。
