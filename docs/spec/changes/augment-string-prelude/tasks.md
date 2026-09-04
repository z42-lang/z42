# Tasks: prelude `Std.String` 补齐 char-based BCL 方法（augment-string-prelude）

> 状态：🟡 实施中 | 创建：2026-09-04 | 类型：feat(stdlib) + refactor（纯加性库方法，不动 lang/ir/vm → 最小化模式）
> 上游：`docs/library_review.md` §109「String 缺 PadLeft/PadRight、IndexOf(char)、Split(char[])、Trim(char)、LastIndexOf、Insert/Remove」
> 前置（**已全部拆除**）：E0436 阶段1（#357）/ E0433 partial 协议豁免重载（#375）/ **派发键稳定化（#414）**

## 背景：为什么这个 change 拖了三轮才能开工

补齐动作本身是纯加性的库方法，但**头部方法（`IndexOf(char)` / `Split(char[])` / `Trim(char[])`）是与既有方法
同名的重载**，而 z42 的方法派发键此前依赖「兄弟重载数」：某方法单一重载时注册**裸键**（`IndexOf`），
加第二个重载 → 该名所有重载**全部 rekey** 成 type-mangle（`IndexOf$1$string` + `IndexOf$1$char`），
**裸键消失**。z42c / z42.build 自身大量调用这些方法（`IndexOf` 45×、`.Trim()`、`Split`），
上一版 nightly 的 seed z42c 是用裸键编译的二进制、无法重编 → 冷启动 `undefined function Std.String.IndexOf`。
即：**补一个 String 重载会把自举链打断**（CI 同样会红，非本地专属）。

User 裁决不做 workaround，走根因修复 → `stabilize-instance-dispatch-keys`（PR #391 + **#414**，已合并）把键规则
改成「**primary（声明序首个同名）= 裸基线键 / 非-primary = 全签名 MangleKey**」——加重载变成**纯增量**，
既有 primary 永不 rekey。本 change 是该重设计的**下游首个受益者**，按 support-先行纪律等含 #414 的 nightly
发布后才落（nightly 构建自 `1317141` = #414 本体，已验证种子 `z42.core.zpkg` 里裸 `Substring` 与
`Substring$2$i32$i32` 并存 = 新 keying 已就位）。

## 实施

### 1. refactor：String.z42 声明 partial，拆出碎片（纯位置移动，行为不变）

- [x] 1.1 `String.z42` → `public sealed partial class String`；Split/Join/Concat/Format + `_fmtToken`/`_fmtIndex`
      整块移到新碎片 `String.Split.z42`
- [x] 1.2 新碎片 `String.Edit.z42` 承载 Insert / Remove×2 / PadLeft×2 / PadRight×2
- [x] 1.3 **碎片归属约束**（关键，见下「设计要点」）：同名方法组整组落同一碎片，三个文件头部各注明理由

### 2. feat：补 BCL 方法

- [x] 2.1 `String.z42`：`IndexOf(char)` / `LastIndexOf(string)` / `LastIndexOf(char)`
- [x] 2.2 `String.z42`：`Trim(char)` / `Trim(char[])` / `TrimStart(char)` / `TrimStart(char[])` /
      `TrimEnd(char)` / `TrimEnd(char[])` + 私有 `_inSet` 成员判定
- [x] 2.3 `String.Split.z42`：`Split(char[])` + 私有 `_isSplitSep`（空集合 → 按空白切分，对齐 C#）
- [x] 2.4 `String.Edit.z42`：`Insert(int,string)` / `Remove(int)` / `Remove(int,int)` /
      `PadLeft(int)` / `PadLeft(int,char)` / `PadRight(int)` / `PadRight(int,char)`

### 3. 测试

- [x] 3.1 新测试文件 `src/libraries/z42.core/tests/string_bcl_augment.z42`
- [x] 3.2 **重载决议专项**：每组同名重载都断言到「能区分彼此」的值上（`IndexOf("ll")` vs `IndexOf('l')`、
      `Split("::")` vs `Split([':'])`、`Trim()` vs `Trim('x')`）——`string` 是 primitive 接收者，
      走 MemberResolver prim 路径 + 运行期无-vtable 候选探测，**绑错的表现是「返回另一个重载的结果」而非崩溃**
- [x] 3.3 多字节用例（索引/填充宽度按 Unicode scalar 计数，非 UTF-8 字节）

### 4. 文档

- [x] 4.1 `docs/book/src/language/partial-types.md`「边界与限制 v1」：把含糊的「键 mangle 可能不一致」
      改写成精确机制 + 失败模式（**静默覆盖**）+ 判据（**同名即必须同碎片，与 arity 无关**）+ Deferred 正解
- [x] 4.2 `docs/library_review.md` §109 标记 String 项已完成

## 设计要点

### 追加式纪律：新重载必须追加在既有重载之后

新键规则下 primary = **声明序首个同名**。既有方法要保持裸键（seed / 跨包既有调用的锚点），
新重载就必须**追加在后面**：`IndexOf(string)` 仍是 primary（裸 `IndexOf`），`IndexOf(char)` 取
`IndexOf$1$char`；`Trim()` 仍是 primary，char/char[] 版取全键；`Split(string)` 仍是 primary。
→ 本 change 对 z42.core 既有导出键**零漂移**，是真正的纯加性变更（无格式 bump、无两代自举需求）。

### partial 碎片归属：同名方法组必须整组同碎片

键的 primary 判定用 `MemberCollector._fillClass` 的**局部** `emittedInst` tracker，而 `_fillClass`
**按 `ClassDecl`（即按碎片）逐个调用** —— 「声明序首个」只在本碎片内计数。同名方法分处两碎片时
双方都取裸键，后合并者在 `ct.Methods` 里**静默覆盖**前者（签名不同 → 不触发 E0433）。
**与 arity 无关**（`Trim()` / `Trim(char[])` 同样会撞）。正解（把 tracker 提升为按类型、碎片已有
确定序）是编译器改动、受 support-先行纪律约束，**留作 Deferred**，本 change 按约束布局即可。

## 验证

- [ ] 5.1 基线 GREEN：`xtask test stdlib z42.core` 49/49 文件（改动前，已取得 ✅）
- [ ] 5.2 refactor 后 `xtask test stdlib z42.core` 全绿 + **导出键与 nightly 逐一对账**（证明纯移动不改键）
- [ ] 5.3 feat 后 `xtask test stdlib z42.core` 全绿（含新测试文件）
- [ ] 5.4 `xtask test` 完整 GREEN
- [ ] 5.5 `xtask test bootstrap`：上一版 nightly z42c 仍能编当前源（无语法/格式/stdlib-API 越界）

## 关联

- 根因修复上游：[`archive/2026-09-04-stabilize-instance-dispatch-keys`](../../archive/2026-09-04-stabilize-instance-dispatch-keys/)
- partial 机制页：[`docs/book/src/language/partial-types.md`](../../../book/src/language/partial-types.md)
