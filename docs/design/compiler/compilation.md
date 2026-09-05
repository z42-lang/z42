# z42 编译产物规范

本文档定义编译输出粒度、文件格式、zbc 生成策略（Strategy D）和工具命令。

> **本文档的边界**：描述编译产物（`.zbc` / `.zmod` / `.zpkg` / `.zbin`）的形态、生成策略与 VM 加载语义。**不描述** 用户 manifest 字段（归 [`project.md`](project.md)）、`.zbc` 内部 wire format（归 [`../runtime/zbc.md`](../runtime/zbc.md)）、IR 指令集（归 [`../runtime/ir.md`](../runtime/ir.md)）。

---

## 总体设计

```
.z42 源文件
   │
   ▼ z42c 编译（Phase 1：per-file）
   │
   ├─ 粒度 1（文件级）
   │    每个 .z42 → 独立 .zbc（字节码单元，存放于 .cache/）
   │    工程级索引 → .zmod（模块清单）
   │
   └─ 粒度 2（发布包级）
        所有 .zbc 打包 → .zbin（可分发包，exe 或 lib）
```

---

## zbc 生成策略（Strategy D）

**目标**：增量判断以源文件为粒度（简单），输出 zbc 以类为粒度（语义清晰）。

### 两阶段编译

```
阶段 1 — 逐文件编译（当前 Phase 1/2 实现）
  src/Hello.z42         → .cache/src/Hello.zbc
  src/Hello.Partial.z42 → .cache/src/Hello.Partial.zbc
  src/Helper.z42        → .cache/src/Helper.zbc

阶段 2 — 链接合并（Phase 2 完成后实现）
  检测到 partial class Hello（跨两个文件）
  → 合并为 .cache/types/Hello.zbc
  增量 key：任一源文件 hash 变化即重合并
```

### 输出路径约定

| 产物 | 路径 | 说明 |
|------|------|------|
| 增量 zbc（开发态）| `.cache/<relative_source>.zbc` | 镜像源文件目录结构 |
| 发布包（生产态）| `dist/<name>.zbin` | 所有 zbc 打包进单文件 |
| 模块索引 | `dist/<name>.zmod` | 供 VM 按文件加载 |

### 内部类规则

**内部类（Inner Class）随外部类**：内部类的字节码包含在外部类的 `.zbc` 文件中，不单独输出。外部类改动 → 外部类 zbc 重编译，内部类一并更新。

### Partial Class 规则（Phase 2）

- Phase 1/2：每个 `.z42` 文件产出独立 `.zbc`（partial 片段分散）
- Phase 2 完成后：TypeChecker 收集同名 partial 片段 → 链接阶段合并为单一 `.zbc`
- `.zmod` 索引的是合并后的类级 zbc，`sources` 字段记录所有源文件片段

---

## 粒度 1 — 文件级：`.zbc` + `.zmod`

### 1.1 `.zbc`（z42 Bytecode — 单文件字节码）

每个 `.z42` 源文件独立编译为一个 `.zbc`。

**类比**：Python `.pyc`、Java `.class`

**Phase 1（JSON 调试格式）**：

```json
{
  "zbc_version": [0, 1],
  "source_file": "src/greet.z42",
  "source_hash": "mmh3:d1c6cd75a506b0a2a506b0a2a506b0a2",
  "namespace": "Demo.Greet",
  "exports": ["Greet"],
  "imports": [],
  "module": { /* IrModule — 与 .z42ir.json 格式相同 */ }
}
```

**Phase 2（二进制格式）**：

```
[4 bytes]  magic:        0x5A 0x42 0x43 0x00  ("ZBC\0")
[2 bytes]  version_major
[2 bytes]  version_minor
[32 bytes] source_hash   (MurmurHash3 x86_128 of source .z42 file, hex)
[4 bytes]  section_count
[sections...]
  STRINGS  — string pool
  FUNCS    — function bodies (SSA instructions)
  TYPES    — type definitions
  EXPORTS  — public symbol table: [(name, kind, offset)]
  IMPORTS  — external symbols needed: [(namespace, name)]
  META     — optional: source map, debug info
```

**增量编译语义**：

- 加载 `.zbc` 时对比 `source_hash` 与当前 `.z42` 文件的 MurmurHash3 x86_128（非密码学；只做相等性比较，理由见 [book zpkg-format MODS](../../book/src/compiler/zpkg-format.md)）
- 若哈希一致 → 跳过重编译
- 若哈希变化 → 仅重编译该文件，不触碰其他 `.zbc`

---

### 1.2 `.zmod`（z42 Module Manifest — 模块清单 / 索引）

`.zmod` 描述一个**工程**（库或可执行）的所有文件及其依赖，是 `.zbc` 的索引层。

**类比**：Python `__init__.py` + `MANIFEST` 的组合

**格式（始终为 JSON，便于 VCS diff）**：

```json
{
  "zmod_version": [0, 1],
  "name": "Demo.Greet",
  "version": "0.1.0",
  "kind": "lib",
  "files": [
    {
      "source": "src/greet.z42",
      "bytecode": ".cache/src/greet.zbc",
      "source_hash": "mmh3:d1c6cd75a506b0a2a506b0a2a506b0a2",
      "exports": ["Greet"]
    },
    {
      "source": "src/util.z42",
      "bytecode": ".cache/src/util.zbc",
      "source_hash": "mmh3:a044242bf7de91dbb631db9ab631db9a",
      "exports": ["StringUtil"]
    }
  ],
  "dependencies": [
    { "name": "z42.stdlib", "path": "../../stdlib/z42.stdlib.zbin", "version": ">=0.1" }
  ],
  "entry": "Demo.Greet.Main"
}
```

**字段说明**：

| 字段 | 说明 |
|------|------|
| `kind` | `"lib"` = 类库（无入口）；`"exe"` = 可执行（有 `entry`） |
| `files[].bytecode` | 相对于 `.zmod` 文件的路径，指向 `.cache/` 下的 zbc |
| `files[].source_hash` | 编译时记录；运行时可验证 |
| `exports` | 该文件对外公开的符号，供 VM 和链接器快速查找 |
| `dependencies` | 引用的外部 `.zbin`；路径可为本地或 registry URL |

**增量更新流程**：

```
z42c build
  │
  ├─ 读取 .zmod，遍历 files[]
  ├─ 对比每项 source_hash vs 磁盘文件 SHA-256
  │    ├─ 相同 → 跳过
  │    └─ 不同 → 重编译该 .z42 → 更新 .zbc + source_hash
  └─ 更新 .zmod 中变化项的 source_hash
```

---

## 粒度 2 — 发布包级：`.zbin`

`.zbin` 将一个工程的所有 `.zbc` 打包为**单一可分发文件**。

- `kind = "exe"`：含入口函数（`entry` 字段），可直接被 VM 执行
- `kind = "lib"`：无入口，仅导出符号，供其他工程依赖

**类比**：C# `.dll`（exe 和 lib 都是 assembly，靠 metadata 区分）；Java `.jar`

### Phase 1（JSON 调试格式）

```json
{
  "zbin_version": [0, 1],
  "name": "Demo.Greet",
  "version": "0.1.0",
  "kind": "lib",
  "exports": [
    { "symbol": "Demo.Greet.Greet", "kind": "func" }
  ],
  "dependencies": [
    { "name": "z42.stdlib", "version": "0.1.0" }
  ],
  "modules": [
    { /* ZbcFile — greet.z42 的完整内容 */ },
    { /* ZbcFile — util.z42 的完整内容 */ }
  ]
}
```

### Phase 2（二进制归档格式）

```
[4 bytes]  magic:        0x5A 0x42 0x4E 0x00  ("ZBN\0")
[2 bytes]  version_major
[2 bytes]  version_minor
[4 bytes]  flags         bit0=executable, bit1=debug_info
[4 bytes]  section_count
[sections...]
  MANIFEST  — JSON 元数据（name, version, kind, exports, dependencies）
  ZBC[0]    — 第 0 个 .zbc 文件的完整内容
  ZBC[1]    — 第 1 个 .zbc 文件的完整内容
  ...
  EXPORTS   — 全局导出符号表：[(fqn, zbc_index, func_offset)]
  RELO      — 跨文件调用的重定位表
  META      — 可选：聚合 source map
```

---

## 文件扩展名总览

| 扩展名 | 含义 | 粒度 | 格式 |
|--------|------|------|------|
| `.z42` | 源代码 | — | 文本 |
| `.z42ir.json` | 调试用 IR（Phase 1）| 文件 | JSON |
| `.zbc` | 单文件字节码 | 文件 | 二进制（Phase 2）/ JSON（Phase 1）|
| `.zasm` | 字节码文本反汇编 | 文件 | 文本 |
| `.zpkg` | 工程包（indexed 或 packed 模式）| 工程 | JSON（Phase 1）/ 二进制（Phase 2）|

---

## 编译器命令

```bash
# 单文件编译 → .zbc（JSON Phase 1）
z42c src/greet.z42 --emit zbc

# 工程构建（debug，读取 *.z42.toml） → dist/<name>.zpkg (mode=indexed)
z42c build

# 工程构建（release） → dist/<name>.zpkg (mode=packed)
z42c build --release

# 只构建指定 [[exe]] 目标
z42c build --exe hello
```

---

## VM 加载语义

| 输入 | VM 行为 |
|------|---------|
| `.z42ir.json` | 直接加载（Phase 1 调试，向前兼容）|
| `.zbc` | 单文件字节码，自包含运行 |
| `.zpkg` `mode=indexed` | 按 `files[]` 路径依次读取 `.zbc`，合并符号表后执行 |
| `.zpkg` `mode=packed` | 解包 `modules[]` 内联 ZbcFile，合并后执行；`kind` 字段区分 exe/lib |

VM 入口选择优先级：`entry` 字段 > 名为 `Main` / `main` 的函数。

---

## 符号解析规则

1. 同文件内直接引用（已在单模块 IR 里解析）
2. 同工程跨文件引用：通过 `.zpkg` 的 `exports` 表快速查找
3. 跨库引用：通过 `.zpkg` 的 `EXPORTS` section + 重定位

---

## 版本兼容性

- `.zbc` / `.zpkg` magic bytes 固定，版本字段用于前向兼容
- VM 必须拒绝 `version_major` 高于自身支持版本的文件，并给出明确错误
- Phase 1 JSON 格式永远向前兼容（新字段旧 VM 忽略）
