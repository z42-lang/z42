# Tasks: restore-emit-zbc-diagnostics

> 状态：🟢 已完成 | 创建：2026-09-06 | 完成：2026-09-10 | 类型：fix（程序阶段 ⑦+⑧）

**变更说明：** `z42c --emit-zbc` **丢弃全部编译诊断、以 exit 0 照写产物**。
实测：`NoSuchTypeAtAll x = null;` → 返回 **0** 并写出 285 字节 `.zbc`。而单文件 e2e / golden regen /
bench 直编**全走这条路** ⇒ 这些路径上的编译错误一律静默。本 change 把它改成与 `build` 同口径：
**逐条打印诊断 + 非零退出 + 不写产物**，并清掉门打开后暴露出的全部欠债。

## 为什么这是个「程序」而不是一个补丁

这个洞的后果不是「少看见几条警告」，而是**让一整类 bug 得以长期共存**：
binder 报的错没人看见，emitter 那半边碰巧能跑，测试就绿。
这条线累计修出的真编译器 bug（#491 / #493 / #507 / #523 / #529 / #530 / #543 / #546 / 本 PR）
**共同形状都是「binder 不认 / 元数据抹掉，emitter 却照常发码」** —— 这个不对称是系统性的，
而 `--emit-zbc` 吞诊断正是它的掩护。

## 落地

### ⑧ 开门

`Main.z42` 的 `--emit-zbc` 分支：`IrDump.BuildModuleD(...)` → `cm.ErrorCount > 0` 则逐条打印
`cm.DiagMsgs` + `Environment.Exit(ExitCode.BuildError)` 且**不写产物**；无错才调
`IrDump.ZbcBytesOf(cm)` 序列化。

**结构上的保证（关键，不只是加个 if）**：`IrDump` 只留
`BuildModuleD`（出编译束，含 `ErrorCount`/`DiagMsgs`）+ `ZbcBytesOf`（出字节）两个入口；
旧的 `ZbcBytes` / `ZbcBytesD` 把「编译」和「序列化」**焊死在一次调用里、只返回字节**
⇒ 调用方**在类型层面就拿不到诊断**，诊断无处可去（driver 于是 exit 0 照写产物）。两者已删除。
拆开之后，「丢诊断」不再是能默默做到的事。

### B6 顺带修：单文件路径丢 collector 相位诊断

`IrDump.BuildModuleD` 不 merge `coll.Diags`（包路径 `BuildPackageCus` 本就 merge）⇒
形参 / 字段 / 返回类型位置的诊断整类不可见。补 `diags.MergeFrom(coll.Diags)` 后 E0443 从 3 → 11。

### ⑦ 清机械欠债

门装上后冷扫 642 个单文件语料 + golden regen 全量（299 个），逐类清干净：

| 类 | 条数 | 文件 | 处理 |
|---|---|---|---|
| E0404 私有成员 | 393 + 14 | 51 | 给被访问的成员补 `public`（User 裁决：默认 private 是正确设计，改测试） |
| E0401 `undefined: Utf8` | 23 | 9 | z42.net 测试补 `using Std.Encoding;`（依赖本就在 toml 里） |
| E0439 `0xDEADBEEF`→int | 1 | 1 | 字面量 3735928559 > int.MaxValue ⇒ 补显式 `(int)` |
| E0404 partial method | 2 | 2 | 声明碎片写了 `public`、实现碎片没写 ⇒ 合并后取了实现侧（private）。补齐（C# 要求两侧一致，否则 CS0763） |

其中 126/129 由脚本机械定位（要求能精确定位到唯一声明，定位不到就打印出来交人工，绝不猜），
5 条人工（属性写成 `get_Area()` 访问器 / 单行类体 / partial 第二碎片）。

### 门自己的门

`xtask test compiler` 新增 `_e2eEmitDiagChecks`：
**阳性** = 有错的源 → 非零退出**且产物不存在**（只判退出码不够：写了半个产物照样是错的）；
**阴性** = 干净的源 → exit 0 且产物存在（防「一律报错」这种把门焊死的假修复）。
两半缺一不可 —— 只有阳性会被「无条件失败」骗过，只有阴性会被「无条件成功」骗过。

## 门打开后暴露的真 bug（各自独立成 change，同 PR）

| change | 条数 | 一句话 |
|---|---|---|
| `fix-explicit-type-arg-not-substituted` | 18 | 显式类型实参没代回签名 ⇒ lambda 形参拿到裸 `T` |
| `fix-func-constraint-reported-unknown` | 8 | `where T : Action<int>` 被当成「约束名拼错了」，合法代码编不过 |
| `add-bare-name-ambiguity-diagnostic` | 0（新门） | 裸名跨 ns 歧义此前静默择一，新增 E0456 |

（另有 18 条 `int 不满足 INumber` 已由前一个 PR `fix-inferred-type-arg-not-resolved` #546 修掉。）

## 🔴 语料枚举的坑（踩过两次，写下来）

- lib 单元里**只有 `tests/`·`bench/` 的直接 `.z42` 子文件**走 `--emit-zbc`；子目录是 dir-mode。
- 但 `src/libraries/<lib>/tests/<dir>/source.z42`（无 `[Test]` 的 Main golden）**仍走 `--emit-zbc`**，
  由 golden 腿枚举、**不在 `xtask test stdlib` 里跑**。我的手写扫描按 `-maxdepth 1` 排除了它们，
  于是漏掉 `z42.collections/tests/self_ref_class/`，GREEN 在 build wave 才炸出来。
- ⭐ **教训：别再手写近似语料枚举 —— 直接跑 `xtask build test` 读它的失败清单**，那才是权威枚举。

## 验证

- [x] 门校准：阳性（有错 → exit 1 + 无产物）/ 阴性（干净 → exit 0 + 有产物）
- [x] 冷扫 642 文件：461 条 → **0**
- [x] golden regen 299 个：**全通过**
- [x] `xtask test` 全绿
- [x] 自举不动点
