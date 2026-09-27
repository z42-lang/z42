# Tasks: 名义类型身份一律 FQN，歧义与无解一律响亮

设计见 [design.md](./design.md)。**阶段 1 起需 User 先裁 D-A / D-B（身份表示）。**

## 阶段 0 —— I3 + 三条真缺陷（✅ 已完成，待开 PR）

- [x] 0.1 `StubCollector._passQualifyIfaceNames`：归一**不再就地改写**，产出新数组只换本类型字段引用
      （长度与 `InterfaceCount`/`BaseCount` 原样保留；⚠️ 接口侧尤其不能动长度——`BaseNames` 与
      `BaseRefs` 共用一个 `BaseCount`，错位是静默的）
- [x] 0.2 **A1**：归一 pass 补挂 `SymbolCollector.Collect` 与 `CollectWithImports`
      （此前只挂 `CollectAll`；三份平行 pass 序列漏一处 = 那条路上 `Implements` 恒假）
- [x] 0.3 **A2**：`ClassDescBuilder` 接口限定补点号守卫（镜像同文件基类那格），避免 FQN 被二次限定成
      `Demo.Std.IDisposable`
- [x] 0.4 **B3**：`SymbolTable.InterfaceDerivesFrom` 两个参数对称归一 + 改走 `GetInterface` 双键
- [x] 0.5 带 `namespace` 的真门用例 ×6（`collect_tests.z42`）——既有用例源码都没有 namespace ⇒
      `Fqn()` 退化成短名 ⇒ 断言恒绿 = 空门
- [x] 0.6 字节对账：`base3` vs 带改动 **0 差异**；且 stdlib pass1≡pass2（让结论不依赖会挂死的第二遍）
- [x] 0.7 门：stdlib 347 文件全过 / compiler 自举不动点 3/3 gen1==gen2 / lines / diagcodes / stage2
- [x] 0.8 `xtask test fingerprint` —— **结案为「被更直接的测量覆盖」**，不是跑过了。
      该门的判据是「字节变了 ∧ 指纹没变 ⇒ 红」；本程序每一批都做了**完整 stdlib 字节对账、
      全部 0 差异** ⇒ 前提不成立、无需 bump 指纹。门本身要一棵「stdlib 已由 base 编译器建好」
      的 base 源码树（主 worktree 在 #896 分支上不能当 base，另建买不到新信息）。
- [x] 0.9 ✅ **判别力已验**：撤掉 0.2 的挂钩后，
      `test_iface_names_qualified_under_namespace` / `test_implements_accepts_both_spellings_under_namespace`
      / `test_iface_base_chain_qualified_under_namespace` **确实 FAIL**（共 14 FAIL）⇒ 不是空门。
      源码已自动恢复（`git diff` 空）。
- [x] 0.10 ✅ `xtask build sdk` 通过；`xtask test examples` **95 transcript / 153 step 全过**
- [ ] 0.11 文档同步（doc-check 三道门）+ 开 PR

## 阶段 1 —— P2 消除幻影内建接口（🔴 bootstrap 种子，最高风险）

- [ ] 1.1 摸清 `BuiltinTypeDefs._builtinInterfaces()` 的**真实消费者**：哪些 cold-start 入口在没有
      z42.core 真元数据时依赖它（`bootstrap-seed.md`：删兜底前必须先为**所有** cold-start 入口供种，
      且删+供种是**同一个原子变更**）
- [ ] 1.2 `_extractCore` 的注入改为「只在该注入的地方注入」（候选：仅拥有这些声明的包/ns；
      或改由真实本地声明产出、彻底去掉兜底）
- [ ] 1.3 `ISubscription` 单独处理——11 个里唯一不在 `z42.core/src/Protocols/` 的
      （真声明在 `z42.core/src/Delegates/ISubscription.z42`）
- [ ] 1.4 验证：zsym 应显著变小；`CompilerFingerprint` bump；两代自举
- [ ] 1.5 ⚠️ 失败恢复姿势：`rm -rf artifacts/build/{compiler,libraries}`（**保留 `artifacts/.z42`**）

## 阶段 2 —— P1 建接口歧义判据（✅ 已完成）

- [x] 2.1 新增 `SymbolTable.IfaceNsAll`（对称 `ClassNsAll`：短名 → 全部声明 ns，`StrBox` 以 `|` 连接）
      + `NoteIfaceNs` / `IfaceNsOf` / `IsBareIfaceNameAmbiguous`（判据与 `IsBareNameAmbiguous` 逐条同构）
- [x] 2.2 两处登记：`StubCollector._passInterfaces` 两个分支（本地）+ `ImportedSymbolLoader` 接口循环
      （在无守卫的 last-wins `Put` 旁累积——短名表只留得下一份，本表要的正是被塌掉的那个信息）
      + `SymbolCollector._mergeImports` 逐 ns 并入
- [x] 2.3 `WithAliases` 共享该字段（并行段内只读，同其余 origins 表）
- [x] 2.4 门：**字节与基线恒等**（`DIFF_BASE_S2=0`，50 文件）；stdlib 25/25；
      自举不动点 3/3 gen1==gen2；新单测 ×2 PASS、全套 0 FAIL
- [ ] 2.5 ⚠️ **暂不并进 `ClassNsAll`**（并表才合「判据只此一份」）——并表会让 `IsBareNameAmbiguous`
      对 11 个 prelude 协议名全仓开火。**阶段 3 两表合一**，届时
      `IsBareIfaceNameAmbiguous` 与 `IsBareNameAmbiguous` 要一起塌成一个，别各自漂移

## B2 —— 收口解析入口（歧义 / 无解响亮）

### 🔴 动手前必须先做的归因（**已排除的别重查**）

「无解」在全量 stdlib 构建里命中 **4 次 / 2 个名字**：`ICompiler`、`IReplCompiler`。
把它翻成错误之前必须知道它们走哪条路。已排除：

- ❌ **不是没导出**：`ICompiler` / `IReplCompiler` 在 `z42.build.zsym` 与 `.zpkg` 里都在（`strings` 实测）。
- ❌ **不是缺依赖**：`z42c.pipeline` 的 manifest 有 `"z42.build"`，`Z42cCompiler.z42` 有 `using Z42.Build;`。
- ❌ **不是单 CU 路径**：`Collect(cu)` 的调用点只有 `IrDump:414/430` 与 `SemanticDump:33`，
  而 `ExtractExports` 的调用方只有测试 + `SemanticDump` —— **都不在包构建路径上**。
- ⇒ 只可能来自 `CollectAll`。但 `_mergeImportedInterfaces`（:286）在
  `_passQualifyIfaceNames`（:310）之前，理论上该查得到 —— **矛盾未解**。

⇒ **下一步 = 带上下文的探针**（打印包名/ns + 是 5 个调用点里的哪一个），一个构建周期定论。
5 个生产调用点：`SymbolTable:371`（`Implements` 查询侧）· `:407`（`InterfaceDerivesFrom`，死代码）·
`StubCollector:182`（类 `InterfaceNames`）· `:208`（接口 `BaseNames`）· `InheritanceResolver:35`（impl 块 trait）。

⚠️ **不要在未归因的基础上把「无解」翻成错误** —— 本轮已经因为「机制存在就当病因」返工两次。

### 任务

- [ ] B2.0 归因（见上）
- [ ] B2.1 `Resolve(scope, name) → id | AMBIGUOUS | NOTFOUND`，唯一入口
- [ ] B2.2 歧义 → 新诊断码（**实测代价 = 0**，带阳性对照）。分配码必须逐个
      `git show <每个在飞 PR 分支>:DiagnosticCodes.z42`，扫 main 不够（已四次撞码）
- [ ] B2.3 无解 → 按 B2.0 的结论决定（报错 / 或先收敛调用点）
- [ ] B2.4 阳性对照：`$SCRATCH/ambig` 工程必须变红

## 阶段 3（原字符串路的计划，D-B 选定后由 B2~B5 取代）—— I2 归一唯一出口响亮化

- [ ] 3.1 `IfaceFqnOf` 三分支化：唯一解 → FQN；多解 → 新诊断码；无解 → `E0401`。**删掉原样返回**
- [ ] 3.2 新诊断码走 `DiagnosticCodes.z42` 登记表（⭐ 分配码**必须逐个
      `git show <每个在飞 PR 分支>:DiagnosticCodes.z42`**，扫 main 不够——已四次撞码）
- [ ] 3.3 消息与判据**各只一份**（照 `AmbiguousBareNameMsg` / `IsBareNameAmbiguous` 的既定纪律）
- [ ] 3.4 收口 5 处过期契约注释（design §7）
- [ ] 3.5 ⚠️ 预期会让 `examples/` 变红——`run.console` 把期望的错误也记着

## 阶段 4 —— I1 扩到类基类轴 + 删死码

- [ ] 4.1 `BaseName` → FQN（`IsSubclassOf` 及其调用方）
- [ ] 4.2 ⚠️ `SymbolTable.Implements:361` 的 `this.Classes.Find(cur)` **绕过 `GetClass`**——
      今天安全（7 个调用方第一参数全是短名），但**翻 `BaseName` 那一刀落下时它会静默截断 base 链**
- [ ] 4.3 删死码：发射端「歧义→退回短名」降级（`FunctionEmitter.z42:243` / `ClassDescBuilder.z42:81`）、
      `ResolveTypeP` 的两个 ScopeNs 改道分支
