# Design: indexed zpkg 最小 patch 分发

## Architecture

```
z42c build <toml>（pack 决议 = indexed）
  ├─ 编译/增量与 packed 完全同路（cache SoT，add-file-level-incremental）
  ├─ dist 投影（indexed）：
  │    ├─ 失效文件：cache/<rel>.zbc → dist/<rel>.zbc **字节原样复制**
  │    ├─ 未失效文件：dist/<rel>.zbc **不触碰**（字节+mtime 不动 → patch 最小）
  │    └─ 主文件 dist/<name>.zpkg：META/STRS/NSPC/EXPT/DEPS/SIGS/TSIG/IMPL（同 packed）
  │        + FILE 段（rel/源 hash/ns/zbc 内容 hash）——每次构建重写（User 确认可变）
  └─ VM 装载：flags=indexed → FILE 目录 → 逐 zbc（自包含 fullMode）复用 standalone
      zbc 解码 → ns/entry/DEPS/SIGS 取主文件；zbc hash 校验不符 → 报错
```

## Decisions

### D1: 主文件段面 = packed 减 MODS 加 FILE；聚合 TSIG 留主文件
跨包消费方（DepScan：NSPC/SIGS/TSIG/IMPL）对 indexed lib **零改动**——`ZpkgReader.Open`
只需放行 indexed flags（不读 MODS 即可）。散装 zbc 自带 EXPT/IMPT（fullMode 既有），但
包级 TSIG（类型签名池）与 DEPS 仍以主文件为 SoT（与 packed 单源同构）。主文件每次重写
（TSIG 全包耦合，本就随任一文件变化而变——与 User「主 zpkg 允许变」裁定一致）。

### D2: 散装 zbc = cache 条目的原样投影（字节级复用）
add-file-level-incremental 已证：cache fullMode zbc 自包含（文件局部池），源不变 → 字节不变
（对账器 47/47）。indexed dist 的 `.zbc` 直接 `File.Copy`（增量：仅失效文件），**不经
IrModule 重序列化**——packed 下不可行的字节切片（全局池耦合，前 change D7）在 indexed 下
正是天然形态。未失效文件 dist 不触碰 → 「未变 zbc 逐字节不动 + mtime 不动」的硬保证。

### D3: pack 决议接通（首次消费）
三层优先级（`[profile.*].pack` > `[[exe]].pack` > `[project].pack`）+ 内置默认
（debug→false=indexed / release→true=packed）——project.md 既有规范原样生效。
现有 tomls 几乎全显式 `pack=true` → 存量行为不变；无 [profile] 覆盖的裸 debug build
将开始产 indexed（VM 同 change 落装载，端到端自洽）。

### D4: indexed × strip 不兼容 → 诊断错误
indexed 是开发态（DBUG 内嵌散装 zbc，按文件可符号化）；strip/zsym 是发布态（packed）。
`pack=false ∧ strip=true`（显式或 CLI override）→ `z42c build` 报错退出（设计完整性：
不做静默忽略）。indexed 不产 `.zsym`。

### D5: zpkg minor bump（zpkg-only 路径）
FILE 段布局与 indexed 语义相对 C# 时代文档重定义 → `ZpkgWriterZ.Minor++` 与
`ZPKG_VERSION_MINOR` 同 commit 同步（version-bumping 步骤 6-9：writer/reader 常量 +
changelog 注释 + zpkg.md 表 + fixture regen）。packed 布局字节**零变化**（段序/编码不动）
——bump 只因格式语义面扩展；`indexed-minimal` fixture 用 z42c 新布局重生（解除 minor=22
搁浅）。bootstrap：自举链全 packed；bump 触发已文档化的 nightly 自愈周期（publish-nightly
仅依赖源码构建腿），不与其他格式变更同期落地。

### D6: VM 装载复用 standalone zbc 路径
z42vm 已能直跑单 `.zbc`（fullMode 解码存在）。indexed 装载 = 解析主文件（复用 packed 段
解析，跳过 MODS）→ 按 FILE 目录逐 zbc 读文件（相对主文件所在目录，rel 结构镜像）→ 每
zbc 走既有 fullMode 解码 → 模块注册用主文件 ns/DEPS/entry。**zbc 内容 hash 校验**：FILE
段存每 zbc 的内容 hash，装载时校验，不符 → 明确报错（防散装文件与主文件版本错配——
增量分发场景的一致性守门）。strict-pin：主文件 + 散装 zbc 的版本常量均精确匹配。

### D7: 增量投影与对账器扩展
indexed dist 写出挂在既有增量结论上：失效集 → 复制对应 cache zbc + 重写主文件；全命中 →
沿用 preserved 跳过（主文件也不动）。`--no-incremental` → 全部 zbc 重写 + 主文件重写。
对账器新增 indexed 腿：逐文件 touch 后断言 ① 增量 dist 与 `--no-incremental` 全量 dist
**逐文件字节相等**；② 未 touch 文件的 `.zbc` **未被重写**（mtime 采样断言）——把「patch
最小」也变成被测量的事实。

### D8: 兼容与迁移
零兼容层（pre-1.0）：旧 minor 的 zpkg（含旧 indexed fixture 语义）随 strict-pin 自然失效，
`xtask build test` regen。`z42b clean` 语义覆盖 indexed dist（dist 整目录可删重建）。

## Implementation Notes

- FILE 段编码（提案，实施期按字节对账定稿）：`u32 count` + 每条
  `u32 rel_idx（STRS）+ u32 src_hash_idx + u32 ns_idx + u32 zbc_hash_idx`；zbc 内容 hash
  = SHA-256 hex（与源 hash 同池复用编码设施）。
- dist 布局：`dist/<name>.zpkg` + `dist/<rel 去 .z42>.zbc`（子目录镜像；`_writeCacheZbc`
  同款 rel 规则）。exe 依赖复制（`_bundleExeDeps`）不变。
- 主文件 BLID：沿用 packed 的 BLAKE3-128 尾（对 indexed 主文件同样成立）。
- DepScan 对 indexed lib：`Open` 放行后 NSPC/SIGS/TSIG 直读；MODS 相关面（ReadModuleSigs
  的 MODS 头游标）对 indexed 走 FILE 段等价读取或直接跳过（DepScan 只用 SIGS 平铺 +
  firstSig 序——实施期核对其对 MODS 的真实依赖面后定）。
- 增量 dist 的删除文件清理：源清单变化 → 全量路径重写主文件 + 清理孤儿 `.zbc`
  （按 FILE 目录 diff 删除）。

## Testing Strategy

- 单元：indexed 主文件写→读往返（FILE 段 + 段面）；pack 决议三层优先级；strip 冲突诊断。
- e2e golden：indexed exe 直跑（z42vm 装载主文件 + 散装 zbc → 输出正确）；hash 错配报错。
- 对账器 indexed 腿（D7）：字节 + mtime 双断言。
- 格式：`cargo test lazy_loader`/`zbc_compat` + fixture regen + `xtask test` 全 stage +
  自举不动点（全 packed 路径零扰动回归）。
