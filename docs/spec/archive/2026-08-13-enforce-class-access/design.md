# Design: 类级访问强制

> **拆分（2026-08-13）**：本 change 落 **①同包 private/protected 嵌套类引用强制**（D1–D4，无格式 bump，
> 本地完整 GREEN）。**②跨包 internal 类**（D5/D6 = 类可见性进 zbc/zpkg 元数据 = 格式 bump）因撞 macOS 两代
> 自举墙拆为 follow-up `enforce-crosspkg-internal-class`。**D5/D6 及其代码留作该 follow-up 的权威设计**
> （已实现、已存 patch），本 change 不含。下文 D5/D6 保留供 follow-up 承接。

## Architecture

复用成员级强制的**同款 mediator 模式**：`AccessChecker`（持 `TypeChecker` 反向引用取 `_diags`）新增一个
类级校验入口，绑定期在类型引用点被调用。类可见性沿「本地由 collector 从 `Mods` 设 / 跨包由元数据还原」
两条腿上 `Z42ClassType.Visibility`。

```
声明期  ClassDecl.Mods ──(位置默认)──> Z42ClassType.Visibility        [本地]
                       └─> IrClassDesc.Visibility ─> zbc TYPE 字节 ─> TsigReconcile ─> ExportedClassZ.Visibility
                                                                                        └─> ImportedSymbolLoader ─> Z42ClassType.Visibility  [跨包]
引用期  绑定: t = _resolveTypeChecked(te, env) = env.ResolveType(te);
              AccessChecker.CheckTypeRef(t, env.CurrentClass(), env.Symbols, _tc._diags, te.Span)
        收集: t = symbols.ResolveTypeP(field/param/ret te);
              AccessChecker.CheckTypeRef(t, c.Name, symbols, coll.Diags, te.Span)   // 声明签名位置
                                        └─ private/protected: currentClass + 嵌套 Outer + 基链
                                        └─ internal: t.IsImported && t.Visibility=="internal"  → E0404
```

## Decisions

### Decision 1: 强制点 = 绑定期「checked resolve」包装，不改 `TypeEnv.ResolveType`

**问题：** 类型引用点散布 ~15 类（var/param/new/cast/is/as/typeof/static/catch/泛型实参…）。在哪里挂钩？

**选项：**
- A — 在中心解析器 `TypeEnv.ResolveType` 内直接校验。**否决**：`TypeEnv` 是纯解析助手、无 `_diags`；且
  `ResolveType` 在收集期（`SymbolCollector`）、绑定期、codegen（`FunctionEmitter` 直呼 `symbols.ResolveTypeP`）
  多相调用，就地 emit 会重复报 / 相位错乱。给 `TypeEnv` 塞 diags 侵入其所有构造点，得不偿失。
- B — **在类型引用调用点解析后即校验**（选它）。绑定期：`TypeChecker._resolveTypeChecked(te, env)` =
  `t = env.ResolveType(te); AccessChecker.CheckTypeRef(t, env.CurrentClass(), env.Symbols, _tc._diags, te.Span); return t;`，
  把 binder 里 `env.ResolveType(te)` 换成它。收集期：`SymbolCollector` 在字段/签名类型解析后、基类解析后调
  同一静态 `CheckTypeRef`（D2/D2b）。**单一校验逻辑点**、相位精确、零 `TypeEnv` 改动，与成员级
  `MemberResolver`/`ExprTyper` 解析后调 `CheckAccess` 同构。

**决定：** 选 B。关键事实（探索确认）：**codegen（`FunctionEmitter`）走 `symbols.ResolveTypeP` 直呼、
绕过校验入口** → 不重复报、不碰字节不动点；绑定期体引用 + 收集期声明位置合起来即 C# 全覆盖。

### Decision 2: v1 全覆盖——体引用（绑定期）+ 声明签名位置（收集期）【User 裁决 2026-08-13】

**问题：** 字段类型 / 参数·返回类型 / 基类·接口列表也是类型引用，是否 v1 覆盖？

**事实：** 这些在**收集期**解析——字段/签名类型经 `SymbolCollector._fillClass` 的 `symbols.ResolveTypeP`
（非 `env`），基类/接口是 `SymbolCollector` 存的**字符串**（`c.Bases[b].Dump()`），均不经绑定期
`env.ResolveType`。

**决定：** **v1 全覆盖**（User 于 6.5 gate 裁决）。C# 忠实：类型只要被「命名」就受可访问性约束，不论出现在体
还是签名。除绑定期体引用外，加**收集期声明位置** hook：
- **字段 / 参数 / 返回类型**：`SymbolCollector._fillClass` 解析出类型后即校验（当前类上下文 = 正在填充的 `c`）。
- **基类 / 接口列表**：`SymbolCollector` 处理 `c.Bases` 时把基名解析成类再校验；解析不出（泛型/接口/歧义）
  即跳过（绝不误报）。

**双相位无重复报**：字段/签名类型只在收集期解析一次（绑定期的 `obj.f` 用已解析的 field 符号类型、不重解
TypeExpr）；体引用只在绑定期。不同 TypeExpr → 不双报。

### Decision 2b: `CheckTypeRef` 设计为静态 + 显式 diag bag（跨两相位复用）

**问题：** 收集期在 `SymbolCollector`（emit 到其自己的 `Diags`），绑定期在 binder（emit 到 `_tc._diags`）——
同一校验逻辑要服务两个不同 diag bag。

**决定：** `CheckTypeRef` 做成 **`AccessChecker` 的静态方法**，显式收 diag bag：

```
static void CheckTypeRef(Z42Type resolved, string currentClass, SymbolTable symbols,
                         DiagnosticBag diags, Span sp)
```

- 绑定期：`AccessChecker.CheckTypeRef(t, env.CurrentClass(), env.Symbols, this._tc._diags, te.Span)`
  （经 `TypeChecker._resolveTypeChecked` 包装）。
- 收集期：`AccessChecker.CheckTypeRef(t, c.Name, symbols, this.Diags, te.Span)`。

`_nestedOuter` / 派生判定（原成员级 `_currentDerivesFrom` 逻辑复刻为取 `currentClass` string 的静态版）随之
静态化。成员级 `CheckAccess`（实例、emit `_tc._diags`）不动。

### Decision 3: 复用 E0404，不新增诊断码

**问题：** 类型不可访问用 `E0404 AccessViolation` 还是新码 `E0441`？

**决定：** 复用 **E0404**。C# 对成员与类型不可访问统一用 CS0122「is inaccessible due to its protection
level」；z42 的 `E0404 AccessViolation` 语义即「访问违规」，天然覆盖两者。消息按 `kind="class"` 区分。
`DiagnosticCodes.z42` 注释补一行说明覆盖类型引用。

### Decision 4: 嵌套类可访问性判据 = `+` 名结构，无需新结构标记

**问题：** `Z42ClassType` 无「是否嵌套 / 外层是谁」字段（探索确认）。要不要加结构标记？

**事实：** `NestedFlatten` 已把嵌套类命名为扁平键 `Outer+Inner`（任意深 `A+B+C`）；嵌套性纯由名字含 `+`
推断，`TypeEnv._stripLastNestSeg` 已有「剥最后一个 `+` 段」逻辑。

**决定：** 不加结构字段。`CheckTypeRef` 对 private/protected 嵌套类：
- `outer = _nestedOuter(declName)`（剥 `+` 末段；无 `+` → 顶层类，private/protected 顶层类判为「不可
  从任何外部引用」，但破坏面为 0 且属声明合法性 Deferred，见 Out of Scope）。
- **private**：`cur = CurrentClass()`；允许 iff `cur == outer` 或 `cur` 以 `outer + "+"` 为前缀（`Outer` 内层
  更深嵌套仍算 `Outer` 文本内）。
- **protected**：`cur` 或其基链任一 `== outer`（复用 `AccessChecker._currentDerivesFrom` 同款上溯）。

### Decision 5: 类可见性元数据 = TYPE 记录新增 `Visibility` 字节（真格式 bump）

**问题：** 跨包 internal 类判定需 importer 知被引类声明可见性。载体？

**事实：** ① 成员 internal=3 曾**零 bump**——因成员 `Visibility` u8 字段**早已存在**，只加值 3。② 类级**无**
任何现成可见性载体：`ExportedClassZ` 无 Visibility 字段；zbc TYPE 的 `class_flags` 是**已满 u8**（bit0–7 全占，
bit7=inline-struct 注释明写「last free bit」）—— **塞不下**可见性位。

**选项：** A — 把 `class_flags` u8 拓宽为 u16 用 bit8–9；B — TYPE 记录新增独立 `Visibility` 字节（镜像成员）。

**决定：** 选 **B**（独立字节）。理由：与成员 Visibility 同构（成员用独立 int 而非塞 flags）、语义清晰
（可见性非 shape-flag）、拓宽 u8→u16 反而牵动更多字节。**这是真格式 bump**（zbc 1.32→1.33 / zpkg 0.37→0.38），
按 [version-bumping.md](../../../.claude/rules/version-bumping.md) 全套步骤；与成员 internal 的零 bump **不同**，
不可类比。

**为何仍不破自举字节不动点：** 所有现存导出类均 `public`（Visibility=0）；新增字节对每个 TYPE 记录尾追加一个
`0` → z42c 自编译 gen1/gen2 同样追加、逐字节仍相等。fixture（含非导出/默认类）按格式 bump 常规重生。CI
`ci-bootstrap` 版本差 gate → 两代自举吸收（与近期多次真实 bump 同路径）。

### Decision 6: Rust VM 消费新字节但不上反射面

**问题：** VM 是否需要类可见性？

**决定：** VM **必须**读这个新字节（保持 TYPE 记录后续字段偏移正确），但 v1 **不**接入反射（`Type.IsPublic`
等类级可见性反射列 Deferred，避免范围蔓延）。`bytecode.rs` 加 `class_visibility: u8` 字段、`zbc_reader.rs`
读它并 bump 两个版本常量；`loader.rs` 视需要 thread 进 `TypeDesc`（若不接反射可仅读弃）。

## Implementation Notes

- `CheckTypeRef` 入参先 `t is Z42ClassType` 过滤（prim/接口/泛型形参/未知类型直接放行，绝不误报）。
- **位置默认**（可见性）与成员一致：类名含 `+` → 默认 `private`；顶层 → 默认 `internal`；显式修饰符优先。
  `SymbolCollector._putClassStub` 与 `ClassDescBuilder`/`ExportedTypeExtractor` 三处默认逻辑须一致——提取一个
  共用小助手 `_classVisDefault(mods, isNested)` 避免漂移（放 `IrGenFacts` 或就近静态方法）。
- `internal` 判定：`t.IsImported && t.Visibility == "internal"` → deny(cross-pkg)。本地 internal（`!IsImported`）
  一律放行（同包）。这与成员级 `CheckAccess` 的 internal 分支同构。
- `TsigReconcile` 还原：`ecz.Visibility = _classVisStr(cd.Visibility)`（0→"public"…3→"internal"），镜像
  `_visStr`；缺字段（不会发生，strict-pin 保证同版本）默认 "public"（最保守、绝不误拒）。
- **不动点自检**：实施后跑 `xtask test compiler` 确认 gen1==gen2；任何字节漂移必是 public 类误 emit 非 0
  可见性 → 查默认逻辑。

## Testing Strategy

- **单元（access-control）**：private/protected 嵌套类越界引用→E0404、同类/派生类内引用 OK、public 嵌套类
  OK、顶层 internal 同包 OK；每种引用点（new/var/cast/is/as）至少一例。断言走 `coll.Diags` +
  `hasCode(E0404)`（注意 `SemanticDump.ErrorCount` 跳过 collector diags，见 memory
  `semanticdump-errorcount-skips-collector-diags`）。
- **跨包 e2e**：`src/tests/cross-zpkg/class-internal-access/`——包 B 引用包 A 的 internal 类，期望编译错误。
  若 cross-zpkg harness 无 expected-compile-error 模式（成员级即如此），则跨包 internal **逻辑**由单元覆盖
  （构造 imported `Z42ClassType` + Visibility），端到端手工验证并记录；stdlib/z42c 全量构建即海量跨包 public
  回归门。
- **zbc golden**：`zbc_tests.z42` 的 `empty/source.zbc` hex 重截（minor + TYPE 尾字节）。
- **格式 bump 自检**：`xtask build compiler && xtask build stdlib && xtask build test`（zbc fixture 重生）；
  `cargo test --test zbc_compat` / `cargo test lazy_loader`（Rust reader 读重生基线）；zpkg fixture 手工重生。
- **GREEN**：完整 `xtask test`（含 cross-zpkg / stdlib / compiler / vscode-syntax）+ `cargo test --lib`
  （runtime 改了 → 必跑，见 memory `xtask-test-excludes-cargo-test`）。
