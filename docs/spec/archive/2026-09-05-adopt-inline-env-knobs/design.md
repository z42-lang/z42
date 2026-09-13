# Design: 收编 8 个内联 env 旋钮

> 状态：🔴 DRAFT | 提案 [proposal.md](proposal.md) | spec [specs/runtime-settings/spec.md](specs/runtime-settings/spec.md)

## Decisions

### Decision 1：收编是性能**收益**，不是为统一性付税

直觉上"多一层间接"应该更慢，实际相反：

| | 每次读的代价 |
|---|---|
| `std::env::var("X")` | 加锁进程环境块 + 线性/哈希查找 + **分配一个 `String`** |
| `runtime_config().x` | `OnceLock` 的一次 acquire load + 字段读（`bool`/`u32` 是寄存器级）|

`metadata/superinstr.rs::fuse_blocks` 每识别一个方法的基本块就读**三次** env；
`corelib/repl_native.rs` 在 dlopen 候选枚举里读。这两处收编后直接变快。

`jit/mod.rs` 的两个 threshold 只在 JIT 模块构造时读一次，收编与否无所谓——但让它们
和其余旋钮同源，是"表是唯一 SoT"的兑现。

**唯一要小心的**：`interp/stack_alloc.rs::mode()` 在**解释器分配热路径**上。它现在用
`AtomicU32` 缓存首次解析结果。**这个缓存保留**——它省的是 5 路 `match` + 字符串比较，
不是省 env 读；换成 config 后仍然值得缓存。

### Decision 2：四个 `Flag` 转真 `Bool` —— shell 惯例活不过配置文件

`ValueKind::Flag` 忠实描述了现状（"存在即启用"，`Z42_NO_FUSION=0` 仍然关闭 fusion）。
那是 shell flag 的通行惯例，在 env 层说得通。

但**一旦这些旋钮能写进 `[runtime]` 表，Flag 语义就变成陷阱**：

```toml
[runtime]
no-fusion = false     # Flag 语义下：这是"设了" ⇒ fusion 被关掉
```

没有任何人会期待这个结果。TOML 的 `false` 就是假，配置文件里的布尔必须是布尔。

所以**开放层与转换类型必须同时发生**——不能只放开 `sources` 而留着 Flag。转换后：
`0/false/off/no` → 关，`1/true/on/yes` → 开，其它 → 类型非法（诊断 + 默认），
与 `Z42_GC_TRACE` / `Z42_JIT_PROFILE` 完全一致。

**破坏面已探查**：`scripts/` / `.github/` 零消费方，`docs/` 只有归档 spec 的散文提及。
四个都是 `tier: Internal` 的调试开关。会变的只有"显式把 env 设成 falsey 字符串却期望
它生效"这一种用法——那种用法本身就是在利用一个 footgun。

### Decision 3：`Z42_STACKALLOC` 的 typo 由 Enum 校验兜住，消费点的 match 不动

现状 `match` 的兜底臂是 `_ => MODE_ON`，于是 `Z42_STACKALLOC=of`（拼错 `off`）静默变成
"开"——用户以为关掉了优化在做 triage，实际没关。

收编后解析层的 `ValueKind::Enum` 校验先跑：表外的值 → `Invalid` 诊断 + 该层不生效 →
消费点拿到 `None` → 兜底臂给 `MODE_ON`（默认开）。**合法值行为逐字不变，typo 从静默
变明说。**

消费点的 `match` 保持原样（含 `_ => MODE_ON`）：它现在是"默认值"的表达，不再兼职
"错误处理"。两件事分开是好事。

### Decision 4：`no_typed_fusion` 存"旋钮的字面语义"，不存反向

env 名是 `Z42_NO_TYPED_FUSION`，字段就叫 `no_typed_fusion: bool`，消费点写
`let typing_enabled = !cfg.no_typed_fusion;`。

不在 config 里存 `typed_fusion_enabled`（正向）——那样字段名与旋钮名对不上，
`--show-config` 打印 `no-typed-fusion` 而结构体里叫别的，读代码的人要在脑子里做一次
反转。**表里的名字是唯一 SoT，字段跟着它走**；反转发生在唯一需要它的那一行。

### Decision 5：两个 threshold 的 clamp 语义原样保留

现行是 `parse::<u32>().ok().unwrap_or(default).max(1)`——**解析失败静默落默认**，
且任何 `0` 被 clamp 成 1。

收编后：解析层的 `ValueKind::Int{min:1,..}` 只验"能不能解析成整数"（范围校验按
`complete-runtime-settings` 的既定分工留在 parser），parser 保留 `.max(1)`。
于是 `Z42_JIT_THRESHOLD=abc` 从"静默落默认"变成"诊断 + 落默认"，`=0` 仍然 clamp 成 1。

## Implementation Notes

- 新增 `parse_u32_knob(get, name, default)` 到 `config/parse.rs`（两个 threshold 共用），
  语义 = 现行的 `unwrap_or(default).max(1)`。
- `stack_alloc.rs` 的 `mode()` 只改数据来源：
  `runtime_config().stackalloc.as_deref()` 替 `std::env::var(..).ok().as_deref()`。
  ⚠️ 注意生命周期：`runtime_config()` 返回 `&'static`，`as_deref()` 直接可用，
  不需要现行的临时 `String` 绑定。
- `config.rs` 现 307 行 + 8 个字段 + 8 行解析 ≈ 340 行，仍在软限内。
- 登记表的 `INLINE_ENV` / `INLINE_ENV_INTERNAL` 两个基线在本 change 后**只剩
  `Z42_STRESS_ITERS` 用**（它是 `ENV_ONLY` 但不是 inline-env）。把两个基线删掉，
  `Z42_STRESS_ITERS` 直接写 `sources: LayerMask::ENV_ONLY, ..`。

## Testing Strategy

| 层 | 测试 |
|---|---|
| 四层可设 | 每个新字段：CLI / env / user-config / app-config 各能设，且优先级正确 |
| Flag→Bool | `no-fusion=false` / `=0` / `=off` → **不**关闭 fusion；`=true` / `=1` → 关闭；`=maybe` → 诊断 + 默认 |
| Enum typo | `Z42_STACKALLOC=of` → 诊断含 "expected one of"，落默认（开）|
| threshold | 默认 2 / 10000 不变；`=0` clamp 到 1；`=abc` → 诊断 + 默认 |
| 非破坏 | 合法值下所有 8 个字段的取值与本 change 前逐一相同 |
| 防腐门 | 表内不再有 "(inline env read)"；源码扫描门仍绿 |
| e2e | `z42vm --set jit-threshold=5 --show-config` 显示 `[cli]`；`--set no-fusion=false` 不再报 "cannot be set from [cli]" |
