# tasks: enforce-catch-type

> 类型：**fix**（补上一条从未发射的诊断）｜ 创建：2026-09-27
> 出身：结构审计 2026-09「32 个零发射诊断码」。User 口径：**先修正确性的 bug**。

## Why

`catch (T e)` 的 `T` 此前**完全不校验**：

```z42
public class NotAnException { public int X; }

void Main() {
    Console.WriteLine("start");
    try { throw new Exception("boom"); }
    catch (NotAnException e) { Console.WriteLine("caught"); }   // 编译零诊断
    Console.WriteLine("after");
}
```

```console
$ z42c build project.z42.toml      # ✓ wrote -> ./dist/catp.zpkg
$ z42vm dist/catp.zpkg
start
Error: uncaught exception: Std.Exception: boom
```

那个 catch **静默永不匹配** ⇒ 异常穿出去。用户写错 catch 类型拿到的是
**「不报错 + 错误处理静默失效」** —— 而 `error-codes.md` 早就如实标着
「⚠️ 零发射点 —— **catch 类型当前不校验**」。

## What Changes

`StmtBinder._chkCatchType`：catch 类型解析成 `Z42ClassType` 后沿基链上溯找 `Exception`，
找不到就报 E0420。基链上溯去掉泛型尖括号（同 `ResolveSealedTarget`）。

### 🔴 宽松闸门：解析不到 `Exception` 时一概不报

```z42
if (!symbols.HasClass("Exception")) { return; }
```

**不链 stdlib 的编译路径真实存在且合法** —— `SemanticDump.FirstErrorCode` 走的独立 Infer
路径就是（`exhaust_tests.z42` / `missing_return_tests.z42` 头注都写了）。那里 `Exception`
根本不存在，没这条闸门**所有** catch 类型都会被误报（`missing_return_tests` 里的
`catch (Ex1 e)` 会全红）。链上某环解析不到时同样返回、不报。

⚠️ **这条的保守方向与 E0403（`add-missing-return-check`）正好相反**：
那条「拿不准就不报」会漏掉真问题，所以必须穷尽 16 种语句；这条「拿不准就报」会误伤
合法编译路径，所以必须留闸门。**同一个词「保守」，对两个检查指向相反的方向** ——
判断保守方向的问法是「**拿不准时，报还是不报更不容易造成假信号**」。

## 指纹 39 → 41

**bump 的理由是诊断变**：写错 catch 类型的源文件此前编得过、零诊断，现在报 E0420，
哈希一字未变 ⇒ 不 bump 会命中旧条目、把新诊断吞掉。发码零变化 ⇒ CI 的 fingerprint
守门对这一档是瞎的，手动 bump。

⚠️ **让号**：main 上是 39，在飞的 **#897（E0403）取 40** ⇒ 本刀 **41**。
合并前按当时的 main 现查复核。

## Scope

- `src/compiler/z42c.semantics/src/StmtBinder.z42`
- `src/compiler/z42c.semantics/tests/typecheck/catch_type_tests.z42`（新，7 条）
- `src/compiler/z42c.pipeline/src/CacheStore.z42`（指纹 39 → 41）
- `docs/reference/src/appendix/error-codes.md`（E0420 状态：⚠️ 零发射 → ✅）

## Tasks

- [x] `_chkCatchType` + 宽松闸门，接进 `_bindTryCatch`
- [x] 7 条测试（2 阳性 + 5 阴性，含**宽松闸门的钉子**）
- [x] 真工程端到端：阳性报 E0420；四种合法形态（基类 / 用户子类 / 多 catch / 裸 `catch`）
      零误报且运行输出正确（`1 2 4 5`）
- [x] **全仓误报面清查：零 E0420** —— 编译器 / stdlib 25 包 / xtask 79 文件 /
      workload + toolchain / 382 golden / 347 stdlib 测试文件
- [x] 自举不动点 3/3 gen1==gen2 + `z42c [Test]` 24 unit 全过 + e2e 740 passed 0 failed
- [x] `CompilerFingerprint` 39 → 41
- [ ] GREEN：CI 全矩阵绿

## 不做（Out of Scope）

- **不校验 `throw` 的操作数类型**（`throw new NotAnException()`）。那是另一条判据、另一个码，
  且 `throw` 的值可能是任意表达式（不止 `new`），需要独立设计。本刀只管 catch。
- **不管 `catch (T)` 里 `T` 是接口 / 泛型型参**的情形：接口不进 `Classes`（`HasClass` 恒假）
  ⇒ 走不到本检查；型参被 Unknown 吸收，同理。要覆盖得先决定语义，另开。
- **不给 E0424 接线**（非法强制转换）：`error-codes.md` 已如实说明它实际走 E0402 / E0439 或
  运行期 `Std.InvalidCastException` —— **有别的码覆盖，不是真缺口**。
