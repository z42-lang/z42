# interface_internal_access — 跨包 internal 接口引用强制（手工验证 fixture）

> change：`complete-class-access-control`（④ 接口类型可见性）。

## 为什么是「手工验证」而非自动 case

cross-zpkg 自动 runner（`scripts/test/xtask_test_cross.z42`）只支持**成功运行 + stdout 比对**
（`expected_output.txt`）——**无 expected-compile-error 模式**。本 fixture 期望 `main` 构建时报
`E0404`（跨包引用 internal 接口），属**构建失败**语义。故本目录**故意不含 `expected_output.txt`**，
runner 会跳过，不影响 GREEN。与类级 `class_internal_access`（#184）同款处置。

## 期望行为

- `target/`（`demo.ifaceinttarget`）：`Handler`（无修饰符 → 默认 internal）、`HandlerExplicit`
  （显式 internal）、`Api`（public）。
- `main/`（`demo.ifaceintapp`，依赖 target）：
  - `Use(Api a)` → ✅ 放行（跨包 public 接口）。
  - `UseInternal(Handler h)` / `UseInternalExplicit(HandlerExplicit e)` → ❌ `E0404 AccessViolation`
    `cannot access internal interface \`Handler\` from another package`。

## 手工验证步骤（需 0.38 全栈：CI 两代自举，或某个 0.38 warm 本地）

```bash
# 1) 建 target lib（产出 demo.ifaceinttarget.zpkg）
Z42_LIBS="$PWD/.z42/libs" .z42/bin/z42c build src/tests/cross-zpkg/interface_internal_access/target/z42.toml --release
# 2) 把 target.zpkg 放进 main 的 libs 后建 main —— 期望非零退出 + 打印两条 E0404（Handler / HandlerExplicit）
Z42_LIBS="<stdlib + target dist>" .z42/bin/z42c build src/tests/cross-zpkg/interface_internal_access/main/z42.toml --release
```
