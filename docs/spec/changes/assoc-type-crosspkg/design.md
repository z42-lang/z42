# Design: 跨包关联类型

## Architecture

三份数据的搬运链（本 change 打通）：

```
接口 type Item;   → Z42InterfaceType.AssocTypeNames  → IrClassDesc.AssocNames (type="")  ┐
类   type Item=int → Z42ClassType.AssocBinding*       → IrClassDesc.AssocNames/Types      ├─ TYPE 记录 assoc 块（统一）
where T:I<Item=int>→ ConstraintBundle.AssocBinding*   → IrConstraintDesc.AssocBinding*     ─ 约束 bundle bit7
   │ ZbcWriter 写 → .zbc → 三方 reader（ZbcReader/type_reader.rs/ZpkgReader）
   │ TsigReconcile._rebuild{Class,Interface} → ExportedInterfaceZ.AssocTypeNames / IrConstraintDesc
   ▼ ImportedSymbolLoader → 导入 Z42InterfaceType.AddAssocType / Z42ClassType.AddAssocBinding / bundle.AddAssocBinding
   ▼ 删三守卫（ConstraintChecker:169/416, InheritanceResolver:360）→ 跨包完整校验
```

## Decisions

### Decision 1: TYPE 统一 assoc 块（接口名单 + 类侧绑定合一），而非两块
**问题**：接口的 AssocTypeNames（names）与类的 AssocBinding（name→type）是两份不同数据，各占 TYPE 块？
**决定**：**合一**。TYPE 记录尾部一个 always-present 块 `assoc_count:u16 + (name_idx:u32, type_idx:u32)×n`——
接口写 `(Item, "")`、类写 `(Item, int)`。消费端按记录是接口还是类路由（`class_flags & CLASS_FLAG_INTERFACE`）：
接口 → `AddAssocType(name)`（type 忽略）、类 → `AddAssocBinding(name, type)`。理由：① 一个 reader 分支 vs 两个；
② 接口/类互斥（接口声明名、类给绑定），同一 slot 复用零歧义；③ 位置固定（记录尾部）reader 游标最简。
count=0 时仅 +2 字节（绝大多数类型无 assoc），双 bump 已让旧产物全失效、+2 字节可接受。

### Decision 2: 约束 assoc 走 bit7（不复用 TYPE 块）
**问题**：`where` 约束的绑定放哪？
**决定**：约束 bundle **bit7**（`assoc_count:u8 + (name,type)×n`，在 iface 列表之后）。理由：约束是**型参**属性
（`where T:...`），天然属约束 bundle；bit7 是 bundle 里唯一空位（bit0-6 全占，三方 reader 实测未触 bit7）。
与 TYPE 块（类型自身属性）正交。

### Decision 3: runtime 只消费字节保游标对齐
`validate_type_arg_constraint`（generics.rs）无关联类型分支、`ConstraintBundle`（class.rs）无 assoc 字段——
关联类型是纯编译期概念。runtime 三方 reader（type_reader.rs）读 bit7 载荷 + TYPE assoc 块**读而不存**（照搬
bit6 funcSig 的处理），仅保游标对齐。ZpkgReader（z42c 侧第三 reader）同样只 skip 保对齐。

### Decision 4: 删三守卫的安全性
- 守卫 1（ConstraintChecker:169，声明点 HasAssocType）：接口 AssocTypeNames 到位后可删。
- 守卫 2（ConstraintChecker:416，使用点 AssocBindingOf）：类 AssocBinding 到位后可删。
- 守卫 3（InheritanceResolver:360，补齐强制）：wire 未到位时也只空转不假红；到位后删，恢复跨包补齐强制。
三守卫随 wire 一起删，恢复完整跨包判别力。

## Implementation Notes

- **构造函数元数不变铁律**：IrConstraintDesc/IrClassDesc 新字段在 ctor 初始化为空数组/0；ExportedInterfaceZ
  新字段构造后赋值（不改 ctor 元数，同 B1 的 IfaceMethodStatic）。
- **三方 reader 必对称消费**：漏一个 reader 的 bit7/assoc 块 → 游标错位 → 后续段全错。ZpkgReader（最易漏）必改。
- **TYPE assoc 块位置**：class TYPE 记录**最尾部**（object 块之后），reader 顺序读到即消费。

## Testing Strategy

- 单元（z42c semantics）：跨包关联类型满足性——正例放行 + 负例（错绑定/缺绑定）报 E0453 + 退回对照（删守卫前后条数）。
- e2e：`assoc_type_cross_pkg` 升级（正例 7/9/11 + 新负例 build error）。
- 格式 fixture：zbc-format 6 + zpkg-format 4 + golden hex（双 bump）。
- Rust：版本 pin 42/47 + reader roundtrip。
- 格式 bump 完整 GREEN 以 CI 为准（macOS 本地两代自举墙；fixture 走「下载 CI 工具链本地重生」配方，同 B1）。
