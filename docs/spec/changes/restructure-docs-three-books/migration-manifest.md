# 搬迁清单（逐文件）

> 覆盖 `docs/book/src/`(74) + `docs/design/`(99) + `docs/workflow/`(25) = **198 篇**。
> 这是批 1–6 的执行依据，与 `tasks.md` 双向对齐。
> `learn` **不接收任何搬迁文件**（它是从零写的，OUTLINE 已定 34 章 + 3 附录），三个目录只作素材源。

## 关键事实（先读这三条）

1. **`docs/design/` 里 72 篇在 book 中从未有过对应页 —— 约 73%**。所以本次不是「迁移重复内容」，主体是**把从未落地的内容首次编入新书**。
2. **C# 编译器已不存在**（`find src -name '*.cs'` = 0）。`design/compiler/compiler-architecture.md`(1431 行) 通篇是 C# 命名空间。
3. **`.zmod` / `.zbin` 在 src 里零引用**，`design/compiler/compilation.md` 描述的机制已不存在。

---

## 1. reference

### 1.1 `reference/language/` ← book（直迁 23 篇）

`available-macro` `const` `constructors` `enums` `generic-constraints` `generic-methods`
`generics`(1521行，推断算法节切走 internals) `member-accessors`→`properties-indexers`(主干)
`member-forwarding` `methodof` `named-arguments` `partial-types` `pattern-matching`
`readonly-fields` `record-attribute` `sealed` `static-classes` `static-constructors`
`static-members` `target-typed-new` `tuples` `README`(重写)
＋ `book/compiler/type-conversion.md` → `language/conversions.md`（转换表用户要查）

### 1.2 `reference/language/` ← design（净新增）

| 源 | 目标 | 备注 |
|---|---|---|
| `access-control.md` | `access-control.md` | **主干**=全量规则；book 版的强制点进 internals |
| `arrays.md` `collection-literals.md` `delegates-events.md` `exceptions.md` `nested-types.md` `object-initializers.md` `parameter-modifiers.md` | 同名 | 直迁 |
| `attributes.md` | `attributes.md` | registry 实现切走 internals |
| `boxing.md` | `boxing.md` | 仅语义节；插入点实现切走 |
| `closure.md` | `closures.md` | 堆擦除实现切走 |
| `compound-assign.md` | 合并进 `operators.md` | 仅脱糖表 |
| `foreach.md` | 合并进 `iteration.md` | foreach 是 iteration 子集 |
| `iteration.md` | `iteration.md` | **主干**（鸭子协议 + IEnumerable 两条路径） |
| `interop.md` §11 | `interop.md` | §1–§10 三层 ABI 切走 internals |
| `namespace-using.md` | `namespaces.md` | |
| `object-protocol.md` | `object-protocol.md` | 仅契约；派发实现切走 |
| `properties.md` | 合并进 `properties-indexers.md` | 只补 interface/extern 两个位置 |
| `raw-string-literal.md` | 合并进 `strings.md` | |
| `naming-conventions.md` | `conventions/naming.md` | |
| **`language-overview.md`**(832行) | 拆成 `syntax` `types` `strings` `operators` `control-flow` `functions` `classes` `structs` `interfaces` `unions` | **待裁决：拆 vs 保留单页** |

### 1.3 `reference/stdlib/`

← book：`app-properties` `runtime-config` `README`(重写为包索引)；`json-serde.md` 只取「公开 API」节合并进 `json.md`
← design：`json`(主干) `cli` `compression` `crypto` `diagnostics` `encoding` `io-binary` `io-stream` `net` `numerics` `random` `regex` `toml` `uri` `yaml`
← 其它：`language/string-builtins.md`→`string.md`；`language/reflection.md`→`reflection.md`；`runtime/gc-handle.md`(anchor 实现切走)；`runtime/stdlib-platform.md`(实现节切走)
⚠️ `design/stdlib/time.md`：`z42.time` 包**已删**，类型在 `z42.core/src/Time/` → 改写页头后迁入

### 1.4 `reference/cli/` + `reference/manifest/`

| 源 | 目标 |
|---|---|
| `book/toolchain/cli.md` | `cli/z42.md`（**主干**：12 命令、退出码、定位规则） |
| `book/compiler/tools.md` | `cli/z42c-z42b.md`（`--dump-*` 调试开关切走 internals） |
| `book/runtime/runtime-settings.md` | `cli/runtime-settings.md`（**只取旋钮登记表**） |
| `design/compiler/project.md`(1586行) | `manifest/z42-toml.md`（**主干**=L1–L4 字段全表） |
| `design/toolchain/export.md` | 合并进 `cli/z42.md` 的 export 节（生成器实现切走） |
| `book/toolchain/README.md` | `cli/README.md`（重写） |

### 1.5 `reference/errors/` `reference/testing/`

- `design/compiler/error-codes.md` → `errors/codes.md`（**主干**=E/W/WS 全量码表）
- `book/compiler/error-codes.md` → 只取分段规则 + Diagnostic 结构，合并进 `errors/README.md`；「新增错误码」节切走 internals
- `design/testing/testing.md` 用户面 → `testing/test-attributes.md`（`[Test]` / `Assert` / `z42 test --filter`）

---

## 2. internals

### 2.1 `internals/compiler/`

← book：`architecture` `source-compile` `project-model` `ctor-inheritance` `README`(重写)
＋ `access-control.md`（只留强制点/两相位/字节不动点）＋ `error-codes.md`（只留「新增错误码」节）
← design：`self-hosting`(自举唯一权威) `binder-hierarchy`(需核实是否仍成立) `scripting-charter`
＋ `language/customization.md` ＋ `language/syntax-config.md`（合并，三层配置是同一机制）＋ `language/metaprogramming.md`

### 2.2 `internals/formats/`（**User 裁决：独立成部分**）

`book/compiler/zbc-format.md`→`zbc.md` ｜ `book/compiler/zpkg-format.md`→`zpkg.md` ｜ `design/runtime/ir.md`→`ir.md`

### 2.3 `internals/runtime/` ← book（21 篇几乎全直迁）

`availability-folding` `diagnostics`(主干) `escape-analysis-stack-alloc`→`escape-analysis`
`gc-incremental-major` `gc-tlab-chunk-exclusive`→`gc-tlab` `gc-tuning-and-safepoint`→`gc-safepoint`(主干)
`heap-diagnostics` `interp-jit-semantics` `jit-lazy-compile`→`jit`(主干) `load-context`(主干)
`missing-symbol-resolution`→`missing-symbol` `native-extensions`(主干) `native-libraries`
`optimization-pipeline`(主干) `reflection-type-identity` `static-ctor-init` `struct-value-semantics`
`superinstr-fusion` `sync-primitives` `README`(重写)
＋ `runtime-settings.md`（只留五层优先级归并 + 可用性检查实现）

### 2.4 `internals/runtime/` ← design

直迁：`vm-architecture`(1212行，**对齐点 05-20，需核实剩多少独有内容**) `execution-model` `pal` `embedding`
`cross-platform` `object-abi` `aot`(前瞻) `tiered-execution`(前瞻) `componentized-runtime`(前瞻)
`hot-reload`(需核实) `concurrency`(前瞻+线程地基)

合并（design 只贡献增补，**book 是主干**）：

| design 源 | 并入 | 贡献什么 |
|---|---|---|
| `gc.md`(1090行) | `gc-safepoint` + `gc-tlab` | 接口形状 / Phase 划分 / 权衡（主体是 Phase 1 RcMagrGC 历史） |
| `safepoint.md` | `gc-safepoint` | 「泛化成统一安全点」设计（自称未实施） |
| `diagnostics.md` | `diagnostics` | 事件分类 L2 / `fire()` 门控（未实施） |
| `load-context.md` | `load-context` | §5 保留根诊断 / §6 native 资源（未落地） |
| `native-ext-loader.md` | `native-extensions` | 「新增 ext lib」配方 + 平台差异表 |
| `ir-specialization.md` | `optimization-pipeline` | intrinsic 特化前瞻 |
| `jit.md` | `jit` | **仅留 Cranelift 后端图**——其「模块加载时预热式 JIT」已被 lazy per-function 取代 |

切片并入：`language/interop.md` §1–§10 → `native-abi.md` ｜ `language/object-protocol.md` 实现节 → `object-protocol-dispatch.md` ｜ `language/boxing.md` 实现节 → `boxing-impl.md` ｜ `language/closure.md` 档C节 → `closures-impl.md` ｜ `runtime/stdlib-platform.md` 实现节 ｜ `runtime/gc-handle.md` 实现节

### 2.5 `internals/stdlib/`

`design/stdlib/overview.md`→`architecture.md`(三层架构主干) ｜ `organization.md` ｜ `api-guidelines.md`
＋ `book/stdlib/json-serde.md` 实现节 → `json-serde.md`（反射底座 / 分派轴）

### 2.6 `internals/toolchain/`

| 源 | 目标 | 备注 |
|---|---|---|
| `book/compiler/project-build.md` | `z42b.md` | 主干（现行八相位） |
| `book/toolchain/deployment-model.md` | `deployment-model.md` | 「轴 × 选项」矩阵抽进 `reference/cli/z42.md` 的 publish 节 |
| `book/toolchain/editor-integration.md` | `editor-integration.md` | 安装步骤给 learn 第 4 章作素材 |
| `design/toolchain/repl.md`(675行) | `repl.md` | **主干**——少数 design 比 book 全的情况 |
| `book/toolchain/repl-input-completeness.md` | 合并进 `repl.md` | 只贡献 parser 完整性判定 |
| `design/runtime/launcher.md` | `launcher.md` | 主干=布局 / apphost / 三包发布 |
| `design/toolchain/launcher-command-dispatch.md` | 合并进 `launcher.md` | 前瞻分发器 |
| `design/toolchain/platform-export-lifecycle.md` | `platform-export.md` | 前瞻 |
| `design/toolchain/export.md` 实现节 | `export.md` | |
| `internals/src/toolchain/workload-distribution.md` | `workload-distribution.md` ＋ manifest/布局表 → `reference/cli/sdk-layout.md` | |

### 2.7 `internals/testing/`

`design/testing/`：`testing.md`→`framework.md`(架构 / TIDX 格式 / runner 协议) ｜ `cross-platform-testing` ｜ `embedded-app-run` ｜ `exec-profile-matrix`

### 2.8 `internals/devinfra/` ← book/dev + workflow（31 篇）

book：`xtask` `build` `test-gate` `benchmarking` `packaging` `README`(重写)
＋ `book/toolchain/test-pipeline.md`（两层模型属仓库侧）＋ `design/compiler/build-artifacts-layout.md`→`artifacts-layout.md`

workflow 全部 25 篇：`quickstart` `ci` `debugging` `packaging`→`local-sdk-package` `release`
`building/{README,macos,linux,windows,compiler,vm,stdlib,wasm,ios,android}`
`testing/{README,bootstrap,verify-by-change,changed-only,cross-zpkg,platform-tests,stdlib-tests,unit-tests,vm-tests}`
（三个 README 重写为导航；`workflow/README.md` 并入 `devinfra/README.md`）

### 2.9 `internals/` 顶层

`docs/design/philosophy.md` → `internals/philosophy.md`（设计北极星，改 z42 的人读）

---

## 3. 删除（18 篇）

**纯目录索引 / 迁移状态表**（10）：`book/src/{README,SUMMARY}.md`、`book/src/appendix/README.md`、
`design/README.md`、`design/{compiler,language,runtime,stdlib,testing,toolchain}/README.md`

**过时快照**（6）：
- `design/runtime/{zbc,zpkg}.md` —— 页首自注「已迁移 2026-07-19」，只剩历史 changelog
- `design/compiler/compilation.md` —— `.zmod`/`.zbin`/JSON Phase1 机制已不存在
- `design/compiler/compiler-architecture.md`(1431行) —— 写的是已删除的 C# 编译器 ⚠️**待裁决：是否先抢救几节**
- `design/testing/test-runner-bootstrap.md` —— 自注「✅已落地」，Rust runner 已删
- `internals/src/toolchain/z42b.md` —— ⚠️**待裁决**：book 迁移表标 ✅，但其 190 行八相位 / `ICompiler` in-process / hook 注入设计，book 对应页只有 109 行

**移出三书**（2）：`design/stdlib/README-template.md`→`docs/agent/rules/`；`design/stdlib/roadmap.md`→并入 `docs/roadmap.md`

---

## 4. SUMMARY 的 16 个空占位 → 填充源

| 占位 | 填充源 |
|---|---|
| 语法与词法 | `language-overview` §1–§4 ＋ `syntax-config` `raw-string-literal` `string-builtins` `compound-assign` |
| 类型系统 | `language-overview` §2 ＋ `boxing` `static-abstract-interface` |
| **所有权与内存模型** | **无对应文件，需新写**（素材：`struct-value-semantics` + `escape-analysis`） |
| 内置协议 | `object-protocol`(主干) ＋ `customization` `iteration` `properties` |
| 异常与错误处理 | `exceptions` |
| 命名空间与访问控制 | `namespace-using` ＋ `access-control` `naming-conventions` |
| FFI / interop 表面 | `interop` §11(主干) ＋ `reflection` `attributes` `metaprogramming` |
| 执行模型 | `execution-model`(主干) ＋ `vm-architecture` `tiered-execution` `aot` `hot-reload` |
| IR 与 zbc | `runtime/ir.md`(主干；zbc 半边已在 book) |
| GC | `runtime/gc.md` ＋ `gc-handle`（三子页已存在） |
| 嵌入与跨平台 PAL | `pal` `embedding` `cross-platform` `stdlib-platform` |
| native interop ABI | `object-abi`(主干) ＋ `native-ext-loader` ＋ `interop` §1–§10 |
| 三层架构与包边界 | `stdlib/{overview,organization,api-guidelines}` |
| 核心包索引 | `design/stdlib/` 下 15 个包文档 |
| workload 与平台发行 | `platform-export-lifecycle` `export` `runtime-workload-distribution` |
| SDK 与发行包布局 | `launcher.md` §磁盘布局/§三包发布 ＋ `runtime-workload-distribution` ＋ `book/dev/packaging` |

---

## 5. 页数估算

| 书 | 页数 |
|---|---|
| `learn` | ≈ 37（OUTLINE 已定；本次贡献 0，已写 3） |
| `reference` | ≈ 70（language 35 ＋ stdlib 24 ＋ cli/manifest 7 ＋ errors 2 ＋ testing 1 ＋ conventions 1） |
| `internals` | ≈ 97（compiler 12 ＋ formats 3 ＋ runtime 32 ＋ stdlib 4 ＋ toolchain 10 ＋ testing 4 ＋ devinfra 31 ＋ philosophy 1） |

198 篇输入 → 约 167 篇输出（约 30 篇被合并折进他页，`language-overview` 一拆为多抵消一部分）。
