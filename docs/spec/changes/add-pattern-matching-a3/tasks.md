# Tasks: 模式匹配 A3（or-模式带绑定）

## 0. DRAFT + 6.5
- [x] proposal / design / spec 落地
- [x] User 6.5 确认（① 类型一致性：完全相同 vs LUB → **完全相同**；② 嵌套 or 绑定 → **支持**）

## 1. semantics 节点
- [x] `BoundPattern.z42`：`BoundOrPattern` +`BindNames`/`BindTypes`/`BindCount`（全经构造器设置，避 E0402）

## 2. semantics 绑定
- [x] `PatternBinder._bindOr` 重写：各 alt 绑进 `env.PushScope()` 子作用域；`child.Vars.Keys()` 收集绑定集；
      首 alt 作参考、后续集合式比对（`_checkOrBindConsistency` + `_indexOf`）；统一集 `env.Define`
- [x] 删死代码 `_patternBinds`（A2 的无绑定校验，A3 不再需要）

## 3. semantics 发射
- [x] `PatternEmitter` or lowering：`BindCount==0` 保持 A2 byte-identical；`BindCount>0` → phi-free 合流
      （预分配 `stable[]`、各 alt `okL` 里 `CopyInstr` 进稳定寄存器、matchL 处 `Locals.Put`）

## 4. 测试
- [x] `src/tests/pattern-matching/pattern_a3.z42`：headline `Circle(r)|Square(r)`（switch-expr + stmt，走
      首/次 alt 都验）、多绑定 `Pair(a,b)|Duo(a,b)`、守卫、`@`+or、嵌套 or `Box(Circle(r)|Square(r))`
- [ ] interp + jit 双绿（直跑 exit 0）
- [ ] 回归：`pattern_core` / `pattern_a2` / `pattern_is` interp+jit 双绿（A2 无绑定 or byte-identical）

## 5. GREEN + 文档 + 落地
- [x] `xtask build compiler`（fresh nightly seed；z42c self-build 绿——本机 seed 需 post-#293 有 analyzer 类型）
- [ ] `xtask build stdlib` + 单 `--file pattern_a3` e2e interp+jit
- [ ] `docs/book/src/language/pattern-matching.md` 补 A3；`examples/patterns.z42` 补例（可选）
- [ ] PR → 盯 CI（gen1==gen2 不动点 + test-vm/stdlib-jit + bootstrap-no-csharp = 权威 GREEN）→ 合并 → 删 worktree/分支

## 备注（本机 seed 教训）
- z42-test 及各 warm 树的 `.z42` seed **太旧**（predates #293 attribute-handler-registry），缺 `TextEdit`/
  `CodeFix`/`FixSink`（`z42c.syntax/src/Analysis.z42`）→ z42c self-build 冷启动 `E0443 undefined type`。
  **修**：`gh release download nightly z42-sdk-nightly-macos-arm64.tar.gz` 换 fresh seed（有 analyzer 类型）
  → 一遍过。**这不是 A3 代码问题**（错误全在 AnalyzerDriver/Main，非本 change 文件）。
