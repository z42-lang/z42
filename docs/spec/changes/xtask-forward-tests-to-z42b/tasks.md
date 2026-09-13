# Tasks: xtask 测试路径转发 z42b

> 状态：🟢 A/B/C/C2/D 全部落地（剩 D3 耗时对账） | 创建：2026-09-12
> 规划：[proposal.md](proposal.md)（含 5.6× 实测与「直接替换会丢掉什么」清单）

## 步骤 A —— z42b 的选择与并行

- [x] A1 `--filter <substr>`：子串筛一批。与 `--name` 分开语义 ——
      `--name` 零命中**报错并列出可选目标**（点名点不中是错），`--filter` 零命中**提示并返 0**（筛不到是正常）
- [x] A2 **不做「z42b 内部并行」** —— 并行留在 xtask 侧（裁决与实测见 proposal ②/③）。
      z42b 定位不到自己（`__env_args` 只给 `--` 之后的参数），自呼需新 VM builtin；而 xtask
      本来就知道 vm + builder zpkg 路径。改为：**xtask 并行拉起 `z42b test <toml> --name <unit>`**。
      实测 z42.io 51 单元、同为 8 路并行：xtask 31.07s → 11.25s（**2.76×**）。
- [x] A3 `z42b test/bench --list`：只列目标名（一行一个），不编不跑 —— 转发架构的接缝。
      xtask 靠它拿单元清单，**发现规则从此只有 z42b 一份**（两套规则各自漂移正是 #580 查出
      那三处分歧的来源）。零目标 → 空输出 + rc=0（「没有目标」是正常答案）。

## 步骤 B —— bench 聚合

- [x] B1 **不需要多目标合并** —— 转发是「每单元一次 `--name` 调用」，每次恰好一个 TestReport，
      正是 `MicroBenchAgg.addModuleJson` 期待的形状。此前把 bench 挡在转发外的理由（「多目标
      合并未做」）建立在「每库一次 z42b」的设想上，与实际落地的形状不符，已订正
- [x] B2 转发路径接上 capture 模式（`--format json` + `addModuleJson` + `setProfileOnce`），
      schema-v2 格式不变。**基线对账**：遗留 65 条 / 转发 65 条，**逐个名字完全一致**

      🔴 **途中发现并修掉一个静默 bug**：`--format json` 下 z42b 的 stdout **混着构建进度**
      （`compiled: …`），消费方按「整段以 `{` 开头」判定落空 ⇒ **整个模块的 bench_stats 被静默
      丢弃**，基线捕获到 **0 条却不报错**（只打印 `wrote baseline (0 benchmarks)`）。
      旧路径没暴露是因为它直接跑已编好的产物，根本没有构建进度可打。
      修法：`--format json` ⇒ 进度改走 stderr（`Z42.Build.BuildLog`）。

      ⚠️ 该开关初版写成静态字段 `public static bool ToStderr = false`，**不生效** ——
      静态初始化器在该包被**延迟加载**时才跑（defer-class-initialization），发生在 z42b 赋值
      **之后**，把它重置回 false。改用环境变量通道（输出格式本就是进程级属性）。

## 步骤 C —— 发现规则对齐 + 孤儿源守卫（**必须先于 D**）

- [x] C0 **发现规则与 xtask 对账**（转发的前置：z42b 发现什么，必须与 xtask 今天发现的一致）。
      逐 lib 比对 xtask 规则与 z42b glob 的单元集合，查出两个真 bug + 一处默认值猜错：

      · **`SourceDiscovery._expand` 的目录段被当字面量** —— `tests/*/source.z42` 去找一个真名叫
        `*` 的目录 → **静默返回空数组**。静默空是最坏形态：调用方看到的「没有测试」与「真没有
        测试」不可区分。改为逐段展开（目录段吃 `*` / `?`）。
      · **段级 glob 只会产出「一文件一单元」** —— 目录单元（多文件共享 namespace）只编入口文件，
        兄弟文件的符号全部未定义；且约定入口文件全叫 `source.z42`，多个目录单元会全部撞名
        `source`、只活下来一个。改为按「段根深度」判定：更深一层 ⇒ 目录单元，名取目录名、
        源取整目录。
      · **默认 include 里的 `<dir>/*/source.z42` 是个猜错的值** —— 本仓 `<lib>/tests/` 下同时住着
        反射 `[Test]` 用例和 VM golden 用例（`main()` + `expected_output.txt`），**两者都长成
        `<name>/source.z42`，glob 分不开**（xtask 靠扫目录里有没有 `[Test]` 标注来分，那不是 glob
        能表达的）。全仓实测：修好 `_expand` 后这条会扫中 **20 个 golden 目录** + 1 个真 `[Test]`
        目录。故删掉它、目录单元一律显式配 —— 对齐「必须主动配置，不要完全不写就自动发现」。
        **删它零行为变化**：它今天匹配数就是 0（正被 `_expand` 的 bug 吞着）。

      对账结论（修复后）：全仓真正的 `[Test]` 目录单元只有 `z42.crypto/tests/secp256k1` 一个
      （已显式配好）；`src/compiler/*/tests/<name>/` 那 23 个各自带**独立 toml**、是独立工程，
      不走段级 glob。~~此前一度以为 z42.collections 少发现 14 个单元~~ —— 那 14 个是 golden 目录，
      xtask 不认领它们是对的。

- [x] C1 孤儿源守卫**落 z42b**（裁决见 proposal Open Questions）。这条是为一次真实静默事故
      （`z42.ir`/`z42c.core`/`z42c.syntax` 静默数月）加的，不能在转发中丢。

      · 守卫插在「零目标 → 回落到编译本包自己再跑」**之前** —— 不插的话「有源却零单元」会报出
        绿色（fixture `src/tests/z42b/orphan-source/` 的 `src/` 里特意放了一个通过的 [Test]，
        证明下游 #571 的零测试判红**接不住**这种形态）。
      · 两条豁免都不是硬编码名单：子目录自带 `*.z42.toml` ⇒ 独立工程（沿用编译器测试单元的既有
        发现规则）；`auto = false` ⇒ 本包显式声明无约定单元，但**照样每轮打印 ⚠**。
      · **顺带对齐第四处分歧**：z42b 此前完全忽略 `section.Auto`，`auto = false` 在它那儿是哑配置。
      · **xtask 侧 `_isKnownOrphanLib` 退休** —— 欠债从工具里的硬编码名单搬到债主自己的清单
        （`z42.scripting` 的 `[tests] auto = false`），两条发现路径从此共用同一条豁免规则。

## 步骤 C2 —— z42b 支持 harness=false（转发的最后一块拼图）

- [x] C2 `harness=false`（自带 Main、**退出码即判定**）在 z42b 落地。
      **必须 fork 子进程**：这类目标的 Main 通常自己调 `Environment.Exit(code)` 表达结论 ——
      in-process 跑会把 z42b 自己一并带走、后面的目标全不跑。
      z42vm 的定位走三级探测（`Z42_PORTABLE_VM` → `Z42_HOME/bin/z42vm` → 开发树 artifacts），
      与 `_findCompilerZpkg` 同构；找不到就明确报错，不假装跑过。

      **顺带修掉「纯测试工程」建不起来**：整个包只有 `[[test]]` 目标、没有自己的源
      （`src/tests/manifest-targets/basic` 正是这形态）时，无条件建父包会撞
      「no .z42 sources under <projdir>」⇒ 目标根本跑不起来。改为**父包无源则跳过**
      （没有父包也就没有 internal 要暴露），目标独立编译。

      **阴性对照**：另造一个 `Environment.Exit(3)` 的目标，z42b 如实传出 rc=3 并判红 ——
      不是假装跑过。gate 冒烟 `_smokeHarnessFalse` 守住这条分叉真的走了子进程。

## 步骤 D —— 切换（高风险，动 GREEN gate 最关键的 stage）

- [x] D0 **「本包压根没有测试」不得判红**（转发的前置阻塞，实测发现）。
      `z42b test <无测试的库>` 此前 rc=1：零目标 → 回落到「编译本包自己再跑」→ 跑出零用例 →
      Runner 的零测试判红（#571）。而 xtask 今天对这类库是**静默跳过**的 —— 不修则转发后
      `z42.build` / `z42.scripting` 会集体把 stage 拖红。

      分流依据是**本包源里有没有 `[Test]`/`[Benchmark]`**：有 ⇒ 「整个包就是测试工程」
      （compile-then-test 形态），照旧回落去编+跑；没有 ⇒ 「无事可做」，打印一行后退 0。
      判据与 xtask 的 `_dirHasTestMethods` 同款（朴素子串扫）—— 假阳性只是退回今天的行为，
      假阴性需要 attribute 在源码里根本不以字面出现，真测试做不到。

      **为什么不违反 #571**：#571 管的是「**测试产物**跑出零用例」。这里一个测试目标都没
      声明、包里也没有测试，没有测试产物存在。「有源却零单元」那条真正危险的静默由 C1 的
      孤儿源守卫判红，不靠这条兜。

      **未给 `z42.test` 加新 API**：它不在六个自依赖库之列，加 API 要受「晚一个 nightly 再用」
      约束（bootstrap-seed 轴 ③）。判定逻辑落在 z42b 内部，零 stdlib API 变更。

- [x] **D1 转发落地** —— `_runLibKind` 在「test 路径 + 该包无显式 `[[test]]`」时改走
      `z42b test <真 manifest> --list` 拿单元清单 → 并行拉起 `z42b test <toml> --name <unit>`。
      今天 stdlib 的 23 个库全部满足该条件（零显式目标、零 `harness=false`）；旧路径保留，
      兜住将来出现的显式目标（尤其 `harness=false`，其 Main 自己调 `Environment.Exit`）。

      **对账（同机、同并行度、背靠背）**：覆盖面**逐个用例相同**（3240 通过 / 332 单元 /
      2 跳过），墙钟 195.17s → 193.09s，差 2s 在噪声内。详见 proposal ②（含三次错误性能
      结论的复盘）。

      **随之消失**：旧路径「编译退 0 但产物缺失」的重试绕行（那个 Heisenbug 出自合成清单的
      独立编译相位，转发路径没有这个相位）。

      **推进途中照出五个先于本程序存在的缺陷**（每一个都只在「按包编译」这条路上暴露，
      旧路径的裸 `--emit-zbc` 单文件编译恰好绕开）：#580 静默空 glob、#584 zpkg 丢 TIDX 字符串、
      #586 E0606 误报父包、#595 本包跨-ns 自有类被当依赖、#598/#600 阻塞调用不让出 GC safepoint。

- [x] D1a `z42.net/tests/http_keepalive.z42` 补 `using Std.IO;` —— `Stream` 是 `Std.IO` 的类型，
      此前漏写；旧的单文件 `--emit-zbc` 路径没判出来，改由 z42b 按包编译后被 E0436 逮到。
      这是那个文件**自身**的缺陷，与转发无关。

- [x] D1b ~~`_runLibKind` 改为转发 `z42b test <lib manifest>`~~ —— **形状订正后与 D1 是同一件事**。
      落地的不是「每库一次 `z42b test <toml>`」，而是「`--list` 拿清单 → 每单元一次 `--name`」
      （并行留在 xtask 侧，裁决见 A2）。D2 把最后那个「有显式目标就回落」的条件也删掉后，
      `_runLibKind` 就只剩这一条路。

- [x] **D2 旧执行引擎退休** —— test/bench 全线转发，xtask 自有的执行器删净（−438 行）。

      删掉的：`_autoUnits` / `_dropOverridden` / `_explicitByHarness`（约定发现 + 显式覆盖）、
      `_runReflectTarget`（显式 harness=true：编 lib zpkg 再让 z42b 反射）、`_runExitTargets`
      （harness=false 批量包装）、`_runUnitsBatched` + `_compilePrep` + `_discoverTestUnits` +
      `TestUnit`（合成 mini-manifest 的批量编译/执行）。**合成清单不再落盘。**

      留下的（都还有真消费方，不是遗迹）：`_runExitTarget` + `_compileTarget` —— **example
      stage 在用**（examples 不是测试，判据是「编得过 + 可选跑得通」，不走 z42b）；
      `_renderSyntheticManifest` —— embedded golden 在用；`_dirHasTestMethods` —— golden
      语料在用；`_validateRunTargets` —— 清单 lint（z42b 对 `HasEntry=false` 是「自动探测
      入口」而非错误，这条它不复制）。

      **关键动作是把 fixture stage 也转过去**：`src/tests/manifest-targets/` 是显式目标 +
      `harness=false` 唯一的真实用例地，不转它就没有任何东西能证明转发路覆盖得住这三种形态，
      引擎也删不掉。转完这个 stage 从「测 xtask 的引擎」变成「测 z42b 对这些形态的处理」——
      与同 stage 那批 `_smoke*` 同向。

      **对账（同机背靠背，只换 `scripts/`，xtask 各自重建）**：
      `xtask test targets` 遗留 vs 转发 —— **11 条 PASS 逐个名字相同**、3 个目标、rc 均 0。

      **阴性对照**（三种形态各破一处，一次跑）：auto 约定单元 `auto_conv` →
      `FAIL ... 0 passed, 1 failed`；显式 harness=true `unit_ok` → `FAIL`；
      harness=false `exit_ok` → `✗ exit_ok (harness=false, exit 3)`。stage rc=1、4 处失败。
      —— 三条路都真的在跑，不是「碰巧绿」。
      另核：`targets <name>` 精确选名照旧；点不中仍 rc=2 且列出
      `available: auto_conv, exit_ok, unit_ok`；`bench targets` 绿。

      **顺带确认没有静默跳过**：新路以「`<lib>/<lib>.z42.toml` 存在」当「这是个真工程」的判据
      （`src/libraries` 的枚举里混着 README.md / z42.workspace.toml）。全仓核过：23 个 lib 目录
      **无一缺清单**，故这条判据不会吞掉任何库。

- [ ] D3 前后耗时对账（当前 `stdlib [Test]` 1m14s / 占 gate 38%）
      —— stdlib 那条 D1 已对过（195.17s → 193.09s，同机同并行度背靠背）；D2 没改那条路的形状。
      ⚠ 本轮**不出耗时结论**：机器上同时跑着别的会话的构建（load ~14），按本程序的铁律
      （同机、同并行度、背靠背、且确认空闲）不满足，数字不能拿来做取舍。

## 备注

**A1 实测**：
```
z42b test <collections> --filter stack  → 只跑 stack，5 passed
z42b test <collections> --filter zzz    → "no test target matched --filter `zzz`"，rc=0
z42b test <collections> --name  zzz     → "no test target named `zzz`" + available: linkedlist, list_api, …
```
GREEN：`xtask test` 全 13 stage 绿。
