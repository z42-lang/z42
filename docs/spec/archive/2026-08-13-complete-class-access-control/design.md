# Design: 补全类级访问控制

## Architecture

四项建在 #183/#184 已落的类可见性地基上，**均无格式 bump**（#184 TYPE 可见性字节对每条
TYPE record 无条件写/读，含接口）：

```
① 反射    zbc TYPE vis byte ─(现 discard)→ 改存 ClassDesc.visibility → TypeDesc.visibility
                                            → 6 builtin 谓词 → Type.z42 6 extern 属性
② 不一致  DeclBinder._bindClass → AccessChecker.CheckExposure(成员/基类 rank vs 类型 rank) → E0441
③ 顶层拒绝 Parser.ParseCompilationUnit 分派点 → mods 含 private/protected → E0442
④ 接口可见 Z42InterfaceType.Visibility ──采集(_passInterfaces)──emit(_interfaceDesc)──
           reconcile(_rebuildInterface/ExportedInterfaceZ)──import(ImportedSymbolLoader)
           → CheckTypeRef 加接口分支（复用 #184 已在线字节）
```

commit 顺序（一个 PR，尊重依赖）：**③（parser 自足）→ ④（接口建模+CheckTypeRef）→ ②（依赖类+接口可见性齐全）→ ①（runtime 正交）**。

## Decisions

### Decision 1: 反射谓词全 4 级、对齐 C# 命名，extern 直连 builtin
**问题：** 可见性字节存 0/1/2/3，反射面暴露几个谓词、如何命名。
**选项：** A 仅 `IsPublic`/`IsInternal`（顶层两级）；B 全 4 级对齐 C# `System.Type`。
**决定：** 选 **B**（User 裁决）。谓词集与 C# 一致：`IsPublic`/`IsNotPublic`/`IsNestedPublic`/
`IsNestedPrivate`/`IsNestedFamily`/`IsNestedAssembly`。z42 无 `protected internal` 组合，
故不引入 C# 的 `IsNestedFamORAssem`/`IsNestedFamANDAssem`。`Type` 类体现 104 行（实测），
加 6 个 extern ≈ +12 行 → ~116 行，远低于 200 硬限，**不拆 partial**。
每个谓词一个 builtin，读 `TypeDesc.visibility` + 名内 `+` 判嵌套（与 `__type_is_nested` 同源判据）。

### Decision 2: 不一致可访问性用线性 rank 近似，完整偏序 Deferred
**问题：** C# 的 accessibility-domain 是偏序（protected 与 internal 不可比）。
**选项：** A 完整偏序（含 protected⊕internal 域运算）；B 线性 rank `public>internal>protected>private`。
**决定：** 选 **B**。规则：被暴露类型 rank ≥ 暴露声明 rank，否则 E0441。理由：z42 无组合修饰符，
偏序退化后线性 rank 覆盖全部实用泄漏场景（public 暴露 internal/protected/private、internal 暴露
protected/private）；唯一近似偏差是「internal 成员暴露 protected 类型」按 rank 报错（protected=1 <
internal=2）——语义上 internal 成员模块可见 > protected 的类族可见，确实泄漏，报错**正确**。
非类/接口类型（基元/泛型形参/未知/func）rank 视为 public → 不误报。完整偏序（如未来引入组合
修饰符）列 Deferred。

### Decision 3: 顶层拒绝在 Parser 分派点，走 parser bag
**问题：** `_parseTypeDecl`/`_parseEnum` 被顶层与嵌套（MemberParser）共用，如何只拒顶层。
**选项：** A 在共享的 `_parseTypeDecl` 内加 `isTopLevel` 参数；B 在 `ParseCompilationUnit`
分派点（本就只走顶层）检查 mods。
**决定：** 选 **B**。`ParseCompilationUnit`（Parser.z42:210-224）分派 `_parseTypeDecl`/
`_parseEnum`/`_parseTopLevelFunc` 三入口即顶层全集；在此统一检查 `mods` 含 private/protected →
`_diags.Error(E0442)`。嵌套经 `MemberParser._parseMember` 不经此点，天然豁免。走 parser
`_diags`（`MergeParseDiags` 计入 `SemanticDump.ErrorCount`，单测可见——避开 [[semanticdump-errorcount-skips-collector-diags]] 坑）。

### Decision 4: 接口可见性复用 #184 在线字节，零 bump；CheckTypeRef 加分支
**问题：** 接口是否需要独立 INTERFACE record / 新 bump。
**选项：** A 新 INTERFACE record + bump；B 复用 #184 已在线的 TYPE 可见性字节。
**决定：** 选 **B**。接口经 `ClassDescBuilder._interfaceDesc` 产 `IrClassDesc`（Flags bit4），
走同一 TYPE record；`ZbcWriter` 的 `WriteU8(cd.Visibility)` 无条件执行 → 接口 TYPE 已携带
该字节（此前 `_interfaceDesc` 未设 → 恒 0=public）。本 change 补 6 处接口分支让其写真实值：
`Z42InterfaceType.Visibility`（Z42Type.z42）、`_passInterfaces`（SymbolCollector）、
`_interfaceDesc`（ClassDescBuilder）、`ExportedInterfaceZ.Visibility`（ExportedTypes）、
`_rebuildInterface`（TsigReconcile）、`ImportedSymbolLoader` 接口路径。`CheckTypeRef`
（AccessChecker.z42:71-103）现 L80 `if (!(resolved is Z42ClassType)) return;` 把接口放行 →
改为也接 `Z42InterfaceType`（读其 `.Visibility`，同 private/protected/internal 逻辑；接口无
嵌套外层继承语义则 protected 分支按类族同法处理，`_denyType` 消息 class→"type"/"interface" 化）。

### Decision 5: 无格式 bump ⟹ 不踩 macOS 两代自举墙
**问题：** 是否需 CI fixture 重生周旋（[[escape-stack-format-bump-ci-learnings]]）。
**决定：** 四项 zbc/zpkg 格式**逐字节不变**（#184 已 bump 到 1.33/0.38，本 change 只填既有字节位
的真实值 + runtime 内部结构字段）。故 warm 自举 gen1==gen2 应保持（接口从恒 0 → 真实值会改变
含 internal 接口的 zpkg 字节，但那是**产物内容**非格式；自举链只要种子能编当前源即收敛）。
本地只需一套 0.38 warm 种子即可全程验证，无需两代自举 / CI 重生。

## Implementation Notes

- **诊断码**：`E0441 InconsistentAccessibility`、`E0442 TopLevelAccessModifier`（E0440 为现最高；E0438 预留注释保留）。
- **rank helper**（AccessChecker）：`_visRank(string vis) -> int`（public 3/internal 2/protected 1/private 0）；被暴露类型取可见性：`Z42ClassType`→`.Visibility`、`Z42InterfaceType`→`.Visibility`、`Z42InstantiatedType`→递归 Def（与 CheckTypeRef 同法）、其余→"public"。
- **CheckExposure 签名**：`static void CheckExposure(string declVis, Z42Type exposed, string ctx, SymbolTable symbols, DiagnosticBag diags, Span sp)`；`DeclBinder._bindClass` 对类的 base/ifaces + 每个 FieldSymbol/MethodSymbol（含 ret+params）调用；类声明本身的 vis 取 `IrGenFacts.classVis(c.Mods, isNested)`。
- **反射 nested 判定**：builtin 内以 TypeDesc 的 FQ 名含 `+` 判定（与既有 `__type_is_nested` 一致），避免额外 nested 字段。
- **接口 protected 语义**：接口继承链的 protected 引用较少见；`CheckTypeRef` 接口分支复用 `_derivesFromOrEq`（若接口实现方在类族内）。v1 与类同法，边角 case 若过严再收窄。

## Testing Strategy
- **单元（Rust）**：`reflection_tests.rs` 加 `type_visibility_decode_*`（构造带 vis 字节的 TYPE record → 谓词）。
- **Golden（z42 e2e）**：`src/tests/types/type_visibility.z42`（顶层 public/internal + 嵌套四级 → 6 谓词断言）。
- **编译器单测**：`access_control_tests.z42` 加 E0441（②各暴露点正反）+ E0442（③顶层各 decl 类别正反）用例，`SemanticDump.FirstErrorCode` 断言。
- **跨包 e2e**：`src/tests/cross-zpkg/interface_internal_access/`（跨包 internal 接口引用 → E0404）。
- **GREEN**：`xtask test`（完整 gate：e2e / cross-zpkg / stdlib / compiler 自举 / vscode-syntax）；自举 gen1==gen2 byte-identical 保持（无格式 bump）。
- **bootstrap 边界**：`xtask test bootstrap`（本 change 不加新语法/格式 → 上一 nightly z42c 仍能编当前源，应过）。
