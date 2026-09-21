# Tasks: 删除 primitive 短名别名，源码与 IR 拼写统一到 C# 关键字

> 状态：🟢 GREEN 全绿，待提交 | 创建：2026-09-21 | 完成：2026-09-22
> 变更类型：lang（收窄语言表面）+ ir（zbc 1.44 / zpkg 0.49 双 bump）
> 文档影响：`docs/reference/src/language/types.md`（基本类型总表重写）、
> `docs/internals/src/formats/{zbc,zpkg}.md`（changelog + 当前版本）、
> `docs/agent/rules/version-bumping.md`（坐标表，顺带修正其自身腐坏的 1/40·0/45）
>
> **不需要两-nightly 分阶段**：上一版 nightly 的 z42c 一直认关键字拼写，源码改写后它照编；
> 「z42c 停止接受短名」与「源码改写」可同 commit 落地（种子编的是源码，源码已不含短名）。

## 进度概览
- [x] 阶段 1: 编译器 canonical 收敛到关键字（PrimModel 等 9 文件）
- [x] 阶段 2: 短名关键字 token 从词法器/语法器退役
- [x] 阶段 3: 全仓源码短名拼写改写（160 行 / 31 文件）
- [x] 阶段 4: runtime 镜像同步（struct_reflect.rs）
- [x] 阶段 5: 格式 bump（zbc 1.44 / zpkg 0.49）+ reader/changelog/pinned 单测
- [x] 阶段 6: 文档同步
- [x] 阶段 7: fixture 重生 + GREEN

## 阶段 1: 编译器 canonical 收敛
- [x] 1.1 `PrimModel.z42`：`Canon` 返回关键字；`Code` 按关键字分桶；`_kw` 表变关键字；
      **删 `Keyword()`**（与 `Canon` 同义）；调用点（MemberResolver / MemberResolver.Prim / BinaryTypeTable）改 `Canon`
- [x] 1.2 `PrimModel.CanWiden`：**删掉死的 `byte`/`short` 分支** —— 历史上 canonical 是 `u8`/`i16`
      故该分支从不命中，改名后会突然生效并静默改变重载决议的数值门。保持语义逐条不变
- [x] 1.3 `SymbolTable._canonPrim` / `TypeNameResolver._canonName` 删除（归一退化成恒等）
- [x] 1.4 `SymbolTable._isPrim` / `BinaryTypeTable._isNumericName`：删短名分支
- [x] 1.5 `Conversion._widensLossless` / `TypeChecker._constIntInRange` / `_repClass`：键改关键字
- [x] 1.6 `EmitContext.PrimTag` / `TypeFactsTc`（30 处）：删短名 arm
- [x] 1.7 `StructLayout._sizeOf` / `_isPrim`：**仅机械改拼写，未补缺失 arm**（见下「顺带发现」）

- [x] 1.8 删 `FunctionEmitter._isAliasPrim` / `ClassDescBuilder._isAliasPrim` 两份守卫
      （存在理由是「次要别名会被 Canon 规范化成 u8/i16，而 SIGS/TYPE 要保源拼写」；
      收敛后 `SurfaceName` 返回的就是源拼写，守卫同结果 ⇒ 纯噪音）

## 阶段 2: 短名 token 退役
- [x] 2.1 `TokenKind.z42`：删 `I8..F64`（旧值 84..93，号段留空不复用）
- [x] 2.2 `Lexer.z42`：删 10 条 `_kw("i8", …)` 注册
- [x] 2.3 `Parser._isTypeKeyword`：删 3 行
- [x] 2.4 `z42.scripting/Classifier._isTypeToken`：类型关键字区间上界 `F64` → `Void`

## 阶段 3: 源码拼写改写
- [x] 3.1 `src/libraries` 31 文件 160 行：类型位 → 关键字；静态调用接收者 → BCL 包装名
      （`u8.Parse` → `Byte.Parse`，走既有确定可用的路径）
- [x] 3.2 跳过 `IrType.z42`（IR 文本 dump 的 LLVM 风格记法，刻意保留）
- [x] 3.3 `ZbcFormat.FromName`：窄整数族改绑关键字（返回类型 tag 逐条等价）

## 阶段 4: runtime
- [x] 4.1 `struct_reflect.rs`：`canon` 退化成剥 `?`；`is_prim`/`size_of`/`tag_from_name` 改关键字
- [x] 4.2 `array.rs` / `reflection/type_object.rs` / `reflection/generics.rs` **不动** ——
      它们的短名分支服务的是 **zbc 指令 tag 词汇表**（`instr_decode` 仍产短名），非 zpkg 类型名

## 阶段 5: 格式 bump
- [x] 5.1 `ZbcFormat.Minor` 43→44；`ZpkgWriter.Minor` 48→49
- [x] 5.2 `versions.rs` 两个常量 + 两段 changelog 注释
- [x] 5.3 `zbc_reader_tests.rs` 的 `zbc/zpkg_version_constants_pinned` 钉值
- [x] 5.4 regen `src/tests/zbc-format/*/source.zbc`（6 个）+ `zpkg-format/*/source.zpkg`（4 个）
- [x] 5.5 `zbc_tests.z42` 内嵌 golden hex 重截（仅 header minor 2b→2c，字节数不变）

## 阶段 6: 文档
- [x] 6.1 `docs/reference/src/language/types.md`：总表去短名列 + 新增「没有 Rust 风格短名」节
      （说明 ③ FFI ABI / ④ IR dump 两层刻意保留，并对标 C# 的 int/int32/System.Int32 分层）
- [x] 6.2 `docs/internals/src/formats/zbc.md` changelog 加 1.44 行
- [x] 6.3 `docs/internals/src/formats/zpkg.md` 当前版本 0.48→0.49、耦合 1.43→1.44
- [x] 6.4 `docs/agent/rules/version-bumping.md` 坐标表（顺带修正其腐坏值）

## 🔴 阶段 6.5: 派发键收敛（本次 bump 的隐藏维度）

**这次不只改容器格式，还改了「派发键」**：`OverloadResolver.TypeKey` 走 `Z42Type.CanonName()`
= `PrimModel.Canon`，所以非-primary 重载的 mangle 从 `Substring$2$i32$i32` 变成 `$int$int`。
同族先例是 zbc 1.38 `stabilize-dispatch-keys`（「wire 布局不变，仅键字符串内容变」）。

**为什么这让自举多需要一代**：`Canon("i32")` 现在**原样返回**（短名不再识别），于是
「对着旧 TSIG 编出来的调用点」会继续沿用旧键。所以：

| 代 | 声明侧键 | 跨包调用侧键 | 自洽？ |
|---|---|---|---|
| gen1 stdlib（对着 gen0 TSIG 编） | `int`（源码推导） | `i32`（旧 TSIG） | ❌ 混代，`z42.toml` 调 `z42.core` 即 `MissingSymbolException` |
| gen2 z42c | 逻辑=新（发射 `int`） | `i32` | 自洽但仍是旧键 |
| **gen3 = 清干净全量重建** | `int` | `int` | ✅ 收敛 |

- [x] 6.5.1 gen3：`rm -rf artifacts/build/{libraries,compiler}` + 各 member `artifacts/` 后，
      用 gen2 driver（entry-dir 自带自洽旧键 libs 保证它跑得起来）全量 `--no-incremental` 重建
- [x] 6.5.2 三方键抽查一致：stdlib 声明 / stdlib 调用 / z42c driver 均 `Substring$2$int$int`
- [ ] 6.5.3 ⚠️ **CI 风险待观察**：`.github/actions/ci-bootstrap` 的 [1.5] 两代自举没有这额外一代。
      纯格式 bump 不受影响（键不变），**改键的 bump 可能在 CI 上复现本地那次 `MissingSymbolException`**。
      若 CI 红在这里，修法 = 给 [1.5] 补一次「清干净 + 对着新键 TSIG 重建」的 pass。
      （增量缓存也会掩盖问题：本地曾出现 `build stdlib` 复用缓存、dist 全是旧键却一路绿。）

## 阶段 7: 验证
- [x] 7.1 两代自举（种子 z42c → gen1 → gen2 新格式）+ gen3 键收敛
- [x] 7.2 `xtask test` 全绿（全 stage 通过，5m37s）
- [x] 7.3 `cargo test` 全量 **1444 passed / 0 failed**（注：须用默认 profile；`--release` 下 `debug_validate_invariants` 是 `#[cfg(debug_assertions)]` 门控、编不过，与本变更无关）
- [x] 7.4 负例实测：`u8 x = 1;` → `E0443: undefined type: u8`；对照组 `byte x = 1;` 通过。
      另在 `prim_model_tests.z42` 加了常驻负例门 `test_short_aliases_are_not_builtin_types`（含关键字对照组）
- [x] 7.5 反射面零变化：runtime 在每个用户可见点**本来就**把短名归一到关键字
      （`array.rs::elem_tag` / `type_object.rs::canonical_type_name`），故 `typeof(byte).Name` 不受影响；
      这些归一表保留不动，它们服务的是 zbc **指令 tag 词汇表**（`instr_decode` 仍产短名）

## 顺带发现（**不在本 PR 修**，另案）
1. `versions.rs` 的 zbc/zpkg changelog 注释块此前漏记 1.43 / 0.48 两行（本 PR 顺手补上）。
2. `docs/agent/rules/version-bumping.md` 的「版本常量坐标表」自己腐坏到 1/40、0/45（落后 4 个
   minor），文件里早有自嘲注释说它没测试兜底。本 PR 顺手修正到 1/44、0/49。
3. 🔴 **`ZpkgReader.Read`（z42.ir:118）版本失配时静默 `return null`**，连一条 warn 都不打。
   VM 侧同款失配有一段极好的报错（「built for a different z42 format version … regen via
   xtask build stdlib」），z42c 侧却什么都不说 —— 后果是依赖包被整个跳过，用户看到的是
   **18 条 `undefined: Span` / `undefined: DiagnosticCodes`**，离真因十万八千里。
   本次实施中实测踩到，耗掉三轮排查才二分定位。属 [[audit-silent-gates-program]] 的同族：
   「失配即静默跳过」是个从不说话的门。建议单独修成 warn（附 path + found/expected minor）。

## 实施中的自伤与纠正（记录以免重犯）
最初用一条批量正则删除「`|| n == "<短名>"`」这类别名分支，正则写成
`\s*\|\|\s*\w+\s*==\s*"<short>"` —— `\w+` 把 `t` 也匹配上了，于是把
**`Conversion._widensLossless` 数值拓宽矩阵里真正的目标类型列表**
（`|| t == "i64" || t == "f64"` …）连同 `TypeChecker._repClass` 的 `|| c == "f64"`、
`StructLayout._sizeOf/_isPrim` 的 `|| c == "u8"` 等一并静默吃掉。
后果：`int` → `double` 隐式拓宽消失，`Math.Pow(2, 3)` 报 E0439。

- **谁抓住的**：`xtask build test` 的 golden `math`（337 ok, 1 failed）。门是有效的。
- **我一度误判**：基于**已被自己破坏**的文件，把 `StructLayout._sizeOf` 缺 arm 当成
  「编译期↔运行期既有分歧」写进了发现清单 —— 纯属自伤。已删除该条。
  教训同 [[verify-conclusion-after-reseeding]]：**读到的现状若在自己动过之后，先 `git diff` 对原文**。
- **纠正**：4 处真逻辑从 `git show HEAD:` 原文整块恢复后再做拼写替换，别名分支保持删除。

## 实施中额外修到的（都在本 PR 内）
- `scripts/install/xtask_install_vscode.z42` 的 `_kwPrimType()` 表 + 重生成的
  `z42.tmLanguage.json` —— 有一道**会说话的门**（`vscode-syntax` stage）当场报
  「category 'type' lists 'u16' which Lexer no longer has」，按它的提示 `xtask deps install vscode` 重生。
- `DeclBinder` 的 E0408 诊断文案 `aliases (e.g. int/i32)` → `(e.g. int/Int32)`。
- 7 处编译器注释里把 `i32` 当 canonical / 当源码拼写的陈述。
- `SemanticDump.HasErrorCode(src, code)` 新增（见下）。

## 测试假绿的一次自查（值得记住）
把 `test_nonpartial_duplicate_unaffected` 的 `F(i32)` 换成 `F(Int32)` 后它**继续绿**，
但原因错了：`SemanticDump` 跑的是**裸代码片段的内存管线**，那里没有 Std 符号表，
`Int32` 根本解析不到 ⇒ 那 1 条错误是 `E0443 undefined type` 而非本意的 `E0408 duplicate`。
是隔壁 partial 用例先红了才顺藤查出来。**这类裸片段测试只能用关键字**，
故三处改用 `int` / `int?`（Canon 剥 `?` 后同签名），并把「别名碰撞」用例删除
（别名已不存在，场景无法构造）。呼应 [[silent-feature-masks-other-bugs]]：绿≠对。
