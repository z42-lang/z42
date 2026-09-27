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
- [ ] 0.8 ⚠️ **`xtask test fingerprint` 尚未真跑**（需 `--base <树>`，本轮 exit=2 是 usage 错误，不算绿）
- [ ] 0.9 ⚠️ **新门判别力未验**：临时撤掉 0.2 的挂钩，确认
      `test_iface_names_qualified_under_namespace` 真会红（否则只是"看起来会红"的断言）
- [ ] 0.10 `xtask build sdk` → `xtask test examples`（改编译器可见行为的必经门）
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

## 阶段 3 —— I2 归一唯一出口响亮化

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
