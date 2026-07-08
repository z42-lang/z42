# Design: 文件级增量编译（cache SoT → dist 投影）

## Architecture

```
z42c build <toml>（单工程；workspace/flat 不走）
  ├─ CacheStore.Load(cacheDir)                       ← cache 条目集（zbc+meta；版本 pin 不符→整体失效）
  ├─ IncrementalBuild.Probe(srcs, texts, cacheEntries)
  │    ├─ 失效种子 = {源 hash != meta.hash} ∪ {无 meta/zbc} ∪ {srcs 集与 meta 集不一致}
  │    ├─ 传递闭包：沿「引用定义文件」反向边扩散（B 失效 → 依赖 B 的 A 失效）
  │    └─ 输出 freshSet / cachedSet
  ├─ 全包 parse + 符号收集 + TSIG/allFuncs/classMap 重算（轻量前端，恒全包——D2 修正）
  ├─ cachedSet 重建：ZbcReader.Read(cache zbc) + meta(usedDepNs) → IrModule；Exported 全包重算
  ├─ freshSet 编译：仅失效子集跑 typecheck+codegen（_compileCu）
  ├─ freshSet 落 cache：zbc + meta（含本次「引用定义文件集」）
  ├─ 全部未失效 ∧ dist zpkg 在盘 → 跳过重写（沿用现状）
  └─ 否则 BuildPackedD(cache 条目全集，按源序) → dist/<name>.zpkg（任一变更即整包重写）
```

## Decisions

### D1: 判定与组装的 SoT 从「上次 zpkg」移到 cache
**问题**：port-incremental-build-cache 用上次 zpkg 的 MODS 做判定，cache 只做存在性标记。
**决定**（User 架构裁定 2026-07-07）：cache 条目（zbc + meta）成为唯一判定与组装来源；
dist 是 cache 的纯函数。收益：① 单源化消除 C# 混合重建失败的「两来源合并」根因；
② cache 形态（fullMode zbc per file）与后继 indexed 散装 zbc 同构，indexed change 只需
把 cache 条目投影进 dist。meta 携 writer 版本 pin（zbc/zpkg minor），版本不符整体失效
——等价 wire 格式 strict-pin 在 cache 层的镜像。

### D2: 全包 parse + TSIG 重算，仅失效子集 typecheck/codegen（1.1 调查后重写，User 确认 2026-07-08）
**问题**：freshSet 编译时需要 cachedSet 的符号（同包跨文件互引）。原案 = 从 cache meta
读序列化 TSIG 注入（cachedExports）。
**调查事实（IrDump.z42:59-126）**：每文件 TSIG 天然**全包耦合**——①每文件 Exported 嵌入
全包所有文件的自由函数（`allFuncs` 兄弟泄漏，C# 字节实证形态）；②符号收集/`pkgClassNs`/
`pkgClassMap` 均需全包 AST；③`ExtractP(cus[i], symbols, allFuncs, …)` 从 AST + 全包上下文
提取。→ B 加自由函数则 A 的 TSIG 必变（哪怕 A 源不动），cache 里 A 的旧 TSIG **无法**
byte-identical 复原。
**决定（修正案）**：不做 TSIG 注入。增量路径 = 照常 **parse 全包（轻量前端）→ 全包符号
收集 → allFuncs/classNs/classMap/TSIG 全包重算**；逐文件仅对 freshSet 跑 `_compileCu`
（typecheck+codegen，贵的部分），cachedSet 的 IrModule 用 ZbcReader 从 cache zbc 读回。
**meta 相应瘦身：不存 TSIG**（每次由当前 AST 重算——源即终极 SoT，单源性更强）；保留
源 hash / usedDepNs（typecheck 产物，源不变即不变）/ 引用定义文件集 / 版本 pin。
proposal What #1（meta 字段）、#3（重建面 = IrModule 而非 Exported）已同步收窄；
spec 全部 scenario 不变。

### D3: 失效边取「被引用文件任何变化」的保守粒度
**问题**：理想失效 = 仅当 B 的**导出签名**变化才失效 A；但需要 TSIG 结构化 diff。
**决定**：第一版用保守边——A 引用 B 定义的符号（按 TSIG 符号→定义文件归属建边），
B 源 hash 变化即失效 A。正确性优先；「TSIG-equal 则不失效」的细化留 Deferred
`incremental-future-tsig-level-invalidation`（roadmap 索引）。格式前提已核实：z42 泛型
为代码共享 + 具化（generics.md），跨文件调用按 FQ 名编码，B 的代码体不会单态化拷贝进
A 的 zbc——故「签名不变则 A 产物不变」在格式层成立，暴力对账器负责实证。

### D4: 暴力对账器是验收的主门
**问题**：文件级增量的正确性风险面（依赖漏边、池耦合）靠人工场景列举覆盖不住。
**决定**：`xtask test incremental`——对 z42c 7 包 + 代表 stdlib 包 + launcher，逐文件
append 注释 touch → 增量 build → 与 `--no-incremental` 全量 build 逐字节比对 → 还原。
任一文件不等即红。CI 併入 compiler scope 腿或独立腿（实施期定，倾向并入 test compiler）。

### D5: ZbcReader 本 change 起有真实消费方
port-incremental-build-cache 裁决不移植 ZbcReader（当时无消费方）。文件级重建使
cached 文件的 IrModule 必须从 cache zbc 读回 → ZbcReader（fullMode 全段 → IrModule）
成为必需。从 git 历史 C# `ZbcReader.cs`(+`.Instructions.cs`) 对照移植，与 ZbcFormat
常量同源；往返单测（Write→Read→Write 字节相等）为其独立验收。

**D5a：wire 信息差 → writer 残留存 meta（2026-07-08 实施期发现）**
zbc wire 不携带三类 writer 侧信息，而它们**参与 STRS/全局池字节**：
① **块 label 串**（`swx_arm_3` 等语义命名；InternPoolStrings 逐块 intern，Br/BrCond
wire 只存块索引）——C# reader 合成 `entry`/`block_N` 占位（其 roundtrip 从未以
byte-identity 为验收）；② **模块 StringPool 原序**（含 TIDX 专用串与 ConstStr 串的
交错插入序——重建 STRS 需按原序 re-intern）；③ TIDX StrIdx 的模块池映射。
**决定**：三者作为「writer 残留」存 cache meta（`pool` 行 = 模块池 1..n 原序、
`labels` 行 = 每函数块 label 表；串一律 UTF-8 hex 编码免转义）。ZbcReader 本体保持
纯 wire 解码（z42c.ir 无 meta 依赖，占位 label + C#-style 池重建）；driver 集成层
用 meta 回填 label/池/TIDX idx。cache 条目语义 = **zbc（wire）+ meta（probe 字段 +
writer 残留）= IrModule 的无损存储**——这正是 D1「cache 为 SoT」的完整形态；纯
wire 版 reader 的占位路径同时服务未来 indexed 装载（VM 不关心 label 命名）。
**REGT 类型回填**：wire 指令仅 dst/cond/val 带 tag，纯操作数寄存器只有 id → 解码后
把 REGT[id] 类型回填给纯操作数 TypedReg（首访者即 REGT 首写者，重放序一致 →
BuildRegt 复现同字节；dst/cond/val 类型取自指令 tag 字节，不用 REGT 覆盖）。
**负整型字面量**：ConstI 存 Text，`_parseIntLit` 不识负号 → 解码负值以 `0x` hex
重表示（parse 按位截断等价）；f64 以 `__double_from_bits` 还原 + 最短可逆十进制文本
（Rust Display 保证 round-trip），roundtrip 单测兜底。

### D6: meta 文件格式 = cache 内部约定，非 wire 格式
不 bump zbc/zpkg。meta 用简单二进制/文本序列化（实施期定，倾向复用 ByteWriter 风格），
头部记 `(zbcMinor, zpkgMinor, metaVersion)`；任何不符 → 该条目（或整 cache）作废重建。
cache 可随时整目录删除（`z42b clean` 语义不变）。

### D7: dist 组装路径 = IrModule 重序列化，不做字节切片（2026-07-08 系统化）
**问题**：「从 cache 组装 dist」有两条候选路径——① cache zbc → ZbcReader → IrModule →
既有 zpkg 写出管线；② 直接把 cache zbc 的 func/type/… 段**字节切片**拼进 MODS（零反序列化）。
**事实（ZpkgWriter.z42:126-158 实证）**：packed zpkg 全段共享**单一全局 STRS 池**——
`_buildSectionList` 把 ns/exports/deps/TSIG/逐模块（ns/src/hash + InternPoolStrings）全部
intern 进一个 pool，MODS 体经 per-module remap 引用**全局索引**；而 cache zbc（fullMode）
的段索引指向**文件局部池**。两个索引空间不同，且 B 文件的字符串会改变全局池布局 →
A 的 MODS 体索引必须重映射。**字节切片在 packed 下物理不可行**。
**决定**：路径 ①——cached 文件经 `ZbcReader.Read` 读回 IrModule，与 fresh 文件的 IrModule
**同构地**进入既有 `ZpkgWriterZ.WritePackedWithSidecar`（全局池 intern + 段构建 + BLID），
组装管线零分叉（cached/fresh 在组装层不可区分）。byte-identity 由「ZbcReader 往返无损」
（Write→Read→Write 字节相等，独立单测）+「组装纯函数性」（同一 IrModule 集 → 同一 zpkg
字节，既有确定性门禁）两支柱保证。字节切片留给 indexed（change B）：其散装 zbc 自包含
局部池，天然可整文件复用——这正是 User「未变文件 zbc 不动」需求与格式的契合点。

**组装输入溯源表**（cached / fresh 文件各字段从哪来）：

| ZbcFileZ / 包级输入 | cached 文件 | fresh 文件 |
|---|---|---|
| IrModule | ZbcReader.Read(cache zbc) | 本次 codegen（同时写回 cache）|
| SourceFile / SourceHash | 源发现 + 当前源计算（≡ meta.hash，probe 已校验）| 同左 |
| Namespace / Usings | 当前 AST（全包 parse，D2）| 同左 |
| Exports | 读回 IrModule.Functions 名 | codegen IrModule |
| UsedDepNs | meta（typecheck 产物，源不变即不变）| 本次 typecheck |
| TSIG（Exported）| 当前 AST 全包重算（D2）| 同左 |
| DEPS / entry / META | BuildDependencyMap + manifest（恒当前）| 同左 |

**失败回退**：任一 cached 条目读回失败（zbc 损坏 / 段不符）→ 该文件降级 fresh 重编，
不 abort；连带其依赖方按闭包失效。

### D8: 性能模型——比全量重编快在哪、上界在哪（回应 User 2026-07-08）
逐阶段对比（单工程 build，有 ≥1 文件失效时）：

| 阶段 | 全量 | 增量 | 差异 |
|------|------|------|------|
| 源读 + hash | 恒做 | 恒做（判定本身需要）| — |
| lex + parse（全包）| 恒做 | 恒做（D2：TSIG/符号全包耦合）| — |
| 符号收集 + TSIG 重算 | 恒做 | 恒做 | — |
| **typecheck + codegen** | 每文件 | **仅失效闭包** | **主要收益**（编译主耗时段）|
| cache zbc 读回 | — | cached 文件 | 新增，但线性字节反序列化 ≪ typecheck+codegen |
| zpkg 全局池 intern + 段序列化 | 恒做 | 恒做（D7）| — |
| DepScan（外部依赖扫描）| 恒做 | 恒做 | — |

结论：**是，快于直接从源码全量重编**——省下的是未失效文件的 typecheck+codegen（编译
管线中最贵的相位），代价是廉价的二进制读回；但受 Amdahl 约束，加速上界 = 恒定成本
（parse/TSIG/组装/DepScan）占比的倒数，**不是** N 文件改 1 个就快 N 倍。最快路径仍是
全命中跳过（零成本，已上线）。若实测 parse/组装恒定成本占比高，后续杠杆已有去处：
TSIG-level 失效（Deferred `incremental-future-tsig-level-invalidation`）、indexed 免
MODS 重序列化（change B）。**暴力对账器同时输出计时**（增量 vs 全量墙钟，逐包报告），
把「更快」变成被测量的事实而非假设——若某语料上增量反而更慢，即为设计回退信号。

**D8 实测与三轮修正（2026-07-08 收口）**：首轮对账器 xtask 语料（42 文件，同 namespace
自由函数密集互引 = 近全连接闭包，touch 1 失效 40/42 的 worst case）实测**增量比全量慢
17%**——回退信号触发。逐项归因并修正：
① token 保守边对全部文件**独立重 lex**（ParseAll 之外第二遍）→ **token 集入 meta v2**
（cached 行零 lex）；② WriteMetas 给 fresh 行收集 token 仍重 lex → **Parser.Tokens()
一行访问器**（Scope 追认）+ `ParseAllTk` parse 时捕获（全链路零第二遍 lex；闭包/降级转
fresh 的行从留存 meta 取）；③ 源 SHA-256 **重复计算 3 遍**（probe/ZbcFileZ/meta；解释
执行的 SHA-256 每遍 ~3-4s）→ 一次计算三处复用。终态（同一 worst-case 语料）：
**增量 77.5s vs 全量 76.3s（+1.5% ≈ 持平）**；全量路径顺带 84→76s（哈希去重）；
no-touch 跳过 8.8s（**~9×**）。稀疏语料的文件级收益、密集语料的持平保底、跳过路径的
数量级优势三态齐备；剩余 +1.5%（meta 解析 + 闭包）与边精度细化同归 Deferred。

## Implementation Notes

- srcs 集一致性：meta 集合与当前 srcs 集不一致（增/删文件）→ 全量失效（沿用记录数防洞）。
- 「引用定义文件集」采集：codegen/typecheck 期记录每文件解析到的**包内**符号的定义文件
  （符号→文件归属表由本次/上次 TSIG 提供）；prelude/外部包引用不入边。
- 失效闭包在文件数 N 上 O(N·E)，N 小（包内文件），朴素实现即可。
- `Z42_INCR_DEBUG=1` 扩展：打印失效种子与传播链（`A invalidated-by B`）。
- 迁移：旧 cache（无 meta）→ 全 miss 一次，下次起走新形态；`ReadSourceHashes` 保留一个
  release 作对照断言后退役（或直接退役，实施期定）。

## Testing Strategy

- 单元：CacheStore meta 往返；依赖闭包（链/扇出/无边）；版本 pin 失效；ZbcReader 往返。
- 暴力对账器（D4）：逐文件 touch 增量 == 全量，语料级；**同时输出增量 vs 全量计时报告**（D8）。
- 场景 e2e：改叶子文件只重编 1 个；改被依赖文件重编其闭包；增/删文件全量；no-touch 跳过。
- GREEN gate：`xtask test` 全绿 + 自举不动点 7/7（workspace 路径不受影响）。
