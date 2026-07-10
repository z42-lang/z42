# Design: 成员可见性元数据（P1-b）

## Architecture

可见性从"只在 TSIG"补进运行时段（TYPE 字段块 + SIGS），供反射读:

```
z42c: FieldDecl.Mods / MethodDecl.Mods ──IrGen──▶ IrFieldDesc.Visibility / IrFunction.Visibility
   （复用 ExportedTypeExtractor：显式 public/private/protected，默认 public）
         │ ZbcWriter
         ▼
   TYPE 字段块每字段: name,type_tag,type_str,attrs,+visibility:u8   （实例 + 静态块同款）
   SIGS 每函数: name,paramCount,ret,execMode,is_static,+visibility:u8,tpCount,...
         │ Rust read_type / read_sigs
         ▼
   FieldDesc.visibility / FuncSig.visibility → loader → TypeDesc
         ▼
   反射 builtin 塞 FieldInfo.IsPublic / MethodInfo.IsPublic
```

## Decisions

### D1: visibility 编码 = u8 code（非 string idx）

`0=public, 1=private, 2=protected`（`internal`/其余归 0=public，MVP）。比 TSIG 的 string-idx 更紧凑。
z42c 有一个 `_visCode(mods) -> int` helper（复用 ExportedTypeExtractor 默认 public 逻辑）。

### D2: 非 gated——字段/SIGS 布局每条 +1 字节

与 enum 块（gated）不同,可见性对**每个**字段/函数都写 → TYPE 字段块 + SIGS 布局变 → 所有含
字段/函数的 zpkg 字节变（regen）。additive:新字节追加在既有记录**末尾对应位置**(字段:type_str+attrs
之后;SIGS:is_static 之后、tpCount 之前——须与 reader 严格对齐)。

### D3: SIGS 插入位——is_static 之后、tpCount 之前

`read_sigs` 现读序:name,paramCount,ret_tag,ret_str,execMode,is_static,[params],tpCount,...
visibility 插在 **is_static 之后、params 之前**(与 writer WriteSigEntries 对齐)。zbc 单模块 SIGS
与 zpkg 全模块 SIGS **共用 WriteSigEntries**（单源）→ 一处改两处生效。

### D4: 字段块可见性——实例 + 静态两块都加

TYPE 字段块（实例）+ 静态字段块同形,两块每字段都 +visibility。reader read_type 两处对称加。
`TypeDesc` 的 FieldSlot（实例）+ static_fields（FieldDesc）都携 visibility。

### D5: 反射 MethodInfo 可见性来源

`MethodInfo` 由 `builtin_type_methods` 建,方法签名来自 SIGS（`FuncSig`）。FuncSig.visibility →
builtin 塞 `MethodInfo.IsPublic` slot。FieldInfo 同理从 FieldDesc.visibility。

### D6: 默认 public 与 C# 对齐（校正）

z42 默认可见性 = `public`（ExportedTypeExtractor:258 实证,注"镜像 C# AST Visibility 默认"）——
注意这**不同于** C# class 成员默认 private。本 change 忠实沿用 z42 现有语义（默认 public）,不改语义,
只把已有的可见性值 emit 出来。

## Testing Strategy

- z42c 单测:字段/方法 visibility golden（含 private/protected/默认 public）。
- Rust:read_type/read_sigs visibility 往返 + pinned 版本。
- 端到端 + z42.core [Test]:`FieldInfo.IsPublic`（public 字段 true、private false）+ `MethodInfo.IsPublic`。
- 全 GREEN + 自举不动点 + 两代自举 format bump。
- 回归:现有反射/派发不受影响（visibility 是新增读取,不改派发逻辑）。
