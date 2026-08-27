# Design: 模式匹配 D —— `with` 表达式（record 非破坏式更新）

## 数据流（纯编译期脱糖，收口既有 BoundSeqExpr）

```
源码 `p with { Y = 99 }`
  → 词法：`with` 关键字（TokenKind.With=154，末尾追加不入 zbc）
  → 解析：ExprParser 后缀分支（bp 85，仿 switch）→ _parseWithBody → WithExpr { Target; FieldNames[]; FieldValues[] }
  → 绑定：ExprTyper dispatch → ConstructTyper._bindWith
          → 脱糖 `$t = p; new Point($t.X, 99)` → BoundSeqExpr（无新 Bound 节点、无新 opcode、无新 emitter）
  → 发射：既有 BoundSeqExpr 发射器（prelude 语句 + ObjNewInstr）
```

## 解析：后缀表达式（绑定力 85）

`with` 接入 `_parseExpr` 的后缀循环，与 `switch` 后缀表达式同绑定力 85：
`if (k == TokenKind.With && minBp <= 85) { advance; left = _parseWithBody(left, wsp); }`。
`_parseWithBody` 复用对象初始化器的字段体文法（`F = v` / 简写 `x ≡ x=x` / 尾逗号；`..base` defer 报错），
产出 `WithExpr(target, names, values, count)`。

## 绑定：`_bindWith`（脱糖为 BoundSeqExpr）

模板 = `ConstructTyper._bindObjInit`（对象初始化器脱糖）。步骤：

1. 绑 `target` 求其类型；校验 **record class**（`IsRecord && !IsStruct`），否则诊断 + `BoundError`。
2. 校验每个覆盖字段名 ∈ record 主构造器字段（`OwnFieldNames`），否则诊断「no field」。
3. **temp 求值一次**：`pre[0] = BindStmt($t = target)`（`VarDeclStmt(tyExpr, $t, target)`）——避非破坏式
   更新时 target 副作用被多次求值。
4. **按主构造器声明序造 ctor 实参**：逐 `OwnFieldNames[i]`——覆盖集命中 → 用 `with` 里的值表达式；
   否则 → `MemberExpr($t, fieldName)` 读原对象字段。
5. `val = _bindExpr(ObjNewExpr(tyExpr, ctorArgs, n))` → `new T(...)`；返回 `BoundSeqExpr(pre, 1, val, val.Type())`。

**为何读字段作 ctor 实参、非「new 默认 + 逐字段 set」**：record 主构造器字段常 readonly/init-only，
`new` 后逐字段赋值会撞可赋值性校验（E0415）。走 ctor 实参既避只读、又天然产不可变新值。

### 生成 IR（实测正确）

```
p with { Y = 99 }  →
  %3 = obj_new Point(1, 2)      // p
  %5 = field_get %3.X           // $t.X = 1（未覆盖 → 读原）
  %6 = const.i64 99             // Y 覆盖值
  %8 = obj_new Point(%5, %6)    // new Point(1, 99) = q
```

## 词法 / grammar / 自举

- **无 zbc/zpkg 格式 bump**：`with` token 末尾追加不入 zbc；纯脱糖复用 `ObjNewInstr` / `FieldGetInstr`，
  无新 opcode、无新 class-flag。
- **新关键字 `with`**：`_kwOperatorExpr()`（同 `is`/`as`）加 `with` + 重生成 `z42.tmLanguage.json`
  （`test compiler` 的 vscode-syntax 一致性 gate 会校验 Lexer 关键字 ↔ grammar 分类，漏归类 → ghost 报错）。
- **两-nightly 纪律**：`with` 只在 e2e 测试文件用；z42c / stdlib / xtask 源不使用（`with` 当前零标识符
  冲突，14 处全注释/诊断字符串）→ 上一 nightly 的 z42c 仍能编当前源。
- **自举字节不动点**：z42c 源无 `with` 用法 + 纯脱糖不产新 Bound 类型（`WithExpr` 只在 semantics 内被
  消费、下沉到既有 `BoundSeqExpr`）→ gen1==gen2 天然成立，且减少 syntax→semantics 冷启动 stale-cache 风险。

## 限制（初版 defer）

- 仅 record class；struct record `with`、`..base` 结构更新、基类字段 `with`、`init`-only 访问器为后续特性。
