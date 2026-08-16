# Proposal: 未定义类型引用诊断（E0443）

## Why
z42c 的类型检查器对**引用了未定义类型名**的类型注解**静默放行**——把该类型解析成
`Z42UnknownType` 哨兵却不报任何诊断。实测（`z42c --dump-bound`）：

| 未定义类型出现位置 | 现状 |
|---|---|
| 局部变量 `C c;` | `(decl c :<unknown> _)`，**0 error** |
| 字段 `Undef f;` | 静默 `<unknown>` |
| 返回类型 `Undef Ret()` | 静默 `<unknown>` |
| 参数 `void P(Undef a)` | 静默 `<unknown>`（甚至 mangle 进符号名 `P$1$<unknown>`）|
| 对比：未定义**标识符/函数** `badFunc()` | 正确报 E0401 ✅ |

用户在 REPL 里输入 `C c` 不报错即此 bug 的表象（但根因在编译器、非 REPL）。这违反
[philosophy.md「解析失败降级为 sentinel 让下游猜」是反模式] + 让拼错类型名 / 漏 `using` 的错误
无声穿过编译期，直到运行期 `undefined function` 或行为异常才暴露。C# 对应 CS0246
「type or namespace not found」是独立诊断，z42 缺失。

## What Changes
- 新增诊断 `E0443 UndefinedType`：类型引用解析不到已声明类型时报「undefined type: `<name>`」。
- 根因修复：`Z42UnknownType` 携带其**未解析的原始名**（`UnresolvedName`），使诊断能命名类型且
  无需改动 ~15 处 `_chkTypeRef` 调用签名。
- 在既有唯一choke point `AccessChecker.CheckTypeRef`（所有类型引用注解的集中校验点：局部/字段/
  参数/返回/基类/cast/as/is/typeof/default/new/catch/属性/索引器 + 泛型实参递归）里，对
  带名的 `Z42UnknownType` 报 E0443；顺带补 `Z42ArrayType` 元素类型递归（`C[]` 也能捕获）。

## Scope（允许改动的文件）

| 文件路径 | 变更类型 | 说明 |
|---------|---------|------|
| `src/compiler/z42c.core/src/DiagnosticCodes.z42` | MODIFY | 新增 `E0443 UndefinedType` 常量 |
| `src/compiler/z42c.semantics/src/Z42Type.z42` | MODIFY | `Z42UnknownType` 加 `string UnresolvedName`（默认 ""）+ ctor |
| `src/compiler/z42c.semantics/src/SymbolTable.z42` | MODIFY | `ResolveTypeP` NamedType 未解析 fallthrough（line 217）设 `UnresolvedName = nt.Name` |
| `src/compiler/z42c.semantics/src/AccessChecker.z42` | MODIFY | `CheckTypeRef` 对带名 Unknown 报 E0443 + 递归 `Z42ArrayType` 元素 |
| `src/compiler/z42c.semantics/tests/typecheck/undefined_type/` | NEW | 单元测试：局部/字段/参数/返回/泛型实参/数组元素 未定义 → E0443；`var`/泛型形参/合法类型不误报 |
| `docs/book/src/compiler/type-checking.md` 或对应机制页 | MODIFY | 记录未定义类型诊断机制（若无页则新写并挂 SUMMARY） |

**只读引用**：
- `src/compiler/z42c.semantics/src/StmtBinder.z42`、`TypeEnv.z42`、`TypeChecker.z42` — 理解 `var`/
  泛型形参已被过滤（不改）
- `src/compiler/z42c.semantics/src/SymbolCollector.z42` — 确认字段/参数/返回经 `_chkTypeRef`（不改）

## Out of Scope
- REPL 层（Bug 1 已由 PR #200 独立修复；本 change 是编译器侧、不同子系统）。
- `Z42ErrorType`（已报错的级联抑制哨兵）语义不动。
- 命名空间未找到 vs 类型未找到的细分（z42 无 CS0234/CS0246 之分，统一 E0443）。

## Open Questions
- [ ] 错误信息措辞：「undefined type: `C`」 vs C# 风格「type or namespace `C` could not be found」——建议前者，与 E0401「undefined: `x`」对称。
