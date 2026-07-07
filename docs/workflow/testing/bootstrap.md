# 自举与测试流程（本地 + CI）

> 权威的**操作层**流程文档：z42c 自举的工具链如何从一个下载的 SDK 起步、编出当前版本、
> 并被测试/打包/发布。设计原理（src/compiler 架构、
> 受限写法、对账策略）见 [`docs/design/compiler/self-hosting.md`](../../design/compiler/self-hosting.md)；
> CI 总览见 [`ci.md`](../ci.md)；测试层级见 [`README.md`](README.md)。

---

## 1. 核心模型：两个 matched set + 成对分代

z42 工具链用 z42 写、编成 zpkg。要编它先得有一个能跑的 z42c —— 鸡生蛋；打破它靠一个**已存在的种子**。

**版本成对匹配（底层硬约束）**：strict-pin（`zbc_reader.rs` major/minor 精确匹配）→ **z42c(emit 格式) +
z42vm(read 格式) + stdlib(身处格式) 必须同格式 = 一个 matched set**。任何时刻有且只有两个 set：

| | 来源 / 内容 | 职责 | 位置 |
|---|------|------|------|
| **SDK set** | 下载的**上一版 nightly**（`install-z42`）：`{z42c, z42vm, stdlib}` 同旧格式 + launcher，**无 xtask** | ① 打破 chicken-egg：编出 xtask(驱动) + gen1；② 保留供交叉验证/差分 | `.z42/` |
| **本地 SDK set** | 当前源编出：`{z42c, stdlib}`=gen2 对 + z42vm(cargo) + toolchain，同当前格式 | **被测试、被打包、成为下一个 nightly** | `artifacts/.z42/` |
| **xtask** | 项目构建工具（`scripts/`），由 SDK 编出驱动构建 | 构建驱动 + 交叉验证靶子，**不进 SDK / 不分发** | `artifacts/xtask/` |

> **z42vm 角色**：cargo 建的原生件,不在 z42c 编译链,但格式必须配它要跑的 zpkg。跑 SDK z42c(旧格式)用
> 旧 z42vm,跑 gen2(当前格式)用当前 z42vm。不 bump format 那些轮一个 z42vm 通吃;bump 轮过渡期要两个。

**成对分代 `{z42c, stdlib}`**（二者耦合,作一对翻代）：
- `gen1` = **SDK 编 `{stdlib, z42c}`**：行为当前、字节由 SDK codegen 出（stdlib 先编 → z42c 用它解析）。
- `gen2` = **gen1 编 `{stdlib, z42c}`**：字节也当前 = 当前 codegen+格式。**发布的是 gen2 这一对**。
- `gen3` = **gen2 编 `{stdlib, z42c}`**：仅不动点检查,条件触发（见 §交叉验证）。

> **为什么发布 gen2 而非 gen1**：gen1 字节由 SDK（旧 codegen/格式）emit。改 codegen 时 gen1 字节旧;format
> bump 时 gen1 的 stdlib 旧格式、当前 z42vm strict-pin 读不了 → 跑不了。gen2 对 = 完全当前 = 永远能跑。

- **平台无关 vs 平台相关**：set 里的 **zpkg（z42c/stdlib）是平台无关字节码**——编一次全平台共享。**z42vm 是原生二进制**——每个 host OS 各自 `cargo build`。
- **`--toolchain <dir>` 选择器**：build/test 据此定位用哪个 set（`.z42` = SDK，`artifacts/.z42` = 本地）。两套布局一致 → **交叉验证天然可做**。

### 交叉验证 + 三个正交属性

下载的 SDK set 不在编完后丢弃,而是**留着对账**;发布前必须过**三个互相独立**的属性,缺一不可：

- **完整**（self-hosting closure）：当前工具链能从源码编出完整 `{z42c, stdlib}` 不报错。由 gen2 编出来 + cross-bootstrap 重建验。
- **稳定**（不动点）：`{gen2} == {gen3}` 逐字节（mod BLID）。证编译器自编自得到一样字节。**gen1==gen2 时自动成立 → 跳过 gen3**（条件触发）；只在改 codegen（gen1≠gen2）时真编 gen3。
- **正确**（行为）：gen2 工具链跑 **vm goldens + stdlib [Test] + cross-zpkg** 全绿。**`{gen2}=={gen3}` 只证稳定不证正确**——"稳定地错"（gen2/gen3 同 bug）也能过不动点 → 必须靠测试套件单独验。

> 三者正交：完整 ≠ 稳定 ≠ 正确。光不动点（稳定）不够,必须叠加测试（正确）。三关都进发布 gate（见 §4.2）。

---

## 2. 共享 host SDK：编一次，全 job 复用

**所有 job 都先需要 host 平台的 SDK**——连移动/wasm 打包也要 host 上的 z42c 来驱动交叉编译。
所以 host SDK（Current toolchain 的 zpkg 部分）**编一次、上传 artifact、全下游复用**，不重复编。

```
            ┌─ host-test (per OS)      下载 host-SDK + cargo z42vm + 跑测试
host SDK ───┼─ host-package (per OS)   下载 host-SDK + cargo z42vm + 打包
（编一次）  ├─ jit-test (linux, 分片)   下载 host-SDK + cargo z42vm + jit
            ├─ package-{ios,android,wasm}  下载 host-SDK + cross-target 编译
            └─ test-{ios,android,wasm,desktop}  下载 host-SDK + 平台测试
```

> **现状（2026-06-27）**：9 个 package/platform job 已消费 host-SDK artifact（`xtask-bootstrap-artifact`
> action）✓。但**测试 job（build-and-test / vm-jit / stdlib-jit）仍各自全量
> 重新自举**（`ci-bootstrap.sh`，~12min/job）——这是当前最大的冗余（见 §5）。

---

## 3. 本地流程

```
# fresh checkout（无 warm 产物）：
bash scripts/install-z42.sh          # 下载 SDK → .z42/
# （后续：SDK 编 xtask → xtask 编 current → 测试；目标由单一入口驱动）

# warm（已有 artifacts/.z42 的 Current）：
xtask test                  # GREEN gate（用 Current）
xtask test e2e --mode jit           # 单独 jit
xtask build sdk             # 重建 Current toolchain
```

- **无非-z42 兜底**：fresh checkout **必须**先下载 SDK 才能起步（需 gh auth + 网络）——工具链没有任何非-z42 的逃生编译器。
  无 warm 种子时 `xtask build` 明确报错引导跑 `install-z42` / 下载流程。
- **GREEN gate**：`xtask test` = cargo z42vm + 用 Current 跑 vm(interp)/cross-zpkg/stdlib/
  compiler。jit 由 `test e2e --mode jit` / CI 的 jit 专腿覆盖。详见 [`.claude/rules/workflow.md` 阶段8]。

---

## 4. CI 流程

### 4.1 目标拓扑（compile-once）

> 成对分代 `{z42c, stdlib}`：gen1 = SDK 编一对（字节旧 codegen）；gen2 = gen1 编一对（字节也新）。
> **发布 gen2 对**。xtask 不进 SDK（项目构建工具,`scripts/`,从源码编驱动构建）。

```
compile (linux, 一次)
  S0  cargo z42vm(release+debug)                          (当前格式 VM)
  S1  下载 SDK set → .z42/                                 (打破 chicken-egg;旧格式 z42c+vm+stdlib)
  S2  SDK z42c → 编 xtask → artifacts/xtask/               (构建驱动;xtask 不进 SDK)
  S3  gen1 = SDK 编 {stdlib_g1, z42c_g1}                   (stdlib 先编→z42c 用它解析;SDK codegen)
  S4  gen2 = gen1 编 {stdlib_g2, z42c_g2} → artifacts/.z42  ★发布对(当前 codegen+格式)
      条件不动点门：比较 gen1 vs gen2 对
        == → 没改 codegen → gen2 自动是不动点 → 跳过 gen3
        ≠ → 改了 codegen → gen3 = gen2 编 {stdlib,z42c}，断言 {gen2}=={gen3}（逐字节 mod BLID）
      + gen2 编 toolchain(src/toolchain) → 本地 SDK set = {z42c,stdlib,toolchain}(gen2) + z42vm(S0)
  S5  xtask --toolchain artifacts/.z42 regen goldens + 编 test-units → *.zbc（供下游 --no-build）
  → 上传 artifact "current-sdk"（artifacts/.z42 + goldens + units）
      [gate①完整: gen2 编出来无报错]  [gate②稳定: {gen2}=={gen3} → 才上传]
     │
     ├─ test-interp (per OS)   下载 + cargo z42vm + --toolchain artifacts/.z42 test --no-build (interp)
     ├─ test-jit (linux, 4-shard)  同上 + jit + --shard k/4         ┐ gate③正确:
     ├─ (stdlib [Test] / cross-zpkg 行为覆盖)                       ┘ gen2 工具链跑测试套件全绿
     ├─ host-package (per OS) / package-{ios,android,wasm} / test-{platform}
     └─ cross-bootstrap（交叉验证,独立 job = 改造 bootstrap-no-csharp）
          用"打包发布形态的本地 SDK set"当种子重跑 S2-S4：编 xtask + {stdlib,z42c}
          验：{z42c,stdlib} 逐字节==本地 set 自带({gen2}=={gen3}) / xtask 编成功（完整性二次确认）
  publish-nightly  needs ← package-* + cross-bootstrap + test-interp + test-jit (+ stdlib/cross-zpkg)
                  ← 三关全过:完整 + 稳定 + 正确
```

### 4.2 严密性不变量（CI 必须保证）

1. **SDK set 只在 compile job 用**（S1-S3 产 gen1 + 保留供差分）；下游一律本地 SDK set(gen2)。
2. **测试跑的就是将发布的本地 SDK set**（同一 artifact）——测的 == 发的。
3. **发布 nightly 必须过三关**（缺一不可,见 §1 / Decision 8）：
   - **完整**：gen2 编出来无报错 + cross-bootstrap 重建成功。
   - **稳定**：`{gen2}=={gen3}`（S4 不动点 gate 上传；gen1==gen2 时跳 gen3）。
   - **正确**：test-interp / test-jit (+ stdlib/cross-zpkg) 全绿 → **进 publish-nightly needs**。
     （`{gen2}=={gen3}` 只证稳定不证正确,"稳定地错"也能过不动点 → 测试腿必须进 needs。）
4. **xtask 不进 SDK**：S2 由 SDK 编出驱动构建；其发布形态由 cross-bootstrap 用本地 SDK set 重编落 `artifacts/xtask/`。

---

## 5. 边界与已知风险

### 5.1 chicken-egg（已解）
SDK set（下载）打破。compile job 的 S0-S2 是**不可消除的 shell 层**（cargo z42vm + 下载 + SDK 编 xtask）——
此刻 xtask 还不存在，无法"用 xtask 编 xtask"。S3 起全用 xtask 驱动。

### 5.2 语法/能力 bump（已有纪律）
新语法分两阶段：**support 先行**（z42c 加能力但源码不用）→ 发一版 nightly → **晚一个 release 再 use**。
保证上一版 nightly 的 z42c 永远能编当前源。CI 的 bootstrap 本身强制（SDK 编不过当前源就红）。

### 5.3 🔴 zbc/zpkg format bump 死锁（**待修**）
`zbc_reader.rs` **精确匹配 major+minor**（拒读 older + newer minor）。一旦某 commit bump 格式：
新 z42vm 读不了旧 SDK 的 zpkg → bootstrap 全断 → publish-nightly 发不出新格式 nightly → **死锁**，
且无逃生口（工具链没有非-z42 的兜底编译器）。compile-once 把 bootstrap 收成单点后，此风险**更集中**。

**修法（待拍板）**：
- **A. committed seed**：`.z42/` 下载不兼容时回退**仓库内提交的当前格式种子**。最严密、零外部依赖，
  代价 ~3MB 二进制进 git（仅能力 bump 时刷新）。
- **B. staged dual-format reader**：format bump 时 reader 临时读 `[旧,新]` 两格式（support 先行）。
  无 git 二进制，代价是每次 format bump 写临时双格式读取代码。
- **C. 接受死锁 + 手工恢复**：最省事最脆。

### 5.4 不动点未 gate 发布（待修）
现状不动点只在 `bootstrap-no-csharp` job 验，而 publish-nightly 的 needs **不含它**。理论上能发出没验过
自洽性的 nightly。compile-once 把不动点门移进 compile job（S4，条件触发 gen3 见 design.md Decision 6）+
把 bootstrap-no-csharp 改造成 cross-bootstrap（种子换本地 SDK）进 publish needs → 自然 gate 一切（§4.2.3）。

---

## 6. 当前冗余清单（系统改进目标）

| # | 冗余 | 现状 | 目标 |
|---|------|------|------|
| 1 | **16× 重新全量自举** | 测试 job 各自跑 `ci-bootstrap` action（~12min）| 全消费单一 current-sdk artifact |
| 2 | **build-wave 二次重建** | test all 的 `_regenCore` 又编 z42c+stdlib+goldens（~17min）| compile-once + `--no-build` 消除 |
| 3 | test all 内 z42c 编 3 次 | ✅ 已修（89db08a0：cross-zpkg/compiler stage 加 noBuild）| — |
| 4 | 两个 bootstrap 脚本 | ✅ 均已消除：`ci-bootstrap.sh` → `.github/actions/ci-bootstrap` composite action（10 处统一 `uses:`）；`selfhost-bootstrap.sh` 已删（其重建+gen1==gen2 不动点逻辑本在 xtask，verify-selfhost 改用 `ci-bootstrap action + xtask test compiler`）。scripts/ 仅剩 install-z42.sh | — |
| 5 | `bootstrap-no-csharp` job | 唯一做不动点，但 needs 不 gate 发布 | 不动点门折进 compile job S4；此 job **改造**成 cross-bootstrap（种子换本地 SDK），进 publish needs |
| 6 | `compiler-stdlib` job | ✅ 已删（simplify-xtask-verify，2026-07-07）：replace-csharp S2.1 dogfood 遗物，C# 移除后 "z42c 编 stdlib" 即 `build stdlib` 本身（build sdk / gate regen 波 / 每腿 ci-bootstrap 都做），隔离再编 + emit 探针对 `build stdlib` + `test stdlib` 无独立覆盖 | — |
| 7 | scripts/ 多个 shell CLI | 5 个 .sh | 逻辑内联 CI；仅留 `install-z42.sh` |

### 迭代（每步独立 commit + CI 验证；详见 spec tasks.md）
- **P1** xtask `--toolchain <dir>` + `build sdk`（成对分代 gen1/gen2 + 条件不动点；地基，不依赖任何裁决）
- **P2** compile job（内联 S0-S5 + 条件不动点门 + goldens/units 上传 current-sdk）；format 兜底**第一版不做**（Decision 2）
- **P3** 下游消费 artifact（test-interp / test-jit / host-package / package-* / platform）
- **P4** cross-bootstrap：改造 bootstrap-no-csharp（种子换本地 SDK set，重跑 S2-S4）+ **三关进 publish needs**（package-* + cross-bootstrap + test-interp + test-jit 等）
- **P5** 重命名（build-and-test→test-interp、vm-jit+stdlib-jit→test-jit）+ ✅ 删 compiler-stdlib（已落地）+ 删脚本（仅留 install-z42.sh）

**预期**：关键路径 ~52min → ~25-30min；`{z42c,stdlib}` 编 16 次 → 1 对（+条件 gen3）；matched-set 边界显式 +
发布门三关（完整/稳定/正确）严密。

---

> 维护：本文件随 P1–P5 落地逐步更新；现状段（§2 现状、§6）改完一项即勾掉。
