# cross-zpkg/ — 多 zpkg 端到端测试

## 职责

验证多 zpkg 协作场景（target lib + ext lib + main app 三方编译 + VM 运行），
覆盖普通 golden test (`run/`) 无法表达的跨包路径。

L3-Impl2 (`impl Trait for Type` 跨 zpkg 传播) 是首个驱动用例。

## 测试目录约定

```
<test_name>/
├── target/                   # 提供 class / interface 的 lib
│   ├── z42.toml              # name + pack=true + [sources]
│   └── src/*.z42
├── ext/                      # 依赖 target，提供 impl 块
│   ├── z42.toml              # depends on target
│   └── src/*.z42
├── main/                     # 依赖 target + ext 的 exe
│   ├── z42.toml              # entry = "<Namespace>.<Func>" + pack=true
│   └── src/Main.z42
└── expected_output.txt       # main 运行后的预期 stdout
```

**负例 fixture（期望编译失败）**：放 `expected_build_error.txt` **代替** `expected_output.txt`，
内容 = `main` 构建 stderr 必须包含的子串（通常是诊断码 + 一句关键词）。判定：`main` **编过了**
→ 判红（说明该报的诊断没响）；编不过但错误文本对不上 → 也判红（否则「随便哪种编译失败都算过」
= 没有判别力的门）。这类 fixture 不进 run 波（没有产物可跑）。范例：`dup_fqn_crosspkg/`。

**z42.toml 必须**：

- `pack = true` — cross-zpkg 引用基于 packed 模式的 TSIG section（debug 默认 indexed 没有 TSIG）
- `[sources] include = ["src/**/*.z42"]` 或保持默认（默认就是这个 glob）
- main 的 `entry` 用 `<Namespace>.<Func>` 形式（不是文件路径）

## 运行

```bash
z42 xtask.zpkg test cross-zpkg              # interp 模式
z42 xtask.zpkg test cross-zpkg jit          # jit 模式
```

驱动逻辑（`z42 xtask.zpkg test cross-zpkg`）：

1. 构建 target → ext → main（每步把上一步的 zpkg 复制到下一步的 `libs/`）
2. 收集 stdlib + target + ext 的 zpkg 到临时 libs_dir
3. 用 `Z42_LIBS=<temp>` 启动 z42vm 跑 main 的 zpkg
4. 比对 stdout 与 `expected_output.txt`

## 现有测试

| 测试 | 覆盖 | 关键路径 |
|------|------|---------|
| `01_impl_propagation` | L3-Impl2 跨 zpkg `impl IGreet for Robot` | IMPL section 序列化 → Phase 3 merge → IrGen QualifyClassName → VM lazy loader |
| `dup_fqn_crosspkg` | **负例**：两个包同 FQN → E0601 | ImportedSymbolLoader 包名累积 → SymbolTable 判据 → 两个 choke point |
