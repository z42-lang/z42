# Tasks: 窄基元实例方法的运行期不可达

> 状态：🟢 GREEN 全绿，待提交 | 创建：2026-09-22 | 完成：2026-09-22
> 变更类型：fix（compiler codegen + 一个新 builtin）。**零格式 bump**
> 文档影响：`exec_vcall.rs` 的过期注释修正；本目录记录真因

## 进度概览
- [x] 阶段 1: 判据与目标解析（`EmitContext.NarrowPrimTarget`）
- [x] 阶段 2: `CallEmitter` 静态派发分支
- [x] 阶段 3: `__uint64_to_string` builtin + `UInt64.ToString` 改绑
- [x] 阶段 4: 注释修正（`exec_vcall.rs` 的过期陈述）
- [x] 阶段 5: 测试（修复前后对拍 + 退回对照）
- [x] 阶段 6: GREEN
- [x] 阶段 7: 带走两个欠的归档

## 阶段 1–2: 编译期静态派发
- [x] 1.1 `EmitContext.NarrowPrimTarget(recvType, methodKey)`：8 个「运行期反推会给错类」的基元 →
      包装类方法 FQ；校验确为已发射函数（`LocalClasses` / `Deps.Statics`），否则返回 `""` 回落 VCall
- [x] 1.2 `EmitContext.IsRuntimeAmbiguousPrim(kw)`：sbyte/short/byte/ushort/uint/ulong/long/float
- [x] 2.1 `CallEmitter` 实例路径加分支，形状对齐既有 blob-struct 静态 Call 分支

### 🔴 实施中踩的一个坑（值得记）
第一版判据写成 `IsRuntimeAmbiguousPrim(ct.Name())` —— **只修好了一半调用点**。
同一个 `ulong` 会以**两种拼写**到达发射端：

| 接收者形态 | `Z42ClassType.Name()` | 第一版 |
|---|---|---|
| `ulong a = …; a.ToString()`（显式标注） | `"UInt64"` | ❌ 漏 |
| `((ulong)a).ToString()`（强制转换） | `"UInt64"` | ❌ 漏 |
| `var b = …; b.ToString()`（var 推断） | `"ulong"` | ✅ |
| `UInt64.Parse(…).ToString()`（直接调用） | `"ulong"` | ✅ |

因为 `SymbolTable.BuiltinType` 在包装类已在作用域时**直接返回包装类**，而从方法签名推断出来的
保留关键字拼写。修法 = 先过 `PrimModel.Canon` 归一（正是 drop-short-primitive-aliases 建起来的能力）。

**没有逐形态探针就会漏**：第一次跑探针时 `u64 max` 绿了、`u64 2^63` 还红，两条只差在接收者是
直接调用还是局部变量。故测试里四种形态各钉一条。

## 阶段 3: UInt64.ToString
- [x] 3.1 `builtin_uint64_to_string`（按 u64 重解释再格式化）
- [x] 3.2 登记到 `builtin_table_ext.rs` **表尾追加**（BuiltinId 是下标、会烤进 zbc，只可尾加）
- [x] 3.3 `UInt64.z42` 改绑 + 头注写清「单改这一行无效，真正让它可达的是编译期静态派发」

## 阶段 5: 验证
- [x] 5.1 修复前后同一探针对拍（4 项，见 proposal 表）
- [x] 5.2 `xtask test` 全 stage 绿
- [x] 5.3 `cargo test` 1447 passed / 0 failed
- [x] 5.4 退回对照：`double` 哈希不变、窄整型 Equals/CompareTo/ToString 语义不变、
      Single 与 Double 哈希现在必须**不同**（塌回同一实现即红）

## 阶段 7: 带走两个欠的归档
- [x] 7.1 `drop-short-primitive-aliases`（#730）→ `archive/2026-09-22-drop-short-primitive-aliases`
- [x] 7.2 `symmetrize-primitive-api`（#736）→ `archive/2026-09-22-symmetrize-primitive-api`
- [x] 7.3 两份 tasks.md 状态行从「待提交」转终态（含 PR 号 + 合并 sha）

## 登记（未修，另案）
- 🔴 `ZpkgReader.Read`（`z42.ir`）版本失配时**静默 `return null`**，一条 warn 都不打 ⇒ 依赖包被
  整包跳过，用户看到的是满屏 `undefined: Span`。VM 侧同款失配有极好的报错。属
  [[audit-silent-gates-program]] 同族。
- ⚠️ `ci-bootstrap` 的两代自举**缺「键收敛」一代**。纯格式 bump 不受影响；改派发键的 bump 会让
  gen1 stdlib「声明侧新键、跨包调用侧旧键」自相矛盾（drop-short-primitive-aliases 实测）。
