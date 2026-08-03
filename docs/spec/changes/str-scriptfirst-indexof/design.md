# Design: Script-First 字符串搜索（char[] view + 脚本 IndexOf）

## Architecture

```
   VM corelib    __str_to_chars(this:str) -> char[]   （唯一 view 原语，bulk：一次物化整串 scalar）
        ▲ 委托
   String.ToCharArray()  [Native("__str_to_chars")]   （替换旧 per-char CharAt 循环）
        ▲ 用
   String.IndexOf(v)  { h=this.ToCharArray(); n=v.ToCharArray(); scan h[i+j] vs n[j] (ArrayGet) }
        ▲ 用（IndexOf>=0）
   String.Contains    自动获益
```

## Decisions

### D1: 一个 char[]-view 原语，算法留脚本（Script-First 终极目标）
不为每个字符串操作加 native extern。只加 **`__str_to_chars`**（bulk 物化 scalar char[]），
IndexOf/（后续 Replace/Split…）在**脚本**里 over `arr[i]`（ArrayGet **opcode**，非 CharAt **builtin**）。
`ToCharArray()` 本就该是这个原语——顺手把它从 per-char 版改成 bulk native，全体调用方获益。

### D2: 为什么快（实测 8.6×，非布局）
慢因不是「字符串在 native」而是**每字符走 builtin 派发**（`CharAt`：per-site token 查表 + 参数
marshaling + fn 指针调用）。`arr[i]` 是内联 opcode。bulk 物化一次（1 native call）+ N 次 ArrayGet ≪
N 次 CharAt builtin。剖析佐证：`Vec<Value>` native scan 2.06µs/轮，但走 interp ArrayGet 118µs/轮
→ 解释器**派发**占 57×；CharAt builtin 再叠 ~9×。char[] 脚本吃掉后者。

### D3: scalar 语义（UTF-8 双索引，默认 scalar，像 Rust）
`__str_to_chars` 产 **Unicode scalar** 序列（Rust `s.chars()`）；`IndexOf` 返回 **scalar 索引**，与
`CharAt`/`Length` 自洽（`"héllo".IndexOf("llo")==2` 而非 byte 3；`"日本語テスト".IndexOf("テスト")==3`）。
String 表示不变（UTF-8 `Arc<str>`）；byte 视图（`ByteLength`）保留供 byte-语义快路（本 change 不涉及）。

### D4: bootstrap —— 新 builtin 不需两-nightly
新增 corelib builtin = 改 Rust VM。GREEN/冷启动的 z42vm 都 `cargo build` 自当前 runtime 源 → 必含
`__str_to_chars`。`xtask test bootstrap`（nightly z42vm + nightly stdlib 编当前 z42c 源）：nightly
stdlib 的 `ToCharArray` 仍是旧脚本 → 不触发新 builtin → 通过。不 bump zbc 格式（无新指令）。

### D5: 已否决/延后
- **native per-op str builtin**（`__str_index_of`）：2ms/1000× 最快，但每操作一个 extern，违 Script-First
  终极目标 → **否决**（曾实现后删）。留作升级阶梯「最后手段」，仅当某操作 char[] 版仍不达标再单议。
- **数组 packed 布局（C#-like）**：native scan 实测 Vec<char> 76ms vs Vec<Value> 103ms = **仅 1.35×**；
  耦合面 `Value::Array` 123 处 match。大头是解释器派发（57×）非布局 → **暂不做**（数周级重写换 1.35× 不值）。
- **String 改 UTF-16/32**：否决（2-4× 内存 + 代理对复杂度 + 全量迁移；UTF-8 scalar 语义更干净）。

## Testing Strategy
- Rust 单测（`cargo test`，[[xtask-test-excludes-cargo-test]]：xtask test 不含 Rust 单测，须单跑）：
  `__str_to_chars` 产 scalar（`"héllo"`→5 scalar）+ 空串。
- stdlib 行为回归：IndexOf/Contains ASCII + UTF-8（scalar 索引），与旧逐字对齐。
- 完整 GREEN：`cargo build`+`cargo test` + `xtask test`（e2e string goldens / stdlib [Test] / compiler
  自举 / cross-zpkg）+ 自举不动点。
- 性能：`07_string_heavy` + 真实 `String.IndexOf` before/after（实测 2128ms→248ms，8.6×）。
