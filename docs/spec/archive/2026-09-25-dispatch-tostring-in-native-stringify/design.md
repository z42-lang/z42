# Design: native 字符串化路径派发 `ToString`

## D1：修在 VM 还是编译期？

上一刀（`fix-struct-tostring-paths`）选的是**编译期**（在插值/拼接处改发 struct 的 `ToString` 调用）。
本刀选 **VM**，理由是三条路会**汇聚到同一个已经正确的 helper**：

| | VM（选中） | 编译期再补一轮 |
|---|---|---|
| 覆盖面 | 静态类型未知也对（`object` 变量、跨包、数组元素、反射取回的值） | 只覆盖编译期能看出是对象的位置 |
| 落点数 | 3 处（`io.rs` 一族 / `exec_value::add` / `jit_add`） | 每个字符串化 sink 各补一次（`String.Format`、`Join`、Assert 消息…） |
| 新机制 | **零** —— `obj_to_string` 早就在 `exec_function` 重入 | 需再加一轮 lowering，且字节漂移 |
| 上一刀漏掉的单字段 struct | 自然覆盖（判据按**运行期值形态**，不按 `_isBlobStruct`） | 得再判一次那个闸门 |

⭐ **关键事实校正**：上一刀在 golden 抬头把 ④ 判成「涉及 GC 根与可重入性 ⇒ 另开」。
`ToStr` 指令（路 ②）走的 `obj_to_string` **本来就在重入 VM**，`corelib` 里 repl / threading /
`reflection/invoke.rs` 也各自在重入。⇒ **重入不是本刀引入的风险**，
那句判断把「没接线」误记成了「有障碍」。

## D2：装箱接收者的判据从哪来

**不自己写第二份**，直接复用 `resolve_vcall(ctx, module, val, "ToString", 0, None)`：
它已经把三种盒的正确答案定好了 —— enum 盒 → 成员名；基元盒 → 标量（且 `this` 给的是**拆箱后的
标量**）；struct 盒 → 自身槽位的 `ToString`，没有才短类型名。

🔴 **为什么不能用 `resolve_by_candidates`**：它最后会回落 `Std.Object.ToString`，
而那个 builtin 收到装箱 struct 直接抛 `__obj_to_str: expected an object`
（`vcall_resolve.rs:224-227` 的注释记着前人第一版就这么踩的）。

## D3：没有自声明 `ToString` 的类型打什么

**短类型名**（`BareC`），不是 `BareC{...}`。判据 = 与路 ①② **早已钉下**的行为对齐
（既有 golden 断言 `Assert.Equal("Bare", $"{b}")`），本刀的全部意义就是让四条路给同一个答案。
代价：REPL 三个 fixture 的期望要更新（`Std.Collections.List{...}` → `List`）——
REPL 回显走 `_fmt(object v) { return "" + v; }`，注释写着「MVP：ToString via concat」，
改后它才真的是 ToString。

## D4：快路不动

- `Str + Str`：仍走 `alloc_str_concat2` 融合分配，零改动
- 整数 `int_binop`：零改动
- 混合臂只在操作数是 `Object` / `BoxedStruct` 时才派发；`I64`/`F64`/`Bool`/`Char`/`Null`/
  `Array`/`Str` 一律仍走裸 `value_to_str`（**逐字节不变**，含数组的 `[1, 2]` 递归格式）

## D5：interp / JIT 两侧对称

`jit_add` 的两条混合臂加同款 `jit_stringify`。只补一侧 ⇒ 同一段代码解释执行对、JIT 后不对
（`jit_field_get` 的 StackArray 臂、`jit_vcall` 的 GetType 臂都栽过这一条）。
验证判据 = 同一用例 `--mode interp` 与 `--mode jit` 输出**逐字节 diff 为空**。

## D6：已知语义后果

- **自指 `ToString` 变成无限递归**：`override ToString() => "C" + this` 今天因为不派发而「碰巧」
  终止，改后与 C# 一样栈溢出。这是正确语义的代价，写进 reference。
- **异常仍被吞成 `<exception: …>` 字符串**（沿用 `obj_to_string` 既有约定）。
  改成传播会让 `Console.WriteLine` 成为可抛点 ⇒ 独立取舍，登记 Deferred。
- 展示路径多一次 VM 重入：只在操作数是对象时发生，热路径（数字/字符串拼接）不受影响。

## D7：格式与缓存

无指令/元数据/编码变更 ⇒ zbc / zpkg 格式不变、无 fingerprint bump、编译器零改动、零字节漂移。
