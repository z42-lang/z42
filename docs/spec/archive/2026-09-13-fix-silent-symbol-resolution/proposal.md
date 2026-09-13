# Proposal: 缺符号不再静默 —— 用到才抛（可 catch 的类型化异常）

> **状态：🟢 完成**（2026-09-13 落地）| 创建：2026-09-12 | User 2026-09-12 裁决「用到才抛」
> 前置已满足：`available!()`（PR #540）已合并 —— 它是本 change 的**唯一显式豁免通道**，
> 没有它，所有 guarded 降级代码会在本 change 落地后全部炸。

## Why

依赖 zpkg 版本 skew（编译时依赖 v2、运行时加载到 v1）在 z42 里**几乎全是静默错误答案，
不是报错**。六条实测确认仍在（2026-09-12 在 `c58d775c` 上复核）：

| # | 场景 | 现在的行为 | 位置 |
|---|---|---|---|
| ① | 读缺失的静态字段 | **静默 `Value::Null`**（`resolve_static_field_id` 注释直言 "always succeed"） | `vm_context/statics.rs:55` |
| ② | `new` 缺失的类型 | **合成零字段零 vtable 空壳** | `interp/exec_object.rs:60` |
| ③ | ctor 缺失 | **照常把未初始化对象写进 dst** | `interp/exec_object.rs:131` |
| ④ | 基类被删/改名 | 子类**静默退化成「无基类」**，丢掉全部继承字段和 vtable | `loader/type_registry.rs:215,230` |
| ⑤ | 静态 `Call` 缺失（interp） | 报错，但是**不可 catch 的 VM abort** | `interp/exec_call.rs:174` |
| ⑥ | 同上（JIT） | 抛**裸 `Value::Str`** —— 只能被无类型 `catch {}` 捕获 | `jit/helpers/call.rs:168` |

**根因是单一的**：`UNRESOLVED` 这个哨兵在设计上**同时编码「跨包待解析」和「根本不存在」**
（`metadata/resolver.rs` 注释写死了这一点），所有下游兜底都按前者处理——一路 fallback、
合成、返 Null。

这直接违反 [philosophy.md](../../../../.claude/rules/philosophy.md) 点名的反例：
> ❌ 在解析失败时降级为 sentinel 值，让下游用启发式去「猜」

**这是既有技术债，不是新需求。**

## What Changes

**在每个使用点，当解析「确定失败」（所有回落路径都穷尽之后）时，抛出可 catch 的
类型化异常，而不是返回哨兵值。**

- 新增 `Std.MissingSymbolException`（消息含符号全名与种类）。
- 上述 ①②③④⑤⑥ 六处改为抛它。
- ⑤⑥ 顺带**统一两个后端**——同一场景现在一个是不可 catch 的 abort、一个是裸字符串。

### ⚠️ 关键边界：只在「确定不存在」时抛

②④ 的兜底**本来是为跨包两阶段加载服务的**（基类还没加载 ≠ 基类不存在）。必须只在
**所有惰性加载路径穷尽之后**才抛：

- ② `ObjNew`：`module.type_registry` → `try_lookup_type`（会触发包加载）→ 都落空才抛
- ④ 基类：`try_fixup_inheritance` 的**定点循环收敛之后**仍未解析才算真缺失

把这条判错会把正常的跨包加载变成崩溃。

## 机制已经现成（做静态构造器时铺好的）

`add-static-constructors` 刚刚把「从 Rust 抛出可 catch 的类型化异常」这条管道在**四个
触发点 × 两个后端**上走通并验证过：

- 构造：`exception::make_stdlib_exception(vm, module, "Std.X", msg)`，逐级回落
- interp 抛出通道：返回 `Ok(Some(exc))`（**不是** `bail!` —— 那条走 anyhow Err，
  不经 `find_handler`，用户 `catch` 抓不到）
- JIT 抛出通道：`set_exception(vm, exc)` + 返回错误码，translate 端 `self.check(ret)`
- 两后端共用同一份判定实现，避免语义漂移

本 change 直接复用这套，不需要新造管道。

## Scope（允许改动的文件）

- `src/libraries/z42.core/src/Exceptions/MissingSymbolException.z42`（新）
- `src/runtime/src/vm_context/statics.rs`（①）
- `src/runtime/src/interp/exec_object.rs`（②③ + 实例字段读写）
- `src/runtime/src/jit/helpers/object.rs`（同上，JIT 侧）
- `src/runtime/src/metadata/loader/type_registry.rs`（④）
- `src/runtime/src/interp/exec_call.rs` / `src/runtime/src/jit/helpers/call.rs`（⑤⑥）
- `src/tests/` skew 用例（复用 `available!` 那套 `skew-absent.txt` 脚手架）
- `docs/book/src/runtime/`

## Out of Scope

- **急切的全程序 link 校验**（`--verify-links`）——独立后续。它与本 change 互补：
  开发/CI 用急切校验抓 skew，生产用「用到才抛 + `available!` 降级」。
- **包级版本元数据**（`ZpkgDep` 加 version）——独立 change。

## 预期代价（必须认清）

**会让今天绿的东西开始红。** 静默返回 Null 的地方可能有代码正依赖着这个行为，而 skew
场景**零测试覆盖**，所以谁都不知道有多少。这不是坏事——那正是本 change 要暴露的——
但要预期一轮清理，且 GREEN 第一次跑大概率不绿。

## 实现顺序（逐站点落地，每站点单独验）

一次改六处、GREEN 一起红，会分不清哪条是真欠债、哪条是我改错。故**一次一个站点**：
⑤⑥（两后端统一，最独立）→ ① → ③ → ② → ④（最危险，涉及跨包两阶段加载）。


---

## 落地结果（2026-09-13）

六个站点全部落地，**每个站点单独一个提交、单独跑一遍完整 GREEN**（按 DRAFT 定的顺序
⑤⑥ → ① → ③ → ② → ④）。机制页：
[`docs/book/src/runtime/missing-symbol-resolution.md`](../../../book/src/runtime/missing-symbol-resolution.md)。

| 提交 | 内容 |
|------|------|
| `25e04aed` | ⑤⑥ 缺函数抛可 catch 的 `MissingSymbolException`，两后端统一 |
| `216191e6` | ① 缺失静态字段不再静默读出 `Null` |
| `efa90be2` | （顺带）值类型静态字段无初始化器时读出 `Null` 而非零值 |
| `0a43df9b` | ③ 缺失构造器不再静默写未构造对象 |
| `c4d61e4e` | ② `new` 解析不到的类型不再合成零字段空壳 |
| `5f53eb8d` | （顺带）主模块的类继承跨包基类时丢掉全部继承字段 |
| `a9e10d88` | ④ 基类解析不到不再静默丢掉整片继承面 |

### 与 DRAFT 的偏差

1. **③ 的判据不是「名字解析不到」，是「有没有实参」。** DRAFT 默认「ctor 名解析不到」
   即缺失，实测站不住：z42c 对无构造器的类照样发射 `ObjNew`、ctor 键取裸类名，而这与
   **单构造器**的 primary 裸键同形。改判据为 `argc > 0`（没有构造器的类不可能接受实参）。
   `argc == 0` 是有意保留的残留缺口，见机制页。
2. **④ 的「定点循环收敛之后」不可直接用。** 按需加载下，加载期收敛 ≠ 最终态（下一个
   zpkg 随时可能带来基类）。改为在 `ObjNew` 用**使用点**判定：`base_unmerged` 旗子 +
   先去惰性加载器取一份（内部会 `ensure_base_chain_loaded` + 跑到不动点），取回来仍带
   旗子才定案。与 User 裁决的「用到才抛」一致。
3. **多修了两个既有 bug**（不在 DRAFT 里，都是被静默行为盖住的）：主模块类继承跨包基类
   丢字段、JIT 的回落描述符是空的。前者与 skew 无关、基类在场也照样错。

### 「预期会让今天绿的东西开始红」—— 实际没有

DRAFT 预判会有一轮清理。实际六个站点全部落地后 `xtask test` / `cargo test` /
`test stdlib --mode jit` 一次全绿，真欠债 0。原因大概是判据都收得足够紧（「证不出来就
放行」），以及 stdlib / z42c 本来就没有跨版本 skew。

### 每个站点都有「修复前」实测对照

不是推理，是把判定临时关掉、跑同一份产物量出来的：

| 站点 | 修复前实际输出 |
|------|---------------|
| ③ | `constructed 0` —— 若无其事，字段是零值 |
| ② | `VCall: expected object, got Null` —— 不可 catch，现场离根因十万八千里 |
| ④ | `constructed B=3` —— 整片继承面没了，程序毫无察觉 |
| 跨包基类 bug | 急切副本 1 个字段槽 / 惰性副本 2 个（直接打印证实） |

站点 ④ 还在**完整 GREEN 负载下**加探针实测过：整轮只触发 1 次（就是它自己的用例），
无任何误伤——用来排除「它是不是把 `Z42NetHttpServerPoolTests` 弄红了」。那条偶发红是
既有的 `concurrency-null-thread-flake`，关掉站点 ④ 同样会红、开着也同样会绿。

### 测试脚手架的扩展

`src/tests/cross-zpkg/` 新增 `skew-replace.txt` + `oldtarget/`：此前只有
`skew-absent.txt`（依赖整包消失），演示不了「类型还在、某个成员没了」——而后者才是版本
skew 的常态。约定写进 `src/tests/cross-zpkg/README.md`。

新增用例（每个抛异常的用例都配一个对照组）：

- `missing_ctor_skew` / `missing_ctor_present`
- `missing_type_skew`
- `missing_base_skew` / `crosspkg_base_fields_main`
- `symbol-resolution/static_field_zero_init`

### 仍然开着的后续

- ③ 的 `argc == 0` 缺口（要动元数据：让 `TypeDesc` 记录本类声明了哪些构造器）。
- DRAFT 已列的 Out of Scope 两项：急切全程序 link 校验、包级版本元数据。
