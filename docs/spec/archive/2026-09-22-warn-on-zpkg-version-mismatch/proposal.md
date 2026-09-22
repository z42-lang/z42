# Proposal: zpkg 版本失配不再静默跳过

> change: `warn-on-zpkg-version-mismatch` ｜ scope: `stdlib` + `compiler` ｜ 无格式 bump
> 来源: [[primitive-silent-defects-program]] 登记的余项 ①

## Why

`ZpkgReader.Open` 对**版本失配**的处理是一条光秃秃的 `return null`，**一个字都不打**：

```z42
if (major != ZpkgWriterZ.Major || minor != ZpkgWriterZ.Minor) { return null; }
```

调用方（`DepScan.ScanDirs` / `DepScanCache.Get` / `DepReconcile` / `TsigReconcile`）拿到 null
就当这个包不存在。**而「不存在」不会失败**——它在很远的地方以完全不相干的形态浮出来：

```
E0401: undefined: DiagnosticCodes
E0443: undefined type: Span
...（满屏）
```

真因是「整个 `z42.ir` 包被跳过了」，用户看到的却是「你的代码引用了不存在的类型」。
实测为此二分过三轮（记录在 [[local-two-gen-bootstrap-recipe]]：「一大片 undefined 先怀疑整包被跳过」）。

**同一个仓库里已经有一条好报错可以照抄**——VM 侧 `src/runtime/src/app.rs` 的
`version_mismatch_hint`，它的设计说明写得很清楚：

> version mismatches get named **at the point of detection**, with the command that fixes them.
> Every other read failure keeps its warn-and-continue behavior.

编译器侧缺的正是这一条。属 [[audit-silent-gates-program]] 同族（「静默的跳过」是假保障的一种）。

## What Changes

1. `ZpkgReader.Open(byte[] data)` 增加一个重载 **`Open(byte[] data, string origin)`**，
   `origin` 是这份字节的出处（文件路径；REPL 那种内存包传包名）。旧签名保留、委托给新的，传空串。
2. 版本失配时调 `_warnVersionSkew(origin, major, minor)` 往 **stderr** 打三行：
   - 跳过了谁、它是哪个格式版本、本工具链读哪个版本（strict-pin 无跨版本兼容）；
   - **为什么你会在别处看到一堆 `undefined`**（把真因和症状连起来，这是最值钱的一句）；
   - 怎么修（`xtask build stdlib`，或换回产出它的工具链）。
3. **按版本去重**：一个过期的 libs 目录常有几十个同代旧包，逐个报会把真信号淹掉
   ⇒ 同一个 `<major>.<minor>` 只报一次，并明说「同版本的其余包不再重复」。
4. 6 个调用点把手上已有的路径传进去（5 处是 `File.ReadAllBytes(<path>)`，路径就在眼前）。

**非版本原因的读失败（坏 magic / 太短 / SymOnly sidecar）维持静默跳过**——那些确实可能是
无关产物，与 VM 侧的分界一致。

## 风险

- 新增的是**重载**：`Open(byte[])` 仍是先声明的那个，沿用 #414 的 primary-bare 键
  ⇒ 既有调用点的派发键不变。
- `z42.ir` 新增方法、同 PR 被 `z42c.pipeline` 调用 = **一层**跨成员新符号。GREEN 的
  `seed cold-start` stage 正好压这条路径；若它红，回落到「不带 origin 的版本」（告警仍在，只是少个路径）。
- 库往 stderr 写：`z42.build` / `z42.cli` / `z42.net` 都有先例。
