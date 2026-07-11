# Design: delegate 元数据（P1-e ②）

## Architecture

```
DelegateDecl（含泛型）
  │ IrGen（新 pass，镜像 P1-a enum）
  ├─► IrClassDesc { Name: _q(dd.Name), Flags: 0x40, TypeParams: dd.TypeParams }  → TYPE
  └─► IrFunction `<FQ>.Invoke` 死体桩（实例/virtual/参数源拼写+名+P1-d 元数据）→ SIGS+FUNC
                                                    │
VM：CLASS_FLAG_DELEGATE=1<<6 → TypeDesc.is_delegate() → Type.IsDelegate
    GetMethods → own_methods 前缀扫描自动含 Invoke → 既有 MethodInfo 面反射签名
```

## Decisions

### D1: tps 存 TYPE 条目，Invoke SIGS 按名引用
z42c IrFunction 无 typeparam 写支持（SIGS tpCount 恒 0）。泛型 delegate（Action<T1..>）的 tps
存 **TYPE 条目 TypeParams**（TYPE 已支持）；Invoke 参数类型写 "T1" 等名。P2 重建
ExportedDelegateZ：名/tps 取 TYPE，签名取 SIGS Invoke——两处拼合，不需扩 SIGS tp 写。

### D2: Invoke = 死体桩（复用 P1-c abstract-stub 机制）
SIGS/FUNC 按 index 配对 → 必须有 FUNC body。合成 MethodDecl（"public virtual" Invoke，
dd.Params/RetType）→ `_emitAbstractStub` 同款死体（`ret null`/`ret`）。真实 delegate 调用走
CallIndirect（FuncRef/Closure），桩永不被调。`method_flags` bit0=virtual（C# Invoke virtual）。
参数类型用**源拼写**（TSIG `_sigTypeName` 同口径——P2 对账需逐字节一致）。

### D3: 不动 ResolveTypeP；typeof(delegate) Deferred
delegate 名在 ResolveTypeP 必须继续解析为 Z42FuncType（否则破坏 delegate 赋值/类型检查）。
typeof 对 delegate 维持现状；反射入口 `Type.GetType(fq)`（运行期 registry 查找，已可用）。

### D4: bump 沿 1.19 先例
bit6 无额外 payload（TYPE 记录布局不变），但 flags 语义扩展 + 新增 TYPE/SIGS 条目 → 沿
interface（1.19）先例 bump zbc 1.26 / zpkg 0.30。两代自举（gen1-stdlib EMPTY Z42_LIBS）。
stdlib 会新增 Action（非泛型）+ 泛型内建 delegate 的 TYPE 条目 → stdlib 字节变、z42c 源无
delegate 声明 → z42c 字节不变、不动点不受影响。

## Implementation Notes
- IrGen pass 位置：镜像 P1-a enum pass（`d is EnumDecl` 分支旁加 `d is DelegateDecl`）。
- 泛型 delegate 的 TypeParams：`dd.TypeParams.Names/Count`；constraints 空（与 enum 同）。
- Invoke 桩参数含 this（index 0 = "this"，类型 = delegate FQ 名）；_fillParamMeta(md, true)。
- 源拼写：复用/镜像 `_sigTypeName`（在 FunctionEmitter；若 private 则 IrGen 内联最简版：
  NamedType→Name+泛型args 拼接、ArrayType→elem+"[]"）。
- 反射：`__type_is_delegate` 镜像 `__type_is_enum`（P1-a）全套（builtin + 注册 + Type.z42 getter）。

## Testing Strategy
- reflection.z42 [Test]：`delegate int Adder(int a, int b);` → `Type.GetType(fq).IsDelegate`、
  GetMethods 含 Invoke（ret int / IsVirtual / 2 参数名 a,b 类型 int）；非 delegate 类 IsDelegate=false。
- golden：empty/f5 无 delegate → 仅 header minor 变；stdlib regen（Action 等新增条目）。
- 全 GREEN + 两代自举 + 不动点 + cargo。
