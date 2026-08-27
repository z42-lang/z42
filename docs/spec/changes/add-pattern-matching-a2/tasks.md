# Tasks: 模式匹配 A2

## 0. DRAFT + 6.5
- [x] proposal / design / spec 落地
- [x] User 6.5 确认（is 只收 `..=`/关系；or 无绑定；`..=`/关系限可比较基元 —— 均确认）

## 1. 词法
- [x] `TokenKind.z42`：+`At = 152` / `DotDotEq = 153`
- [x] `Lexer.z42`：`..=`（三字符，先于 `..`）+ `@`（单字符）

## 2. syntax 节点 + parser
- [x] `Pattern.z42`：+`OrPattern` / `AtPattern` / `RangePattern` / `RelationalPattern`（+ `Dump`）
- [x] `PatternParser.z42`：or-链 + `_parsePrimaryPattern` + `_parsePatternConst()`(bp45) + 关系/`@`/`..=` 起始 + `_isPatternLead` +relop
- [x] `ExprParser.z42`：is-path 改调 `_parsePrimaryPattern`（`|` 保持位或、or/@ 不入 is）

## 3. semantics 绑定 + 发射
- [x] `BoundPattern.z42`：+4 bound 节点
- [x] `PatternBinder.z42`：`_bindOr`（+ `_patternBinds` 无绑定校验）/ `_bindAt` / `_bindRange` / `_bindRelational`（用 `TypeFacts.IsOrderable`）
- [x] `PatternEmitter.z42`：4 lowering（`_relInstr` 派发 Gt/Ge/Lt/Le）

## 4. 测试
- [x] `src/tests/pattern-matching/pattern_a2.z42`：四形态 × switch-stmt/expr（+ is 关系/范围）；**interp + jit 双绿（直跑 exit 0）**
- [x] 回归：`pattern_core` / `pattern_is` interp+jit 双绿；无生产源 A2 语法（byte-identical 保）
- [x] 负例：or-带绑定 / 非可比较 range 均报错

## 5. GREEN + 文档 + 落地
- [x] `rm -rf artifacts/build/compiler && xtask build compiler`（clean-cold 绿，retry-on-fail 收敛 pre-A1 seed）
- [x] `docs/book/src/language/pattern-matching.md` 补 A2；`examples/patterns.z42` 补例
- [ ] `xtask test compiler`（lexer/parser 单测）
- [ ] PR → 盯 CI（gen1==gen2 不动点 + test-vm/stdlib-jit + bootstrap-no-csharp = 权威 GREEN）→ 合并 → 删 worktree/分支
