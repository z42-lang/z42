# Design: readonly 字段 + readonly-load 优化

## Architecture

readonly 信息的数据通路（**同模块**，不经 zbc）：

```
FieldDecl.Mods("readonly")
  └─(SymbolCollector)→ FieldSymbol.IsReadonly            [内存符号表]
       └─(ExprEmitter._emitMember 查接收者类型的 FieldSymbol)
            └─→ FieldGetInstr.Readonly = true            [内存 IR 标志，不序列化]
                 └─(IrOptPipeline / IrLicm 读该标志)
                      ├─ CSE: 同(obj,field) readonly 读消重
                      └─ LICM: this 接收者的读外提循环
                 └─(ZbcWriter 序列化时忽略 Readonly)→ 普通 field_get 字节
```

强制不变性（soundness 前提）：

```
ExprTyper._bindAssign(a):
  若 a.Target 解析到 FieldSymbol.IsReadonly:
     若 !(env.InCtor && 接收者是 this && 字段属当前类)  → Diag E0415
  （字段初始化器注入的赋值走 ctor 语境，豁免）
```

## Decisions

### Decision 1: readonly 用内存 IR 标志而非 zbc 元数据
**问题：** 优化器要知道 FieldGet 读的是 readonly 字段。
**选项：** A — `FieldGetInstr` 加内存 bool（优化在序列化前消费）；B — zbc 存字段 readonly 位（需格式 bump + 跨包）。
**决定：** 选 **A**。优化管线（IrOptPipeline/IrLicm）在 ZbcWriter 之前跑，readonly 只在这段内存 IR 生命周期需要；序列化后 field_get 字节不变 → **无格式 bump、无两代自举、无版本偏差风险**。代价：imported 字段（跨 zpkg）拿不到 readonly（FieldSymbol.IsReadonly 对导入符号为 false）→ 保守当非 readonly。这是 A 的唯一取舍，且是安全的（保守 = 不优化，不会错优化）。跨包 readonly 留 Deferred（届时走 B）。

### Decision 2: LICM 仅外提接收者 = this 的读
**问题：** 把 FieldGet 提到 pre-header，若循环 0 次执行且 obj 为 null，NPE 会被提前触发 → 可观测行为漂移。
**选项：** A — 仅提 `this.<field>`（this 恒非空）；B — 加非空/支配分析支持 params/locals。
**决定：** 选 **A**。`this`（实例方法接收者寄存器）在方法体内恒非空（调用点已解引用），提其 readonly 字段读到 pre-header **无 NPE 时机风险**。覆盖头号热点（方法在循环里反复读自己的 readonly 字段）。B 工作量大（需流敏感非空），Deferred。判定"Obj 是 this"：函数是实例方法且 `Obj.Id == 接收者寄存器 id`（见 Implementation Notes 确认约定）。

### Decision 3: CSE 遇同字段 FieldSet 失效值号
**问题：** ctor 内 `this.x=1; r=this.x; this.x=2; s=this.x`，r≠s，不能合并。
**决定：** 块内 value-numbering 扫描时，遇到 `FieldSetInstr`（字段 F）就从 `seen` 表删除所有 `fget|*|F` 键。readonly 字段实际只在 ctor 被写，此举正确处理 ctor；非 ctor 方法里 readonly 字段无 FieldSet → 全程有效。

### Decision 4: 独立 OptSet 位 ReadonlyLoad
**问题：** bench 要前后对比，且 readonly-load 与通用 CSE/LICM 语义不同（走独立分支）。
**决定：** 加 `ReadonlyLoad = 256`（bit8），`All = 511`。独立位让 `--opt -readonly-load` 一键关掉做 A/B 计时，且不污染现有 Cse/Licm 位的语义。

## Implementation Notes（精确接入点，来自源码调研 @ origin/main 5490286c）

### 语法
- `TokenKind.z42:68` 后加 `public static int Readonly = 150;`（值仅需互异，不入 zbc）。
- `Lexer.z42:436 _initKeywords()` 加 `this._kw("readonly", TokenKind.Readonly);`。
- `DeclParser.z42:97 _isModifier()` 加 `if (k == TokenKind.Readonly) { return true; }`。`_parseModifiers()` 无需改（自动进 `FieldDecl.Mods`）。判定用现成 `_modsHas(mods, "readonly")`。

### 符号 + 类型检查
- `Symbol.z42:29 FieldSymbol`：加 `public bool IsReadonly;`，构造签名加参数。
- `SymbolCollector.z42:587/:599`：本地字段构造 FieldSymbol 时传 `_modsHas(fd.Mods, "readonly")`。
- `ImportedSymbolLoader.z42:256`：导入字段 **传 false**（v1 不跨包）。
- ctor 上下文：`TypeEnv.z42` 加 `bool InCtorThis`（当前在声明类 ctor 体、接收者 this）。`DeclBinder.z42`（ctor 体绑定处 `:52/:64/:182/:206`）绑 ctor 体时置 true。字段初始化器注入路径（`TypeChecker.z42:140-141` 描述）也置 true（等价 ctor）。
- `ExprTyper.z42:61 _bindAssign`：target 是 `MemberExpr`（`this.x`）或 `IdentExpr`（裸 `x` 解析到字段）时，查接收者类型 `Z42ClassType.Fields.Get(name) as FieldSymbol`（套路见 `ExprTyper.z42:127`）；若 `IsReadonly && !(env.InCtorThis && 接收者是 this && 字段属当前类)` → `_diags.Error(DiagnosticCodes.ReadonlyAssignment, ...)`。不脱轨（累积诊断继续）。
- `DiagnosticCodes.z42:39` 后加 `public static string ReadonlyAssignment = "E0415";`（E0415–E0419 空闲）。

### IR + emit
- `IrInstr.z42:313 FieldGetInstr`：加 `public bool Readonly;`，构造默认 false（保留旧构造点兼容——加带默认或重载）。**ZbcWriter/ZbcReader 的 field_get 编码不动**（Readonly 不写）。
- `ExprEmitter.z42:695 _emitMember`：已有 `Z42ClassType propCt`（`:689`）→ `FieldSymbol fs = propCt.Fields.Get(m.MemberName) as FieldSymbol; bool ro = fs != null && fs.IsReadonly;` → `new FieldGetInstr(dst, obj, m.MemberName, ro)`。裸 this 字段读 `_lookupIdent`（`:302`）同款。

### 优化管线
- `OptSet.z42:23` 加 `public static int ReadonlyLoad = 256;`；`:25 All = 511`；`_optFromName`（`:34-42`）加 `if (name == "readonly-load") { return Opt.ReadonlyLoad; }`。
- `IrOptInfo.z42:239 CseKey`：加分支
  `if (ins is FieldGetInstr) { FieldGetInstr fg = ins as FieldGetInstr; if (fg.Readonly && _stable(defs, paramCount, fg.Obj.Id)) return "fget|" + fg.Obj.Id + "|" + fg.Field; return null; }`。
- `IrOptPipeline.z42:270 _passCse`：门控加 `ReadonlyLoad`（该 pass 现由 `Cse` 位控——readonly 分支额外要求 `ReadonlyLoad` 开）；扫描到 `FieldSetInstr(F)` 时删 `seen` 里所有 `fget|*|F` 键（失效）。
- `IrLicm.z42`：现有不变量循环（`:72-78`）之外，加 readonly-FieldGet 外提分支：`ins is FieldGetInstr && fg.Readonly && ReadonlyLoad 开 && Obj 循环外定义 && _isThisReceiver(f, fg.Obj) && 循环体内无 field_set 到该字段` → 提到 pre-header。`_isThisReceiver`：f 是实例方法且 Obj.Id == 接收者寄存器 id（实现时确认 IrFunc 的接收者标识；若约定 %0=this 则 `Obj.Id==0 && f.IsInstance`）。

> ⚠️ 实现期确认点：① 实例方法接收者寄存器约定（是否恒 %0）；② FieldGetInstr 现有构造点数量（加参数需全改或用默认）；③ CSE pass 当前是否已 block-local 且能拿到 FieldSet 事件流。这些在实施 2.x 时逐一核对，若与假设不符停下调整。

## Testing Strategy
- **codegen 单测**（`codegen_tests.z42`，仿 `:554-559` CSE / `:542-550` LICM 结构）：
  - readonly 两次读：`Opt.None` 两条 field_get vs `Opt.ReadonlyLoad` 一条。
  - 非 readonly 两次读：`Opt.ReadonlyLoad` 仍两条。
  - ctor 内 set/get/set/get：不误合并。
  - this 循环读：`Opt.ReadonlyLoad` field_get 在 pre-header；非 this：仍在体内。
- **typecheck 单测**（`typecheck_tests.z42`）：ctor 内合法 / 初始化器合法 / 方法内 E0415 / 跨对象 E0415 / 不脱轨。
- **运行时 golden**（`src/tests/optimization/readonly-field-hoist/`）：readonly 字段热循环，验优化后**结果不变**（正确性）。
- **bench**（`readonly_field_bench.z42`）：readonly 字段热循环，`[Benchmark]`。A/B：`--opt -readonly-load`（前）vs 默认（后），`z42b bench` 比时间，数字记 PR。
- **VM 验证**：`xtask test` 全 GREEN gate（含自举字节不动点——z42c/stdlib 不用 readonly，输出不变）。

## Deferred（登记 roadmap Deferred Backlog Index）
- **readonly-future-cross-zpkg**：imported 字段 readonly（需 zbc/zpkg 格式 bump + IrFieldDesc.Readonly + ZbcWriter/Reader + TsigReconcile + ImportedSymbolLoader）。触发：跨包 readonly 成为热点瓶颈。
- **readonly-future-nonthis-licm**：非 this 接收者（params/locals）的 readonly FieldGet 外提（需非空/支配分析）。触发：非空类型系统落地后。
