# annotate-stdlib-nullable-returns

**类型**：`fix`（最小化模式）—— 不改编译器一行，只给 stdlib 的公开 API 加 `?` 标记。

**母线**：`docs/spec/archive/2026-09-23-define-null-check-marks/`。
该线的 D8（反向推导）被裁决「不做」，理由是 **opt-in 的机制只能靠作者 opt in**，
覆盖率改由**人工标注 stdlib** 解决。这就是那一步。前置（extern 桩携带标记）已随 #806 打通。

---

## ⭐⭐ 挑候选：两条判据，第二条是本轮实测补上的

### 判据一（必要）：**注释里作者自述「返回 null 表示没有」**

比主观判断硬得多 —— 等于作者早就 opt in 了，只是当时没有 `?` 可写。
`grep -iE "null (if|when|on)|返回 *null|, or null" src/libraries/*/src/` 实测 **50 条注释**。

⚠️ 别用「往上找最近的 public 签名」那种 awk 粗筛：虚方法桩（`ReadAt` / `StrAt` / `Dump`）
会淹没结果 —— 正是 D8 数过的 37+42 那两类形态。

### 判据二（充分性，**本轮实测才发现**）：看**主流调用形态**

判据一**不充分**。全量标 30 个再跑全仓，**71 处命中全部集中在 4 个文件**、
且**全是同一种形态**：调用方手里有个编译器**原理上看不见**的不变式。

| 形态 | 处数 | 例 |
|---|---|---|
| `if (t.ContainsKey(k)) { t.Get(k) … }` | 68 | `ManifestLoader` 65 / `TomlParser` 3 |
| `foreach k in t.Keys()` 后 `t.Get(k)` | 2 | `TomlWriter` |
| 下标必然存在（`GenericArg(t, 0)`，t 已知是泛型） | 1 | `JsonBinder` |

⇒ **规则**：该 API 是否配有**独立的存在性测试**？
- **配了**（`ContainsKey` / `Has` / `Keys()`）⇒ 惯用法是「先测再取」。标 `?` 就是**跟惯用法打架**，
  产生的误报只能靠重写调用点或 `Expect` 消掉，而**流分析永远看不见跨 API 的不变式**
  （同「`Assert.True(a != null)` 不是控制流窄化」那条）。**不标。**
- **没配**（EOF / 解析失败 / 查找未命中）⇒ 检查返回值**本身就是**惯用法。标 `?` **免费**。

⭐ 这条也解释了为什么 `StrMap.Get` 反而**零命中**、可以标：它虽然是 map，
但注释就写着「调用方 `as`-下行前 ContainsKey **或判 null**」，全仓调用点确实都在判 null。
⇒ **判据是实测的调用形态，不是 API 的名字或类别。**

## 本批标注（27 处 / 20 个文件，全仓命中 0）

| 包 | API | null 的含义 |
|---|---|---|
| z42.io | `StreamReader.ReadLine` / `StringReader.ReadLine` / `TextReader.ReadLineBase` | EOF |
| z42.collections | `LinkedList.Find` / `First` / `Last`、`LinkedListNode.Next` / `Previous` | 找不到 / 空表 / 到头 |
| z42.core | `Type.GetInterface` / `GetAttribute` | 没有 |
| z42.core | `FieldInfo`·`MethodInfo`·`ParameterInfo`·`PropertyInfo.GetAttribute` | 没有 |
| z42.core | `Version.TryParse` / `TimeZone.FromName` | 解析失败 / 未知代码 |
| z42.core | `WeakHandle.Upgrade`(extern) | 目标已被回收 |
| z42.ir | `SidecarReader.Find` / `StrMap.Get` / `DependencyIndex.GetStatic`·`GetStaticScoped`·`GetInstance` | 未命中 |
| z42.json | `JsonPath.Select` | 路径不存在 |
| z42.net | `HttpHeaders.Get` / `HttpClient.GetCookieJar` | 头不存在 / 未设 |
| z42.cli | `SubcommandRouter.Match` | 没路由到 |
| z42.test | `BenchStats.parse` | 没有行能解析 |

**一处 extern**（`WeakHandle.Upgrade`）之所以现在有意义，靠的是 #806。

## 明确不标（留理由）

- **`TomlValue.Get`**：70 处全是 `ContainsKey`/`Keys()` 守卫 ⇒ 判据二否掉。
  要标就得先给 TOML 一套「取即检查」的 API 形状（同 `??` 那轮的结论：
  **问题不在语言特性，在 API 形状**），属独立 change。
- **`JsonReflect.GenericArg`**：下标型访问器，同上。
- **`Type.GetElementType`**：⚠️ **探针漏掉、靠 `xtask test` 才抓到** —— 它配的存在性测试是
  **`IsArray`**，全仓调用点清一色 `typeof(int[]).GetElementType().Name`（调用方静态就知道是数组）。
  同 `TomlValue.Get` 配 `ContainsKey` 一模一样的形态 ⇒ 判据二否掉。
- **`PropertyInfo.GetValue`**：返回 null 表示「值就是 null」，**不是「没有」** ——
  判据一就不成立（注释里那句 null 说的是 `__accessor`，不是 `GetValue`）。
- **字段类**（`HttpRequest.Body` / `HttpResponse` / `ModuleLoader` 的 `[Skip]` 三个 /
  `ParameterInfo.DefaultValue`）：字段标记**跨不过包边界**（母线缺口②未做）⇒ 标了只在本包生效，
  语义半生不熟，随②一起做。

## ⭐⭐ 命中全在测试里 —— 这个分布本身是结论

清缓存跑全量后，27 个标注的真实命中是 **32 处，全部在 `*/tests/*.z42`，生产代码 0 处**：

| 文件 | 处数 | 形态 |
|---|---|---|
| `z42.json/tests/json_path.z42` | 9 | 已知的 `_sample()` + 固定路径 ⇒ 必然命中 |
| `z42.ir/tests/depindex.z42` | 7 | 上一行刚 `AddModule` 注册进去 |
| `z42.core/tests/timezone.z42` | 5 | `"JST"` 是内建短码 |
| `z42.test/tests/bench_stats.z42` | 4 | 字面量就是格式良好的 bench 行 |
| `z42.cli/tests/cli_subcommand.z42` (+`_nested` 1) | 5 | `argv[0]` 就是刚 `Add` 的子命令 |
| `z42.net/tests/http_auth_helpers.z42` | 2 | 上一行刚 `WithBasicAuth` 设过这个头 |

⇒ **生产代码零命中 = 这 27 个标注对现有代码是免费的**（调用点本来就都在检查）。
⇒ 测试里那 32 处**不是误报**：测试确实握有「我构造的输入必然命中」这个前提，
而 `Expect("理由")` 正是为此设计的**唯一显式逃生口**。

⭐ **迁移让测试变好了，不是变差**：原写法 `Assert.NotNull(m); … m.Name()` 里
那句 `Assert.NotNull` 对编译器不可见（它不是控制流窄化），一旦哪天真返回 null，
炸点是 `m.Name()` 的 NRE；改成 `Expect("argv names a registered subcommand")` 之后
**抛的是带理由的异常**。⇒ 这批迁移顺带把「断言与解引用之间的空隙」关掉了。

## ⚠️ 版本语义：加 `?` **是**破坏性变更

母线 proposal 写着「**去掉 `?` 永远不破坏任何人**」—— 反过来说**加** `?` 会破坏：
下游直接解引用这些 API 而不检查的代码会开始报 E0478。
本仓内实测代价为 0，但**仓外用户会受影响**。0.6 线（pre-1.0）可以做，需在 release notes 点名这 28 个 API。

## 落地

- [x] 0. 并入上一个 change 的归档 —— `carry-null-marks-through-native-stubs` 在 #806 里
      **漏了阶段 9「归档必须在 PR 内」**，按 workflow 的补救法并进本 change 的首个 commit。
- [x] 1. 探针：全量标 30 个 + 把 E0478/E0479/E0484 临时降级 warning，全仓跑一遍
  - ⚠️ 缓存判据：日志里**必须全是 `cached: 0/N`**（本线已栽过三次）。本轮已核：无一条 `cached: [1-9]`。
  - ⚠️ 探针改动**已全部还原**（`FlowAnalyzer.z42` 用备份覆盖回来，`git diff` 核对为空）。
- [x] 2. 按判据二撤掉打架的 2 个，保留 28 个
- [x] 3. 按判据二再撤掉 `Type.GetElementType`（27 处）
- [x] 4. 迁移 **32 处测试调用点**（7 个文件）→ 改用 `Expect("理由")`
- [x] 5. 全量 **GREEN**（`build all` + `build sdk` + `xtask test` 全过，零残余诊断）
- [x] 6. 文档：`docs/reference/src/language/types.md` 加「标准库里标了 `?` 的 API」清单，
      并把「配了存在性测试的查找 API 刻意没标」写成给读者的判据。

## 🔴🔴 两条新踩的坑（都值得留）

### ① 探针只跑 `build all` 不够 —— `src/tests/` / `examples/` / `scripts/` 不在里面

`Type.GetElementType` 在探针里**零命中**，`xtask test` 一跑就红两个 golden。
⇒ **摸底的「全仓」必须是 `xtask test`**，不是 `xtask build all`。
（同族教训：`??` 那轮我只搜 `src/`、漏掉 `scripts/` 22 处，合并后 main 直接红。
**「全仓」这个词我已经栽第二次**。）

### ② 🔴 缓存 replay 的**假红**（本线第五次栽在缓存上，且是个新变体）

撤回 `TomlValue.Get` 的标注后重新构建，`ManifestLoader` **仍报 65 处 E0478** ——
zpkg 里 `RetNullable` 明明已经是 0。真因：探针期的编译结果连**诊断**一起留在缓存里，
而源文件哈希与编译器指纹都没变 ⇒ 命中旧条目、把探针期的诊断**replay 出来**。

⚠️ **`artifacts/build/**/cache` 清了不够** —— 还有一批在 **`src/**/artifacts/release/.cache`**
（包目录内！）。完整清法：
```
find . -type d \( -name cache -o -name '.cache' \) -not -path './.git/*' -exec rm -rf {} +
```
⇒ 记忆里原先记的那条清法（只清 `artifacts/build`）**不完整**。
⇒ 判据：撤回一个标注后若现象不变，**先查缓存，别先怀疑撤回没生效**
（我差点据此得出「TomlValue.Get 的标注撤不掉」的错误结论）。
