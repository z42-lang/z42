# Proposal: 实例方法派发键稳定化（全签名键 + VM vtable 保 `$`）

> 承接 `stabilize-dispatch-keys`（2026-07-14，静态-only）明确 **Deferred** 的「实例维度键稳定化」
> （见其 design.md §Deferred、tasks 尾）。本变更把静态方法已经落地的「键 = 自身签名的纯函数」扩展到
> **实例/虚/接口/泛型/委托/foreach** 全部实例派发，并把上次全 mangle 尝试**回退时未做**的运行期部分
> （VM vtable 保 `$` + interp/JIT 派发同步 + 各多态子系统裸名 emit 位点对齐）这次做对。

## Why

派发键的唯一职责：让**调用点 emit 的字符串** == **被调方注册/导出的字符串**。今天**实例**方法的键
仍是「兄弟集的函数」——同名唯一 → 裸 `IndexOf`；同 (name,arity)≥2 → 全签名 `IndexOf$1$string`。于是
**给一个原本唯一的实例方法加一个重载，会让它从裸键 re-mangle**，所有已编译的调用方（含无法重编的
上一 nightly bootstrap seed）仍指向旧裸键 → 打新库 `undefined function`。

这不是假设：`library-review` 的 **String 补齐**正是撞死在这里——给 z42c 自己调用的 `String.IndexOf`/
`Split`/`Trim` 加 char 重载，rekey 后断掉 seed 的 `z42.build` `SourceDiscovery`（实测 `undefined function
Std.String.IndexOf`）。而这一整类反复出现的补丁——**E0436**（prim 实例 type-based 重载决议）、**E0433**
（partial 协议豁免重载误报）、**ImportedSymbolLoader 的 bare first-wins 别名**、本次 rekey 断种子——
根因同一个：**实例方法的键不是自身签名的纯函数**。

### 根因（三路排查确认）

- **编译器侧**：`MemberCollector._fillClass` 的实例分支用「兄弟集预扫描」（`ovldInst`/`arityDup`）决定
  裸 / `Name$arity` / 全 mangle 三档；静态非虚分支早已**无条件全 mangle**（`stabilize-dispatch-keys` 落地）。
  → 差距只在实例维度。键「定义点算一次、序列化进 TSIG、全链路逐字节复用」（D2 契约）——改键内容
  **不改 zpkg/zbc wire 布局**（键只是现有字段里的字符串）。
- **运行期侧**：VM 的 **vtable 槽用「简单名」（`derive_simple_method_name` 在首个 `$` 处截断）** 做
  key。实例方法一旦全 mangle，`Bar$1$string` 与 `Bar$2$int$int` 会**塌进同一槽**——这正是上次全 mangle
  尝试挂 19 个 e2e golden 的物理根因（`types.rs:942`）。
- **上次回退的教训**（`stabilize-dispatch-keys/design.md §全 mangle 回退记录`）：全 mangle **通过**了两代
  自举 + gen1==gen2 逐字节（导出/DepIndex/byte-identity 自洽），但**挂 19 个 e2e golden，全在实例派发**：
  interface（`Demo.Dog.Name not found`）、泛型（`Num.CompareTo not found`）、泛型-over-原始类型
  （`expected object, got I64`）、委托/事件（`FuncRef` 不匹配）、foreach（`ArrayLen: expected array, got
  Object(Ring)`）。根因同类：这些子系统在多处**裸名 emit**，全 mangle 后与新键失配。**本变更的核心工作
  就是把这 5 个子系统的裸名 emit 位点 + VM vtable 一次性对齐**——上次只把实例键缩回了，没做运行期。

## 关键设计张力（一个键在做两件互斥的事）

overload **决议**（编译期，要「全签名」以选对重载、且稳定于兄弟增删）与多态**派发**（运行期，要一个
base/派生/实例化三方都同意的**稳定槽**）被压在同一个字符串键上。二者在泛型下直接冲突：泛型虚方法
base `op_Add$2$T$T` 与原始类型 override `op_Add$2$i32$i32` **必须同槽**才能经约束派发，但**替换后的
全签名不同** → VCall 落空（`expected object, got I64`）。这就是上次连 `static virtual op_*` 都被迫留基线
的原因。**真解需借鉴 .NET：把「overload 决议键」与「多态派发槽」解耦**——槽按**声明层签名（类型参数
未替换）**定位，overload 键按自身签名。具体拆法见 design.md（泛型排查完成后落定）。

## What Changes（拟）

1. **实例方法键 = 全签名 mangle**（与静态同规则，切断兄弟集耦合）：`MemberCollector` 实例分支恒
   `OverloadResolver.MangleKey`。加/删实例重载永不 rekey 现有方法 → 零 bootstrap 破坏。
2. **协议豁免名保留裸「规范槽」**：`ToString`/`Equals`/`GetHashCode`/`GetType`/`get_Item`/`set_Item`
   仍以裸名作为 VM/DepIndex 单槽多态锚点；**统一今天分歧的两份名单**（`SymbolCollector.IsProtocolExempt`
   6 名 vs `DependencyIndex._isProtocol` 4 名）。
3. **VM vtable 槽按全签名定位（保 `$`）**：`derive_simple_method_name` 不再截断 `$`；VCall/interp/JIT
   候选探测同步（interp `exec_vcall.rs` 与 JIT `jit/helpers/vcall.rs` 两处镜像）。
4. **多态派发槽与 overload 键解耦**（泛型/虚/接口/foreach/委托）：多态槽用**声明层签名**，使 base 与
   派生/实例化同槽；overload 键用自身签名。（拆解细节 design.md 定。）
5. **格式 bump**：zbc + zpkg minor 双 bump（wire 布局不变，仅键字符串内容变）→ 触发 ci-bootstrap 版本差
   gate → 两代自举整树一次性重键 + strict-pin 拒绝残留旧产物。
6. **下游解锁**：本变更进 nightly 后，`library-review` 的 **String 补齐**（含 `IndexOf(char)`/`Split(char[])`/
   `Trim(char[])`）作为独立 change 落地——不再撞 rekey 墙。

## Scope（子系统）

`compiler`（z42c.semantics）+ `stdlib`（z42.ir 的 DependencyIndex/ZpkgWriter/ZbcWriter）+ `runtime`（VM
metadata/interp/jit）。允许改动文件清单在 design.md 定稿后附（当前投研正把「上次回退的 5 子系统裸名 emit
位点」精确定位到当前文件布局——`converge-z42c-ir-metadata` 后路径已变，不能照抄上次 proposal 的旧路径）。

## 验证现实（冷环境，权威在 CI）

上次回退的 19 个失败**只在冷 CI 可见**（本地 warm 跑不出）。故本变更：本地做 `cargo test` + 定向
e2e（interface/generics/generics-over-prim/delegate/foreach 的现有测试作回归门）+ 两代自举本地可跑到
的部分；**其余 push 后盯 CI**（`ci-bootstrap` 两代自举、`bootstrap-no-csharp`、全 golden regen、z42c
不动点 gen1==gen2、e2e golden 全绿）。这是「push 盯 CI」型高风险变更——tasks.md 会把这 5 子系统列为硬门。

🤖 Generated with [Claude Code](https://claude.com/claude-code)
