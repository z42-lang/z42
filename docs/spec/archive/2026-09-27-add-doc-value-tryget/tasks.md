# Tasks: 文档值的「取即检查」—— `TryGet` / `TryGetValue<T>`

> 状态：🟢 已完成 | 创建：2026-09-27 | 完成：2026-09-27
> 分支/worktree：`add-doc-value-tryget` @ `wt-tryget` | 基于：origin/main `ed3713c87` (#878)
> 类型：`fix`（**stdlib API 增补 + 调用点迁移**，无格式 bump、无编译器改动）
> 授权：User「都加个 tryget，然后再加个 trygetvalue<T> 直接返回值的接口，然后简化上层调用」
> 　　　＋「都是返回值 bool，参数是 ref」

## 问题：「取」和「测存在」是两次独立调用

全仓 **87 处**同一形状（`ManifestLoader` 58 / `xtask_bench` 13 / `builder_publish` 多处 / …）：

```z42
if (p.ContainsKey("name")) { name = p.Get("name").AsString(); }
```

`Get` 在三个文档库里**都是抛异常**（`throw new TomlException("key not found: …")`），
不是返回 null ⇒ **这里根本不存在一个「该标 `?` 而标不了」的可空返回**。真正的代价是别的三样：

1. **正确性靠一个编译器看不见的不变式**：「`ContainsKey(k)` 为真 ⇒ `Get(k)` 不抛」是**跨两个
   API 调用**的约定，流分析原理上看不到（同「`Assert.True(a != null)` 不是控制流窄化」）。
2. **键字面量写两遍** —— 改一处漏一处，编译零诊断、运行期才抛。实测目前**两处键不一致的 0 处**
   （没有活 bug），但这是结构性的口子。
3. **同一个表查两遍**（TOML / YAML 的 `ContainsKey` 与 `Get` 都是线性扫描）。

## ⭐ 一条要纠正的记录

`annotate-stdlib-nullable-returns`（#810）把 `TomlValue.Get` 列为「标 `?` 的候选」并试标了一轮，
产生 70 处误报后撤回，留下「要给 TOML 一套『取即检查』的 API 形状」这条 follow-up。

**但 `TomlValue.Get` 从诞生（`2ed56b6a5`，2026-05-14）起就是抛异常**，文件最后一次改动是
2026-06-09，早于 #810 三个多月 ⇒ 判据①（「作者自述返回 null 表示没有」）对它**从来不成立**，
它本就不该进候选名单。撤回的结论对，理由记错了。

⇒ 教训：**「某 API 该不该标 `?`」的前提是它真的会返回 null —— 先读实现，别从调用形态倒推。**

## 设计

每个文档值类（`TomlValue` / `JsonValue` / `YamlValue`）加**两个**方法，**签名同族**：

```z42
public bool TryGet(string key, ref TomlValue value)
public bool TryGetValue<T>(string key, ref T value)
```

- **`bool` + `ref` 出参**（User 定），与 `Int32.TryParse` 那套全仓统一。
  ⚠️ 这不是风格选择：**值类型永不可空**（E0476）⇒ `T? TryGetValue<T>()` 对 `long` / `bool`
  编不过，可空返回这条路走不通。`simplify-ref-parameters` 之后 `TryParse` 全迁 `ref` 正是同一条约束。
- **未命中不动 `value`** ⇒「有就覆盖默认值」一行写完，这是主用法。
- **非 table / object / mapping 返回 false 而不抛** —— 与 `ContainsKey` 一致（它对非表也返 false）
  ⇒ 老写法逐字等价可换，顺带把调用点的 `!X.IsTable() ||` 守卫也吸收掉。
- **命中但类型不符照抛** —— 与 `AsString()` / `AsLong()` 一字不差。「键不在」与「值类型错」是两种
  失败：前者是常态，后者是数据写错了。**吞成 false 会让配置里的类型笔误静默退回默认值。**
- **不支持的 `T` 抛**（不是返回 false）—— 那是**调用方写错了代码**，返回 false 会把它伪装成「键不在」。

### `TryGetValue<T>` 怎么分派

`typeof(T)` + `Type` 值相等（#815）驱动，回转走 `object` 中转：

```z42
if (t == typeof(string)) { object o = v.AsString(); value = (T)o; return true; }
```

⚠️ **`(T)(object)x` 这种链式转换 z42 解析不了** —— `(T)(...)` 被当成调用 `T(...)`，报
`E0401: undefined function: T`。必须拆成一个中间局部。（实测，探针第一版就是这么炸的。）

⚠️ **YAML 的访问器叫 `AsInt` / `AsFloat`**（TOML / JSON 叫 `AsLong` / `AsDouble`），但
`TryGetValue<T>` 按 **z42 类型**选 ⇒ 三个库的调用点写法完全一致。

### ⚠️ 性能上不要过度承诺

`Type ==` 比的是 `FullName`，而 `FullName` 是 **extern 原生属性** ⇒ 每次 `TryGetValue<T>` 要做
1–4 次原生调用。`TryGet` 确实省掉一次线性扫描，但 `TryGetValue<T>` 大致**持平**。
⇒ 真正的收益是**键只写一遍**与**「在不在」成为编译期可见的 bool**，不是速度。
（分派顺序把 `string` 放第一位 —— manifest 里它占绝大多数。）

## 进度概览

- [x] 1 三个库各加 `TryGet` / `TryGetValue<T>`
- [x] 2 三套测试（18 条，各钉三条不变量）
- [x] 3 上层调用迁移（5 个文件，`ContainsKey` 全部归零）
- [x] 4 全量 GREEN + 文档 + PR

## 3 迁移账

| 文件 | 迁移前 `ContainsKey` | 迁移后 | 备注 |
|---|---|---|---|
| `z42.project/ManifestLoader.z42` | 58 | **0** | 旗舰样板 |
| `toolchain/builder/core/builder_publish.z42` | 24 | **0** | 顺带消掉一批 `!X.IsTable() \|\|` 守卫 |
| `scripts/xtask_bench.z42` | 13 | **0** | JSON 侧 |
| `builder_test.z42` / `agent/src/agent.z42` | 各 2 | **0** | 三元形态 |

**留着不动的**：`X.Get(keys[i])` 那一类（`Keys()` 遍历，键必然存在）—— `ManifestLoader` 里 8 处。
那不是「先测再取」，没有第二次查找，也没有键重复。

**明确不碰**：`Dictionary` / `StrMap` 那一族（`StructLayout` / `SymbolTable` /
`ImportedSymbolLoader.Resolve` 共约 6 处）。它们是**另一个容器族**，`StrMap.Get` 已经是
`object?` 且 #810 已标 `?`；把它们一起改属于另一刀。

## 4 收尾
- [x] 4.1 `xtask test` 全量 GREEN（**11m12s / 15 stage 全过**）
- [x] 4.2 文档：`reference/stdlib/` 的 `toml.md` / `json.md` / `yaml.md` 各补「取即检查」一节，
      并把 `toml.md` 的用法示例改成新写法
- [x] 4.3 归档（阶段 9，在本 PR 内）→ `archive/2026-09-27-add-doc-value-tryget/`

## ⚠️ 本轮踩的两个构建坑（都值得记）

🔴 **改了 stdlib 之后重建 xtask，必须用 `artifacts/.z42/z42`，不能用顶层 `./.z42/z42`** ——
后者是**种子 SDK**，`xtask build sdk` 装的是前者。用种子去编 `scripts/`，新加的 stdlib 方法
一律 `E0401: no method TryGetValue on JsonValue`，看起来像「方法没加上」，实际是**拿旧库编新代码**。
判据：`strings artifacts/.z42/libs/z42.json.zpkg | grep TryGetValue` 有、顶层 `.z42/libs/` 那份没有。

🔴 **顺序是 `build sdk` → 删 `xtask.zpkg` → republish**。反过来先删 zpkg，`./xtask` 自己就启动不了
（`cannot read artifacts/xtask/xtask.zpkg`），而 `build sdk` 正要靠它 —— 自己造的鸡生蛋。

🔴 **供种时别跨 worktree 拷 `artifacts/build/runtime`**（那是 cargo target-dir）：
`CMakeCache.txt` 里烘焙着**源 worktree 的绝对路径**，debug 构建会炸在
`CMake Error: The current CMakeCache.txt directory … is different than the directory …`，
而报错指向的是 `libz-ng-sys` 这种与本次改动毫无关系的地方。清法：`rm -rf artifacts/build/runtime` 后重建。
