# fix-runtime-constraint-unresolved-refs — 加载期约束校验对「未解析接口引用」的 fail-closed 修复

## 背景

`src/runtime/src/metadata/loader/constraints.rs::check_one`（模块加载期 `verify_constraints`
调用）把约束引用的类型若不在 `type_registry`、非 `Std.*`、又不叫 `I`+大写 → `bail!` 整模块加载失败。
这是 [`add-associated-types-program`](../../../../.claude/) gap 扫描 Tier C 的 **C1**（真 footgun）。

- **注释根因已过期**：注释称「type_registry 只装 class」——**假的**。`read_type` 把接口 TYPE
  entry 照建成 `ClassDesc` → 进 `module.classes` → `build_type_registry` 用名字作 key 收进
  registry；反射 `typeof(接口)` 走的正是同一个 `type_registry.get(name)`（zbc 1.19 反射起接口
  即注册表条目）。命名启发式是过期兜底。
- **它真实还在挡的**：`verify_constraints` 在 `merge_modules → build_type_registry` 之后、
  `boot_context`（惰性加载器在此才创建）**之前**跑 ⇒ 惰性依赖 zpkg 尚未合并。启发式实际放行的是
  「引用了惰性依赖里、恰按 `IFoo` 命名的接口」的约束（同 `Std.*` soft-allow）。
- **footgun**：任何**不叫 `IFoo`** 的接口（`Comparable` / `Iterable` / `Ord` / `Numeric`……，
  I 后小写也算）一作约束就在加载期炸。仓内接口一律 `IFoo` 命名 ⇒ 今天零活跃受害者（闭洞型），
  但对用户与未来 stdlib 是即炸陷阱。

## 变更

**原则：加载期约束校验不得把「此刻解析不到接口」当「约束违反」。**

`check_one` 加 `soft_allow_unresolved: bool`，`check_constraint_refs` 按引用**种类**分派：

- **接口引用**（`b.interfaces`）→ `soft_allow_unresolved = true`。未解析时容忍（延后到运行期真正
  使用点——那里解释器会自然触发惰性加载并报真错），与 `Std.*` 同理。**删掉 `I`+大写命名猜测。**
- **基类引用**（`b.base_class`）→ `false`，保持现状严格（基类是布局/派发关键，且跨包基类**约束**
  极罕见；bogus 基类是完整性问题）。
- **func-sig 类型引用**（`where T:Func<…>` 的签名位）→ `true`。这类约束**编译期专属、运行期无判定**
  （ConstraintChecker.z42:13）且源码零用例；其 runtime 检查纯 tamper 兜底，可能命名接口，故容忍未解析
  以免删启发式后回归。

C1 处惰性加载器尚未存在（`verify_constraints` 在 `boot_context` 之前），故**不能**「先触发惰性
加载再查」——按引用种类 soft-allow 是该时序下的正解（User 裁决 2026-09-15 方案 A）。

## 已查实非目标：C2（反射 enum 约束）**无 bug**（勿再开）

gap 扫描原列 C2「enum 约束编译/运行分歧：未解析实参运行期 `validate_type_arg_constraint` 误 bail」。
**走链路发现前提过期、无可修 bug**（本程序第 7 次「Deferred 前提过期」）：

- `validate_type_arg_constraint` 的 `is_enum = resolve_td(ctx, arg).map(is_enum).unwrap_or(false)`；
  而 `resolve_td = registry.get(name).or_else(|| ctx.try_lookup_type(name))`——**第二支触发惰性
  加载**（自 #281 建文件起就在）。跨包 enum 会被先加载、`is_enum` 正确求值。
- `unwrap_or(false)` 只在类型**真正加载不到**（不存在于任何声明 zpkg）时兜底 bail——那正是反射
  自我监管应有行为（doc 明载「反射构造非法类型必须自police」）。
- backlog 记的「resolve_td 只查表不加载」是当时推断，与现码不符。**C2 已闭合，不立项。**

## 非目标

- 不改编译期语义、不移动 `verify_constraints` 调用点（方案 B 已否）、不改 zbc/zpkg 格式。
- 不动基类约束的严格校验（tamper 检测保留）。

## 影响面

- **无格式 bump**：纯 Rust runtime 逻辑，无 IR/wire 变更。
- **比现状 tamper 检测更严**：非 `IFoo` 命名的类型不再被无脑放行、基类仍严；只对「接口/func-sig
  引用未解析」这一类容忍（本就该延后）。
- **爆炸半径**：仓内零活跃受害者（接口一律 `IFoo`）；放宽的是误拒，不会让原本能加载的模块加载不了。
