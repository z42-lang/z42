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

> **根治（2026-07-16 fix-crosspkg-static-ns-collision）**：这个「同短类名跨 ns → 短键 first-wins
> 串味」的**根因**已修——z42c 的 `DependencyIndex.GetStaticScoped` 按调用方**活跃命名空间集**
> （usings + 本 ns）解析静态调用，只命中调用方 `using` 到的那份 FQN，不再靠排序碰巧选对。sort
> 仍是**必要的兜底**（活跃集内仍歧义时、以及任何非静态调用的 first-wins 注册仍需确定序），故本
> §1 的排序规则**不变、继续遵守**；根治只是让「跨包静态调用绑错 ns」这一具体症状不再依赖排序侥幸。

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

## 添加新规则的标准

新规则进 `common-pitfalls.md` 要满足**全部**三条：

1. 这个坑可以在 z42 / Rust / bash 任一种语言里出现（不是某语言独有 idiom）
2. 至少出过一次实际 bug（不是预防性脑补）
3. 修复方式是"模式而非具体 API"（"避免 X 类型行为"，不是"换用 Y 库"）

否则该规则属于 [compiler-z42c.md](compiler-z42c.md) / [runtime-rust.md](runtime-rust.md) / 具体设计 doc。
