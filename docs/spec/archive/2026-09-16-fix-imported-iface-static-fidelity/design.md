# Design: 接口方法 static 保真度

## Architecture

接口方法元数据的完整搬运链（本 change 打通 static 位这一维）：

```
源码 static abstract Self op_Add(...)
  │ ClassDescBuilder._buildIface   ← 填 IfaceMethodStatic[i] = _hasWord(Mods,"static")
  ▼
IrClassDesc.IfaceMethodStatic[]  （新平行数组）
  │ ZbcWriter                     ← 接口方法块每方法写 is_static:u8（pcount 后、ptypes 前）
  ▼
.zbc TYPE 段接口方法块  name:u32 ret:u32 pcount:u8 [is_static:u8] ptype:u32×pc
  ├─ ZbcReader (z42c)             ← 对称读 → cd.IfaceMethodStatic[]
  │    │ TsigReconcile._rebuildInterface  ← ExportedMethodZ(isStatic=真值, isVirtual=!static, isAbstract=true)
  │    ▼  ImportedSymbolLoader:391 已读 mz.IsStatic → MethodSymbol.IsStatic（已就绪）
  │    ▼  InheritanceResolver._checkOneIfaceMethod  ← 删 it.IsImported 守卫，static 校验对导入接口生效
  └─ type_reader.rs (runtime)     ← 对称读 → IfaceMethodSig.is_static（VM 派发不变，供反射铺路）
```

## Decisions

### Decision 1: 承载方式——镜像 SIGS 的 `is_static:u8` 专用字节，而非新造 flags 位图
**问题：** 接口方法块要携带 static 位，用什么编码？
**选项：**
- A — 每方法一个 `is_static:u8`（镜像 SIGS `ZbcWriter:445` 的 `if (f.IsStatic) WriteU8(1) else 0`）
- B — 每方法一个 `method_flags:u8`（bit0=static/bit1=virtual/bit2=abstract），全携带
- C — 复用 SIGS 完整布局（is_static 独立字节 + method_flags 字节 sealed/sret 等）
**决定：** 选 **A**。理由：① 与 SIGS 既有先例逐字对齐（is_static 本就是 SIGS 的独立字节，不在 method_flags 里）；
② 接口方法的 virtual/abstract 可由 static 位无损派生（regular 接口方法 = virtual+abstract；static 成员 =
abstract-only、非虚），无需上 wire；③ sealed/sret 对接口方法无意义（接口方法无 body、不 sret）。B/C 是为
接口方法付它用不到的字节，违反「不为用不到的付费」。

### Decision 2: 位置——`pcount` 之后、`ptypes` 之前
**问题：** is_static 字节插在方法记录哪个位置？
**决定：** `name:u32, ret:u32, pcount:u8, is_static:u8, ptype:u32×pc`。理由：ptypes 是变长尾部，把定长的
is_static 放在变长段之前，reader 游标逻辑最清晰（先读所有定长头再循环读变长参数），与 SIGS「is_static 在
params 前」的顺序一致。strict-pin + 全量 regen ⇒ 不涉旧产物兼容，位置纯看可读性。

### Decision 3: virtual/abstract 派生而非硬编码 true
**问题：** `TsigReconcile._rebuildInterface` 原硬编码 `isStatic=false, isVirtual=true, isAbstract=true`。
**决定：** `isStatic` = wire 真值；`isVirtual = !isStatic`；`isAbstract = true`。理由：静态接口成员经类型参数
见证派发、非 vtable 虚方法 ⇒ isVirtual 应为 false；接口方法一律无 body ⇒ isAbstract 恒 true。当前消费方
（`ImportedSymbolLoader` → 满足性校验）只读 IsStatic，但把 isVirtual 一并修正是「顺手把失真的元数据一次修对」，
零额外成本、零风险。

### Decision 4: 删守卫后导入接口的三项校验都正确、不假红
**问题：** 删 `if (it.IsImported) { return; }` 会不会引入 static 之外的假红？
**决定：** 不会，删。逐项核实：
- **static**：is_static 修好后，`struct Money : INumber` 的 `public static override op_X`（cm.IsStatic=true）
  与导入 `INumber.op_X`（ims.IsStatic 现=true）匹配 → 不报。
- **可见性**：查的是**本地实现方** `cm.Visibility`（可靠），`Money.op_X` 是 `public` → 不报。导入接口成员
  Visibility 在 TsigReconcile 恒 "public"（接口成员本就隐式 public，正确）。
- **返回类型**：`Self` 经 `_substForIface` 替换为实现类（Money），impl 返回 Money ⇒ TypeKey 相等 → retOk。
  未解析返回被 Unknown/Error 分支吸收放行。
删守卫后导入接口获得与本包接口一致的完整满足性校验（对齐 C# CS0535/CS0737）。

## Implementation Notes

- **构造函数元数不变铁律**：`ExportedMethodZ` ctor 签名已含 isStatic/isVirtual/isAbstract（args 4/5/6），
  不改 ctor 元数；`IrClassDesc` 的新平行数组在 ctor 里初始化为 `new int[0]`（安全默认）。
- **runtime 必须消费新字节**：即使 VM 用 vtable 派发不需要 is_static，reader 也**必须读掉**这个字节以保持
  游标对齐（否则后续 struct/inline/object 布局块全部错位）。
- **bit 语义单一**：is_static 是 0/1，非 flags 位图，reader 用 `!= 0` 判布尔。

## Testing Strategy

- **单元测试（z42c）**：`z42c.semantics` 接口满足性单测——阳性（跨包 static-abstract 正确实现放行 +
  故意实现成 instance 报 E0412）；退回对照（删守卫前后精确条数）。
- **Golden/e2e**：`src/tests/cross-zpkg/iface_static_cross_pkg/`——target 声明 static-abstract 接口，
  main 跨包正确实现 + 调用，运行期行为验证（interp+jit）。
- **格式 fixture**：zbc-format 6 + zpkg-format 4 重生（version-bumping.md checklist）；golden hex 重截。
- **Rust 单测**：`zbc_reader_tests` 版本断言 40→41/45→46；reader roundtrip。
- **VM 验证**：完整 `xtask test`（GREEN gate）+ `test bootstrap`（新格式分阶段纪律核对）。
- **格式 bump 的完整 GREEN 以 CI 为准**（macOS 本地两代自举有环境墙）。
