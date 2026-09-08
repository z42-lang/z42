# 跨语言共同陷阱

> 适用于 **z42c 编译器（z42）+ Rust VM + bash 脚本**所有代码路径。
> 这些规则是"曾经因此出过 bug、且与具体语言无关"的横切约束（含 C# 编译器时代的历史案例）。
> 语言专属约定见 [compiler-z42c.md](compiler-z42c.md) / [runtime-rust.md](runtime-rust.md)。

---

## 1. 资源加载顺序必须显式排序（2026-05-17 强化）

**任何"first-wins 注册到全局 key"的资源加载循环都必须先按稳定键 sort 才迭代，禁止依赖 OS / 文件系统 / Dict / HashSet 的"碰巧字母序"。**

### 为什么这条规则

文件系统迭代 API 都**不是** alphabetical：

| API | 实际顺序 |
|-----|---------|
| C# `Directory.EnumerateFiles` | macOS APFS 通常字母序（巧合）；Linux ext4 / btrfs 按 inode；Windows NTFS 多数字母序，但 .NET runtime 版本 + FS 驱动会扰动 |
| Rust `std::fs::read_dir` | 同上；底层 syscall 返回什么就是什么 |
| Bash `for f in dir/*` | shell glob 字母序（这条**是**确定的，shell 帮你 sort 了） |

哈希容器迭代顺序：

| 容器 | 顺序 |
|------|------|
| C# `HashSet<T>` / `Dictionary<K,V>` | 内部 bucket 布局 + string hash；.NET 5+ 默认 string hash randomization → **同二进制每次跑顺序可能不同** |
| Rust `HashMap` / `HashSet` (std) | 默认 `SipHash` 随机种子；同二进制每次跑也不同 |
| Rust `BTreeMap` / `BTreeSet` | 按 key 排序 ✓ 这条**是**确定的 |

任何后续依赖"第一个出现的赢"的 first-wins 逻辑（`TryAdd` / `if (!contains)` / `or_insert` / 类似 pattern）一旦上面任一非确定源进入数据流，**整条解析链都是非确定的**。本地某 OS 上"碰巧字母序"会让你误以为正确，CI 在另一 OS 上炸。

### 现场案例（2026-05-17 fix-depindex-nondeterministic-order）

`PackageCompiler.BuildDepIndex` 用 `Directory.EnumerateFiles(dir, "*.zpkg")` 迭代 → `DependencyIndex.Build` 用 `TryAdd` 注册 `<ShortClass>.<Method>` 静态 key。z42.core 的 `Std.Assert.Equal` 和 z42.test 的 `Std.Test.Assert.Equal` 都映射同一个 key `"Assert.Equal"`，谁先到谁赢。

- macOS：z42.core 字母序在前 → 用户写 `Assert.Equal(1, 2)` emit 到 `Std.Assert.Equal` ✓
- Linux/Windows CI：枚举顺序不同 → emit 到 `Std.Test.Assert.Equal` ✗ → zbc 字节漂移 + 测试输出从 "AssertionError" 变成 "values not equal"

> **本案例的起因已消除（2026-09-08 unify-assert-api）**：两份 `Assert` 已合并成一份
> （`Std.Assert`，命名空间 `Std`、打包在 z42.test），`Assert` 曾是**整个 stdlib 唯一**的跨命名
> 空间同短名类 —— 现在一个都没有了。
> **但本节规则一字不改、继续遵守**：规则管的是「first-wins + 非确定迭代序」这个**模式**，不是
> 这一对类。任何新加的同短名对都会立刻把它带回来，而下一次未必有 CI 帮你在另一个 OS 上炸出来。

> **根治（2026-07-16 fix-crosspkg-static-ns-collision）**：这个「同短类名跨 ns → 短键 first-wins
> 串味」的**根因**已修——z42c 的 `DependencyIndex.GetStaticScoped` 按调用方**活跃命名空间集**
> （usings + 本 ns）解析静态调用，只命中调用方 `using` 到的那份 FQN，不再靠排序碰巧选对。sort
> 仍是**必要的兜底**（活跃集内仍歧义时、以及任何非静态调用的 first-wins 注册仍需确定序），故本
> §1 的排序规则**不变、继续遵守**；根治只是让「跨包静态调用绑错 ns」这一具体症状不再依赖排序侥幸。
>
> **类型引用同款（2026-08-31 fix-type-ref-ns-collision）**：上面修的是**静态调用**；**类型引用**（`new`/
> `is`/`as`/字段·参数类型）此前仍踩同一坑——`SymbolTable.Classes` 按裸类名 first/last-wins，`new A.Foo`
> 被剥短名撞赢家 → 对象身份 emit 成 `B.Foo`。根治：`Z42ClassType` 带 `Namespace` + `SymbolTable.ClassesByFqn`
> 并存 FQN 视图，限定名按 FQN 精确解析、发射端用已解析类型的 `Fqn()`。机制见
> [source-compile.md「同短名跨命名空间的类型解析」](../../docs/book/src/compiler/source-compile.md)。

### 强制规则

写任何"加载 zpkg / 加载 module / 加载 plugin / 注册 builtin"循环时：

1. **加载循环前必须按稳定键 sort 一次**
   - C#：`.OrderBy(stableKey, StringComparer.Ordinal).ToList()`
   - Rust：`paths.sort_by(|a, b| ...)` 或 collect 到 `BTreeSet` / `BTreeMap`
   - bash：shell glob 本身 sort，无需额外
   - 顺序键要语义稳定（prelude-first 后字母序，或纯字母序）；**不能用 mtime / inode / hash code**

2. **不要"碰巧字母序"**：本地某次跑通别窃喜，显式 sort 一次（成本几乎零）

3. **现有 `foreach (.. in hashSet)` / `for .. in hashmap.iter()` + first-wins 写入** 都是潜在 bug —— 见到就加 sort

### 反例

```csharp
// ❌ C#：Linux/Windows 顺序不确定
foreach (var zpkgPath in Directory.EnumerateFiles(dir, "*.zpkg")) {
    staticBuf.TryAdd(staticKey, entry);  // first-wins
}

// ❌ C#：HashSet 迭代顺序不确定
foreach (var path in allPaths)
    foreach (var mod in LoadZpkg(path))
        modules.Add(mod);
```

```rust
// ❌ Rust：read_dir 顺序不确定
for entry in std::fs::read_dir(dir)? {
    let path = entry?.path();
    table.entry(key).or_insert(path);  // first-wins
}

// ❌ Rust：HashMap 迭代顺序不确定（每次跑都变）
for (k, v) in cache.iter() {
    if !result.contains_key(k) { result.insert(k.clone(), v.clone()); }
}
```

### 正例

```csharp
// ✅ C#：显式排序 + prelude-first 语义键
var sortedPaths = Directory.EnumerateFiles(dir, "*.zpkg")
    .OrderBy(p => {
        string name = Path.GetFileNameWithoutExtension(p);
        return PreludePackages.Names.Contains(name) ? "0_" + name : "1_" + name;
    }, StringComparer.Ordinal);
foreach (var zpkgPath in sortedPaths) { ... }
```

```rust
// ✅ Rust：read_dir 后 collect + sort
let mut paths: Vec<PathBuf> = std::fs::read_dir(dir)?
    .filter_map(|e| e.ok().map(|e| e.path()))
    .collect();
paths.sort();
for path in paths { ... }

// ✅ Rust：用 BTreeMap 替代 HashMap 当迭代顺序重要时
let cache: BTreeMap<String, Value> = ...;  // 迭代时按 key 字母序
```

---

## 2. 在作用域 S 内发的号，不得拿到 S 之外做相等比较（2026-09-08）

**任何「计数器发出来的 id」都只在它的发号作用域内唯一。把它拿到更大的范围里当身份用
（相等比较、缓存 key、去重），就是一个必然会撞、且撞了通常静默的 bug。**

### 现场案例（2026-09-08 fix-crosspkg-typeid-collision）

z42 VM 的 `TypeId` 每个 `Module`（≈每个 zpkg）从 0 重开——文档契约当时就写着「per module」，
本身没错。错在两条派发内联缓存（`VCallIC` / `FieldIC`）拿这个裸 `u32` 当**全局**类型身份：

```
site: body.Run(i)              // IParallelBody 接口调用，位于 z42c.semantics
  第一次 receiver = SrcReadHashTask (z42c.driver,    TypeId 139) → 装入 PIC
  第二次 receiver = CompileCuTask   (z42c.semantics, TypeId 139) → 误命中
      ⇒ 跑了 SrcReadHashTask.Run，this._srcs[i] 读到 CompileCuTask 槽 0 的 _cus[i]
```

- 症状离根因十万八千里：`__file_read_text: arg 0 expected string, got CompilationUnit`。
- 触发条件荒谬到没人会怀疑：**从 z42.core 删掉一个无关的类**，把两边的号对齐了而已。
- `FieldIC` 那条更糟：撞键 = 拿到**错误的字段槽**，不崩不报错，**静默读写错数据**。
- 之所以拖到现在才炸，只是因为绝大多数派发站点是**单态**的——撞键要求同一站点先后见到
  两个同号的类。这是运气，不是设计保证。

> 与 §1「加载顺序非确定性」同族：都是**拿一个不保证唯一/稳定的东西当身份用**。
> §1 是顺序不稳，本条是范围不够。

### 强制规则

给任何 id / handle / token 定义**「发号作用域」**和**「比较作用域」**，并保证
**发号作用域 ⊇ 比较作用域**。两者不等时，只有两条出路：

1. **把发号范围提上去**（本次的选择：进程级 `AtomicU32` 批量发号）。
2. **改用天然全局的身份**——指针 / UUID / (scope, id) 复合键。
   同仓先例：`IsaCache` 键 `*const TypeDesc`，正因为它不敢信 id。

写代码时的自查问题只有一句：**「这个 id 是谁发的？会不会有第二个发号者？」**
若答案是「每个模块 / 每个文件 / 每个连接各发各的」，那它就**不能**单独当 key。

### 兜底：让撞键当场炸

范围对齐之后仍要留一道**会响的门**，否则下一次回归又是静默的。做法是在**用这个 id 的
地方**校验一次身份的其它侧面（本次：debug 构建下核对 callee 的 declaring class /
字段槽名），不匹配即 panic。
**代价放 debug 构建、release 编译掉**，热路径不受影响。

---

## 添加新规则的标准

新规则进 `common-pitfalls.md` 要满足**全部**三条：

1. 这个坑可以在 z42 / Rust / bash 任一种语言里出现（不是某语言独有 idiom）
2. 至少出过一次实际 bug（不是预防性脑补）
3. 修复方式是"模式而非具体 API"（"避免 X 类型行为"，不是"换用 Y 库"）

否则该规则属于 [compiler-z42c.md](compiler-z42c.md) / [runtime-rust.md](runtime-rust.md) / 具体设计 doc。
