# Proposal: add-guid-lazy-version

> 状态：DRAFT | 创建：2026-09-03 | 口令「推进 corelib 对齐」backlog #7

## What / Why

给 `z42.core`（prelude）补三个 C# corelib 小类型，对齐 `System` 命名空间：

- **`Std.Guid`**（struct）—— 128-bit GUID。`NewGuid()` 产 RFC 4122 v4（随机）；`Empty()` /
  `Parse` / `TryParse` / `ToString("D"|"N")` / `Equals` / `GetHashCode`。
- **`Std.Version`**（sealed class）—— dotted-quad 版本号。构造 2–4 段 / `Parse` / `TryParse` /
  `CompareTo` / `Equals` / `ToString`（省略未设的 -1 尾段，BCL parity）。
- **`Std.Lazy<T>`**（sealed generic class）—— 延迟 + 记忆化一次性初始化，包 `Func<T>` 工厂。

均落 `z42.core` 而非可选库，因为 C# 里这三者都在 always-available 的 corelib，用户期望无 `using` 即可用。

## 设计决策：OS 熵原语重分类为 core cross-cutting（唯一非纯-additive 部分）

`Guid.NewGuid()` 需要 16 字节 OS 熵。唯一的熵源 `__crypto_random_bytes` 此前其 z42 extern 声明在
**`z42.crypto`**（`SecureRandom`），而 `src/libraries/README.md` 第 2 类把 crypto 定为**可插拔、可裁剪**
的下游库。`z42.core`（prelude，上游）不能反依赖它 → `Guid.NewGuid` 无法直接取熵。

**这是一个规范冲突**（已按 CLAUDE.md「规范冲突检测」上报 User 裁决）：让 `Guid` 参考 C#（入 prelude +
`NewGuid` 可用）与 README「crypto 可裁剪」两条不能同时成立。

**裁决（User，2026-09-03）：把 OS 熵重分类为 core cross-cutting 原语。** 依据：

- `__crypto_random_bytes` 包的是 `getrandom(2)` / `getentropy` / `BCryptGenRandom`——**OS 能力**，
  与时钟 `__time_now_*`（已在 core `Std.Runtime.Clock`）同性质，而非 crypto **算法**（哈希 / HMAC，
  那些才是真正可裁剪的部分）。
- 落法：新增 core `Std.Runtime.Entropy`（镜像 `Clock`）作 `__crypto_random_bytes` 的**唯一声明点**；
  `z42.crypto.SecureRandom` 删除自己的 extern、改**委托** `Entropy.GetBytes`，仍作安全语义门面留在
  可插拔的 crypto 库。→ 单一声明点规则**不破**（只是换了归属库），README 同步更新第 2/3 类 + 单一声明点条目。

**无 Rust / 格式 / 种子影响**：`__crypto_random_bytes` builtin 早已注册在 `src/runtime`；本次只搬 z42
extern 声明，不新增 builtin、不 bump zbc/zpkg 格式、不涉两阶段 nightly 纪律。

## 变更分类

`feat`（stdlib）。三个类型纯 additive；熵重分类是**跨库 native 归属搬迁 + README 规范更新**（非纯
additive，故写 proposal 记录裁决）。无 lang/ir/vm 变更。

## 已知取舍 / Deferred

- **Guid 字节→hex 布局是顺序式**（非 C# 前三组 mixed-endian），但自洽 round-trip 且是合法 canonical
  GUID 串。若将来要与 .NET 线格式逐字节互通，再补 endianness swap。
- **Guid `ToString` 格式**只支持 `"D"`（默认，连字符）与 `"N"`（32 hex）；`"B"` / `"P"` / `"X"` Deferred。
- **`Lazy<T>` 仅单线程记忆化**（对齐 `LazyThreadSafetyMode.None`）；线程安全发布模式待 L3 线程故事。
- **`Version.TryParse`** 返回 `Version`（引用，失败 null，沿 `GetValueOrDefault` 的 null-ref 惯例）；
  **`Guid.TryParse`** 返回 `Guid?`（值类型 nullable）——**首次在 stdlib 用「用户 struct 的 nullable」，
  GREEN 已验证可用**（`Guid?` 编译 + 运行正常）。
- **`default(Guid)` 与 C# 分歧（z42 语言限制，已文档化，非本 change 修）**：z42 里带引用字段的 struct
  其 `default(T)` 产 **Null**（不 materialize 零结构体），故 `default(Guid)` 不等于 `Guid.Empty()`、
  且对其调方法会 trap——须用 `Guid.Empty()`。**尝试改用纯值（两 i64）表示**以让 default 工作时，撞到
  **另一个 runtime bug**：非-`[Record]` 且全原始字段的 struct 走 blob 布局路径时 `StructCopy dst:
  expected StructRef, got Null`（连 `var g = Guid.Parse(..)` 赋值都炸）。故保留 byte[] 表示 + 文档化
  default 限制。根治候选：① 编译器/runtime 让 struct `default` 产零结构体；② 修全原始字段 struct 的
  StructCopy blob 路径。均记入 backlog。
