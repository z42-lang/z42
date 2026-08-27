# Proposal: 模式匹配 D —— `with` 表达式（record 非破坏式更新）

## Why

record 值语义程序（#300）让 record 成为不可变数据载体（值相等 + 记录式 ToString）。但**基于旧值造新值**
（改一两个字段）目前要手写全部字段的 `new`：

```z42
[Record] class Point(int X, int Y);
Point p = Point(1, 2);
Point q = Point(p.X, 99);   // 只想改 Y，却要重复 p.X
```

`with` 表达式是 Rust `Point { y: 99, ..p }` / C# `p with { Y = 99 }` 的对应物——**非破坏式更新**：产出一个
新 record，除指定字段外逐字段拷贝原值。这是 record（不可变积类型）消费侧的头号人体工学特性。

```z42
Point q = p with { Y = 99 };   // q = Point(1, 99)，p 不变
```

## What Changes

新增后缀表达式 `<expr> with { F = v, ... }`，**纯编译期脱糖**成「读原对象各字段作主构造器实参 + 覆盖指定
字段」，收口进既有 `BoundSeqExpr`（无新 Bound 节点、无新 opcode、无新 emitter）。模板 = 既有对象初始化器
`ObjInitExpr` 的脱糖（`ConstructTyper._bindObjInit`，产 `BoundSeqExpr`）。

### 脱糖策略——核心设计点

探查发现 record 字段可能 **readonly / init-only**（`Readonly` token 存在，record 主构造器字段常只读）。
故**不能**走「`new` 默认 + 逐字段 `set`」（会撞可赋值性校验 `E0415`）。正确策略：**读原字段作主构造器实参，
覆盖项替换对应实参**：

```
p with { Y = 99 }
  脱糖为：new Point( p.X, /*Y=*/99 )        // 主构造器声明序：X 取 p.X，Y 取覆盖值
```

- record 位置字段 ↔ 主构造器形参一一对应（声明序 `OwnFieldNames`）。逐字段：若在 `with { }` 覆盖集中 →
  取覆盖表达式，否则 → `MemberExpr(orig, fieldName)` 读原对象字段。
- 字段读取用 `field_get` 直读（复用 RecordSynth 的字段遍历范式，`RecordSynth._collect` 基类在前声明序）。
- 结果是一条 `new`（`ObjNewInstr`）——干净、避开 readonly 赋值问题、天然不可变。

| 维度 | 说明 |
|------|------|
| 适用类型 | **record class**（`IsRecord && !IsStruct`）；非 record → 诊断 `E0xxx: with requires a record type`。struct record defer |
| 覆盖字段 | 必须是该 record 的**主构造器字段**（`OwnFieldNames` 成员）；非字段名 → 诊断。初版不支持继承基类字段的 `with`（若 record 有基类，defer） |
| 求值顺序 | 先求 `orig`（一次，绑临时避免重复求值）→ 各 ctor 实参按声明序（覆盖项用 `with` 里的表达式）→ `new` |
| 语法 | 后缀，绑定力 85（同 `switch` 后缀表达式）；`{ F = v, ... }` 复用 `_parseObjInit` 的字段体解析（支持简写 `x ≡ x=x`、尾逗号） |

## 实现落点（Scope 文件）

| 文件 | 变更 | 说明 |
|------|------|------|
| `src/compiler/z42c.syntax/src/TokenKind.z42` | MODIFY | `public static int With = 154;`（末尾追加，不入 zbc） |
| `src/compiler/z42c.syntax/src/Lexer.z42` | MODIFY | `_initKeywords()` 末尾 `this._kw("with", TokenKind.With);` |
| `src/compiler/z42c.syntax/src/Ast.z42` | MODIFY | 新增 `WithExpr { Expr Target; string[] FieldNames; Expr[] FieldValues; int FieldCount }` |
| `src/compiler/z42c.syntax/src/ExprParser.z42` | MODIFY | `_parseExpr` 后缀分支（仿 `switch` bp 85，:57-87）：`k==With && minBp<=85` → 解析 `{...}` 字段体 → `WithExpr` |
| `src/compiler/z42c.semantics/src/ConstructTyper.z42` | MODIFY | 新增 `_bindWith(WithExpr, env)`：校验 record + 覆盖字段名 → 按声明序造 ctor 实参（覆盖/读原）→ 脱糖 `BoundSeqExpr`（临时绑 orig + `ObjNewExpr`） |
| `src/compiler/z42c.semantics/src/ExprTyper.z42` | MODIFY | `_bindExpr` dispatch 加 `if (e is WithExpr) return _construct._bindWith(...)`（:325-333 表） |
| `scripts/install/xtask_install_vscode.z42` | MODIFY | `_kwOperatorExpr()` 加 `"with"`（新关键字须归类，否则 vscode-syntax gate 报 ghost） |
| `src/toolchain/devtools/vscode/syntaxes/z42.tmLanguage.json` | MODIFY | 重生成（`xtask deps install vscode`）——含 `with` 的 operator 关键字正则 |
| `src/tests/pattern-matching/with_expr.z42` | NEW | e2e：单/多字段覆盖/简写/表达式值/链式/嵌套 record/原对象不变；interp+jit 双验 |
| `docs/book/src/language/pattern-matching.md` | MODIFY | 新增 `with` 表达式语法 + 脱糖语义 |

## 自举 / 格式影响

- **无 zbc/zpkg 格式 bump**：`with` token 末尾追加不入 zbc（`TokenKind.z42:1-2` 明示 token 不入 zbc）；纯脱糖
  复用 `ObjNewInstr`/`FieldGetInstr`/`CallInstr`，无新 opcode、无新 class-flag。
- **两-nightly 纪律**：`with` 是新关键字/语法，只在 e2e 测试文件用；z42c / stdlib / xtask 源**不使用** `with`
  → 上一 nightly 的 z42c 仍能编当前源（support 先行、use 晚一 nightly）。`grep` 确认 `with` 当前**无标识符
  用法**（14 处全是注释/诊断字符串）→ 提为关键字零冲突。
- **自举字节不动点**：z42c 源无 `with` 用法 → gen1==gen2 天然成立。
- **纯脱糖规避 syntax→semantics 新 Bound 类型**：`WithExpr` 是 syntax 节点，semantics 消费它但**不产新 Bound
  节点类型**（脱糖到既有 `BoundSeqExpr`）→ 减少冷启动 stale-cache 风险（选脱糖路线的额外理由）。仍须 clean-cold 本地验。

## User 6.5 裁决（已确认）

1. **脱糖策略 = 读原字段作 ctor 实参 + 替换覆盖项**（`p with {Y=99}` → `$t=p; new Point($t.X, 99)`；
   避 record 字段 readonly/init 的赋值校验 E0415）。
2. **覆盖字段范围 = 仅主构造器字段**；record 有基类时基类字段 `with` defer。
3. **适用类型 = 仅 record class**（struct record defer）。
4. **关键字 `with`**（当前零标识符冲突）。

补充：新关键字须同步 vscode grammar SoT（`_kwOperatorExpr` 加 `with` + 重生成 tmLanguage.json），否则
`test compiler` 的 vscode-syntax 一致性 gate 报 ghost。
4. 关键字 `with`（vs 其它拼写）是否 OK。
