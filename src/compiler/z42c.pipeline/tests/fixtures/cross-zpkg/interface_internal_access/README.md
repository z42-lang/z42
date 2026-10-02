# interface_internal_access — 跨包 internal 接口引用强制（自动门）

> change：`complete-class-access-control`（④ 接口类型可见性）。

## 自动门

cross-zpkg runner 的 `expected_build_error.txt` 约定：`main` 必须**编译失败**且 stderr 含该文件内容；
编过了判红，错误文本对不上也判红。本 fixture 据此作为自动门，纳入 `xtask test e2e --dir cross-zpkg`。
（不放 `expected_output.txt`：期望构建失败，无 stdout 可比对。）

## 期望行为

- `target/`（`demo.ifaceinttarget`）：`Handler`（无修饰符 → 默认 internal）、`HandlerExplicit`
  （显式 internal）、`Api`（public）。
- `main/`（`demo.ifaceintapp`，依赖 target）：
  - `Use(Api a)` → ✅ 放行（跨包 public 接口）。
  - `UseInternal(Handler h)` / `UseInternalExplicit(HandlerExplicit e)` → ❌ `E0404 AccessViolation`
    `cannot access internal interface \`Handler\` from another package`。

## 期望的失败文本

`expected_build_error.txt`：``E0404: cannot access internal 接口 `Handler` from another package``
（同一次构建里 `SecretExplicit` / `HandlerExplicit` 那条也会报；门只需命中其一即可锁住行为）。
