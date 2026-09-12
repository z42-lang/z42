# Tasks: report-crosspkg-duplicate-function

> 状态：🟢 已完成 | 创建：2026-09-12 | 完成：2026-09-12 | 类型：fix（复用 E0601 / E0606 / E0456，不新开码）

**变更说明：** `report-crosspkg-duplicate-type`（#576/#577）只覆盖了**类型引用**这条链。
本 change 补完同族的另外两块：**自由函数**的跨包同名，以及**静态调用位**这个 E0601 的覆盖漏洞。
两者此前都是静默 first-wins。

## 实测的修前行为（当前 main 自建编译器，非推断）

```
free   -> from-a      # 两个包各声明 Demo.FnNs.helper，编译 rc=0 零诊断
static -> util-a      # 两个包各声明 Demo.FnNs.Util（同 FQN 的类！）——E0601 竟然没响
helper -> alpha       # 分居两个 ns、两个都 using 了，照样静默选一份
```

🔴 **运行期这次也指望不上**：输的那个包因**惰性加载压根不会被载入** ⇒ 连
`duplicate function … keeping first-loaded` 都不会打印。**编译期是唯一防线**
（与类那侧不同——那边运行期至少会 warn）。

## 根因（两条，形状不同）

### ① 自由函数比类更糊：连 FQN 视图都没有

| | 类型 | 自由函数 |
|---|---|---|
| 符号表键 | 裸名 + **`ClassesByFqn` 兜底** | **只有裸名**（`ExportedFuncZ.Name` = `md.Name`，从无 ns 前缀）|
| 同短名跨 ns | 各占一条记录 | **撞同一个键**、first-wins |
| 输家能指到吗 | 限定写法有时可以 | **永远不能**（自由函数只有裸名一种调用形态）|

### ② 静态调用不走 `_chkTypeRef` —— 我上一个 change 的覆盖漏洞

E0601/E0606 挂在 `TypeChecker._chkTypeRef` 上，而 `Util.go()` 里的 `Util` 只经 `GetClass` 取类、
**不作类型引用检查** ⇒ 同 FQN 的静态类在这条路径上零诊断。
（`MemberResolver` 那里早就有一句 `ChkAmbiguousBareName`（E0456）—— 只是 E0601 没跟上。）

## 实现

**不新开码**，三种形状用现成的码，语义正好对上：

| 形状 | 码 |
|---|---|
| 同 ns、两个**依赖包**各一份 | `E0601`（同 FQN，谁也选不了）|
| 同 ns、其一是**本包**声明的 | `E0606`（本地恒赢，被遮那份指不了）|
| **不同** ns、都可见 | `E0456`（裸名歧义，调整 `using` 可消歧）|

- **数据**：`SymbolTable.FuncOriginAll` —— 函数名 → 全部来源 `ns#pkg`（`pkg` 空串 = 本包）。
  三种形状恰好由「ns 同不同」×「pkg 同不同」区分，**只存其一都判不出来**。
- **累积**：导入侧在 `ImportedSymbolLoader` 的 first-wins 守卫**之外**；本包侧在
  `MemberCollector`（那里 `table` 已是 `WithScopeOf(cu)` 视图 ⇒ `ScopeNs` 正是本 CU 的 ns）。
- **判据+消息**：`SymbolTable` 三个方法，**只此一份**。
- **消费**：`MemberResolver` 的两处自由调用漏斗；静态调用分支补一句 `_chkTypeRefPkg`。
- **发码顺序 E0601 → E0606 → E0456**：前两者写限定名也没救，后者还能调 `using`，同现时前者是根因。
- **可见性过滤**：只登记**激活**的包（`using` 命中其某 ns，整包粒度）。同名但没 `using` 进来的
  不参与判定——否则「同名但我根本没用到」变假红，那在真实工程里极常见。

## 语料实测（决定了「不同 ns 同名」这条要不要一起管）

扫全仓 `src/libraries` + `src/compiler` + `scripts` 的自由函数：**638 个名字，跨包同名仅 1 处**
（两个 exe 各自的 `Main`，互不依赖、不可能同时可见）⇒ **真实冲突 0**，三种形状都能安全上门。

## 验证

- [x] 单测 +6（`typecheck/crosspkg_duplicate/`）：4 正例（自由函数同 ns 跨包 / 不同可见 ns /
      本包遮蔽导入 / **静态调用位**）+ 2 负例（单一来源不报 / **不可见 ns 不报**）
- [x] **退回对照**：撤掉 `FuncOriginAll` 累积 + 静态调用位那句 → 4 条新正例全红、2 条新负例照绿
- [x] e2e 负例 fixture `dup_free_function_crosspkg`（真实三包）PASS；cross-zpkg 29/29
- [x] `xtask test` 全绿（13 stage）+ 自举不动点 3/3（已 rebase 到含 #584/#586/#587 的 main 后重跑）
- [x] 零格式 bump

## 与 #586 的关系（父包不算「另一个包」）

本 change 实施期间，#577 的 E0606 在 dev-target 模式下被发现误报（父包源码编进测试单元、
父包 zpkg 又在依赖里 ⇒ 每个父包类型都像「本地 + 导入同 FQN」），main 因此三平台 CI 判红。
我做了独立修复 PR #585，但 **#586 先落地且实现完全等价**（同判据、同三处落点）⇒ #585 已关闭。
本 change 只在**自由函数**那条累积上补同一判据，写法与 #586 保持一致（一个文件只留一种惯用法）。

## Deferred（承接自 report-crosspkg-duplicate-type）

- **运行期 duplicate 是否升成 error**：未动（需先厘清合法重复场景）。
- **其余跨包诊断补门**（E0404 跨包 internal 等）：原语已就位。
