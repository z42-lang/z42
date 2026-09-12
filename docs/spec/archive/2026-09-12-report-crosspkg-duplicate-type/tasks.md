# Tasks: report-crosspkg-duplicate-type

> 状态：🟢 已完成 | 创建：2026-09-12 | 完成：2026-09-12 | 类型：lang（接线 E0601 + 新增 E0606）

**变更说明：** 两个**不同的包**声明**同一个 FQN** 的类型时，z42c 此前**完全静默**地 first-wins ——
输的那一份连同全部成员从未存在过，零诊断、rc=0。E0458（#554）管住了「同一个包内」的重复，
跨包这半边一直没人看。现在报 **E0601**（该码早已分配、`error-codes.md` 也登记了「导入符号冲突」，
但 `ImportedSymbolLoader.z42` 顶上的注释白纸黑字写着「延后：…E0601 冲突诊断」——**从未接线**）。

## 实测的修前行为（当前 main 自建编译器，非推断）

fixture `src/tests/cross-zpkg/dup_fqn_crosspkg/`：两个互不依赖的包 `demo.dupalpha` /
`demo.dupbeta` 各声明 `Demo.DupNs.Widget`，main 同时 `using Demo.DupNs`。

```
bare      -> alpha        # 编译 rc=0、零诊断；alpha 赢，beta 的 Widget 从未存在
```

**症状二（更糟）**：给 beta 的 `Widget` 加一个 alpha 没有的成员 `OnlyBeta()` 再调用 →

```
E0401: no method `OnlyBeta` on `Widget`
```

用户正看着 beta 的源码，那个方法白纸黑字写在那儿。诊断**答非所问**：真相不是「没有这个方法」，
而是「`Widget` 根本不是你以为的那一个」。同 E0456 / E0458 一路在修的形状——**静默择一 ⇒ 自信的错答案**。

**本地遮蔽的实测（决定了 E0606 的文案与严厉程度）**：本包也声明同 FQN 时，`bare` 与 `qualified` **都**绑到
本地那份，而依赖包那份**没有任何写法能指到**（限定名也不行，两者 FQN 逐字相同；C# 靠 `extern alias`，
z42 无）。所以文案给的修法是**改名**，不是「写限定名」。

## 事实校正：运行期那半**不哑**（本 change 不动它）

`lazy_loader/registry.rs` 的两条 `tracing::warn!` 默认就打 stderr（实测，无需 `RUST_LOG`）：

```
WARN duplicate function `Demo.DupNs.Widget.Who` from zpkg `demo.dupbeta.zpkg`; keeping first-loaded
WARN duplicate type `Demo.DupNs.Widget` from zpkg `demo.dupbeta.zpkg`; keeping first-loaded
```

**全哑的只有编译期。** 运行期是否该从 warn 升成 error 是另一个判断（可能有合法重复场景），未在此改。

## 根因

`ImportedSymbolLoader.Load`：FQN 键已被占 ⇒ 第二份**压根不建型**，也没有任何地方记下
「这个 FQN 有两个来源包」。而 `pkgNames[]` 与 `exported[]` 是平行数组 —— **来源包名一直在手边**。

顺带纠正一句假断言：`SymbolCollector._mergeImports` 里 `ClassesByFqn` 的 local-wins 守卫旁写着
「键天然唯一（FQN 不会撞）」——**那句是错的**，本 fixture 就是反例。

## 实现（完全对称 E0456 的既有架构）

| 件 | 落点 |
|---|---|
| 累积 | `ImportedSymbolLoader`：first-wins 守卫**之外**记 FQN → 包名（复用 `SymbolTable.NoteNsInto`）。类/接口/enum 共用一张表 |
| 合并 | `SymbolCollector._mergeImports`：与 `ClassNsAll` 同路并进 `SymbolTable.ClassPkgAll` |
| 判据+消息 | `SymbolTable.CrossPkgDuplicateMsg` / `ShadowedImportMsg`：**只此一份** |
| 消费 | 两个既有 choke point：`TypeChecker._chkTypeRef`（表达式位）+ `SymbolCollector._chkTypeRefT`（声明位）——两者都已拿着**解析后的类型**，不改签名、不动既有诊断的 span |

**与 E0456 的关键差别**：E0456 只管**裸名**（见到限定名就早返回，因为限定写法正是消歧手段）；
本码限定名**照样**歧义，故判据用 resolved `Z42Type.Fqn()` 而非 `TypeExpr`。

**触发时机 = 使用位**（对标 C# CS0433，User 裁决）：两个依赖包撞了但本包没引用 → 不报。
**本地遮蔽 = E0606 error**。这条走过一次弯路，值得记下来：

- 初版按 C# CS0436 做成 **warning**，理由是「本地恒赢是既定规则、不是猜」。
- User 反问「没办法遮蔽调用，那是不是应该改成 error」—— **对的，我那个理由站不住**：
  **规则明确 ≠ 结果可接受**。被遮蔽那份**根本指不了**（FQN 逐字相同，限定名也分不开；
  C# 有 `extern alias` 作逃生口，z42 没有），失败形态与 E0601 一模一样：去调依赖包那份才有的
  成员 → 「no method X on Widget」答非所问。
- 与 E0601「未引用则不报」的分工也不同：那条是两个第三方依赖打架、下游无权修；
  本条冲突的两份里**有一份是本包自己写的**，改名随时可以 ⇒ 报错是**可行动的**。
- 唯一代价：「故意声明同 FQN 以覆盖库类型」这条野路子被堵死。它本就无法控制、无法回退，
  且实测全仓 **0 处** ⇒ 代价为零。

⭐ 教训：**判断一个诊断该 warn 还是 error，别只问「行为是否确定」，要问「错了的话用户能不能
看出来、能不能修」**。确定但不可观测、不可规避的错误行为，比不确定更该报死。

## 🔴 顺带修的一条：z42c 从不打印 warning

实现到一半（当时还是 warning）发现它 **不会出现在终端**——`Main.z42` 的两个诊断打印点都被 `ErrorCount > 0` 罩着，
而 `DiagMsgs` 里其实装着全部诊断。⇒ 现有的 W0700 / W0603 / W0604 / deprecated **一直是哑的**，
「从不打印的门 = 没有门」，与本 change 要修的是同一族毛病。

**先量再改**：临时给打印点加探针（并用 W0700 用例校准探针确实是活的）→ 全量构建
**stdlib 25 个包 + z42c 自建 = 0 条 warning** ⇒ 打开它的代价为 0。现在无错但有 warning 时
也打印（stderr，不改退出码）。

## 顺带补上：跨包负例的自动门

`src/tests/README.md` 白纸黑字记着「跨包 / 多文件的期望报错今天仍靠手工验证，**没有自动门**」。
本 change 把两条原语都补上了：

1. **单测原语**（快，不碰磁盘 .zpkg）：`IrDump.ExtractExports` 合成 `ExportedModuleZ` + 包名 →
   `ImportedSymbolLoader.Load` → `IrDump.BuildPackage(..., imported, ...)`。可造任意跨包形状。
2. **cross-zpkg 负例 fixture**（慢，走真实三包 + 真 zpkg 元数据）：新约定
   `expected_build_error.txt` 代替 `expected_output.txt`——main 必须编译失败**且** stderr 命中其内容。
   ⚠️ 此前活跃过滤只认 `expected_output.txt`，负例 fixture 会被**静默跳过**（连 FAIL 都不是）。

两条都需要：单测覆盖判据逻辑，e2e 覆盖**真实接线**（`scan.ExportedPkg` ← zpkg META name）——
后者若回归成空串，单测照样全绿。

## 验证

- [x] 单测 8 条（`typecheck/crosspkg_duplicate/`）：5 正例（使用位 / 声明位 / **限定名照样报** /
      本地遮蔽类 / 本地遮蔽**接口**）+ 3 负例（同短名跨 ns 不报 / 同包多模块不报 / 未被引用不报）
- [x] **退回对照**：撤掉 `ClassPkgAll` 累积 → 4 条正例全红、3 条负例照绿（= 门有判别力）
- [x] **退回对照（接口那条单独做）**：撤掉 `ShadowedImportMsg` 的 `InterfacesByFqn` 分支 →
      接口用例红、类用例照绿。自查出来的不对称：E0601 经 `PkgCheckFqn` 本就覆盖接口，
      而 E0606 一开始只查 `ClassesByFqn` ⇒ 「本地接口遮蔽导入接口」会单独哑掉
- [x] e2e 负例 fixture **两个**（`dup_fqn_crosspkg` = E0601、`shadow_import_crosspkg` = E0606）
      走真实三包 + 真 zpkg 元数据 PASS；cross-zpkg 27/27
- [x] **新门自校准**：main 改成编得过 → 判红；期望文本改成对不上 → 判红（不是空门）
- [x] `xtask test` 全绿（13 stage）+ 自举不动点 3/3；全量构建**新增 warning 噪声 0 条**
- [x] 零格式 bump（纯诊断，不碰任何持久化字节）

## Deferred

- **自由函数 / 静态调用键的跨包重名**：`DependencyIndex` 有自己的活跃-ns 解析链路，与类型引用
  这条不同源，未覆盖。
- **运行期 duplicate 是否升成 error**：见上「事实校正」。需先厘清合法重复场景（静态链接 / 同包两副本）。
- **其余跨包诊断补门**（E0404 跨包 internal 等）：原语已就位，照上面两条任一条加即可。
