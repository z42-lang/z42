# Tasks: `[Forward]` 成员转发（add-member-forwarding）

> 状态：🟢 **完成（2026-09-13）** | 创建：2026-09-12 | 见 [proposal.md](proposal.md)
>
> **lang 类**变更，走「DRAFT → User 确认 → IMPL → GREEN → COMMIT」。
> ① 组是阻塞项：A/B/C 已做完（结论见 proposal「前置验证」），D 未做。

## ① 前置验证

- [x] 1.A **③ 档 partial 跨碎片配对** —— ✅ **2026-09-12 实测通过**：声明侧
      `public partial string Greet(string m);` 在碎片 1、body 在碎片 2，编译干净、运行正确。
      ⇒ ③ 档成立，per-fragment mangle 预扫描在两侧算出同一 RegKey。
- [x] 1.B **生成源码是否落盘** —— ❌ **不落盘**：`GeneratorDriver` 全文件无 `File.WriteAllText`，
      `_parseGen` 直接在内存 parse 成 CU。**顶在「选 Generator 路线是为了以后能调试」这条理由上**
      → proposal 决策点 1 待 User 裁决。
- [x] 1.C **跨碎片同名重载的真实行为** —— 🔴 **静默覆盖，非硬错误**（实测复现）：用户碎片的
      `Log(string)` 被生成碎片的 `Log(int)` 顶掉，调用点报出**假类型错** `E0402: cannot assign
      string to Int32`。⇒ **S5 必须按名字跳过、不能按签名跳过**。
      ⚠️ 此结论**推翻** 09-06 记忆里的「跨碎片同 RegKey 是硬错误 PartialDuplicateMember」。
- [x] 1.D **①② 档签名保真度** —— ✅ **2026-09-12 查完，结论把 ①② 的边界切在「本包 / 跨包」上**：

      | 形态 | 本包类型（`ms.HasDecl`，有 `Decl`） | 跨包类型（只有 `Z42FuncType`） |
      |---|---|---|
      | 参数类型 | ✅ | ✅ |
      | 参数名 | ✅ `Decl.Params[i].Name` | ❌ **导入时被丢掉**——`ImportedSymbolLoader` 四个读取点(:287/:325/:380/:465) 只取 `Params[j].TypeName`，`ExportedParamZ.Name` 从不落进 `MethodSymbol` |
      | 默认值 | ✅ `Decl.Params[i].Default`（Expr） | ✅ `Z42FuncType.ParamDefaults`（`$Default` ConstBlob，`_fillParamDefaults`:653） |
      | `params` 变长 | ✅ `Decl.Params[i].IsParams` | ✅ `Z42FuncType.ParamsFrom` |
      | `ref` / `out` | ✅ `Decl.Params[i].IsRef` | 🔴 **格式里根本不存在**——`IsRef` 在 z42.ir 全层 + 导入侧**零命中**，TSIG 从不记录 |
      | 参数 attribute | ✅ `Decl.Params[i].Attrs` | ❌ |

      ⇒ **「参数名一定丢」这句（proposal 初稿 / 09-06 记忆）过于悲观**：只有**跨包**才丢，本包全保真。

- [x] 1.E 🔴 **跨包 ①② 遇到 `ref`/`out` 会静默产生错误语义，且判定不了** —— 实测坐实：
      调用点**丢掉 `ref` 照样编译通过、零诊断，修改静默丢失**（`Bump(ref x)`→2，`Bump(y)`→1）。
      而跨包侧连「这个方法带不带 ref」都判不出来（见 1.D 表）⇒ **无法精确拒绝、只能整类拒绝**。
      这条不是转发引入的，是既有缺陷；`add-ref-borrow-system` 的下一步「typecheck ref 真 byref」
      正覆盖它 ⇒ **本提案不另开修复，只受其约束**。

      **由此定 ①② 的适用边界（v1）**：
      - **本包类型**：①②③ 全档支持，签名完全保真。
      - **跨包类型**：只支持 **③ 档**（签名是用户手写的，`ref` 由用户自己写对）。
        ①② 对跨包类型**报错拒绝**，诊断直接说「跨包转发请用 `partial` 声明档」。
      - 判据本身是可判定的（`ms.HasDecl`），不依赖那个判不出来的 ref 信息 ⇒ **边界是硬的、不靠运气**。

## ② Generator 框架（唯一的框架改动）

- [x] 2.1 `GeneratorDriver` 触发点扫描扩展到**字段级** `AttributedDecl`
      （现仅顶层 `Inner is ClassDecl`，:137-140）
- [x] 2.2 🔴 **不得剥掉字段级触发 attr 的合成工厂**（`fld$<Class>$<Field>`，`AttributeSynth:75-76`）——
      剥了白名单的 `methodof` 就不再被编译 ⇒ 编译期检查消失 ⇒ 白名单退回字符串时代。
      **本条要有一条会变红的测试盯着**（白名单写个不存在的方法 → 必须编译错）。
- [x] 2.3 `ForwardAttribute : Attribute` 真类（合成工厂需要真类型，不能只当标记名）

## ③ 转发面收集

- [x] 3.1 ① 档：解析 `[Forward<I>]` 的接口 → 取接口方法全集
- [x] 3.2 ② 档：读 `Attr.Args` 的**原始 AST**（`MethodOfExpr` 节点 → `OwnerName`/`Member`/`ParamTypes`），
      不经运行期 `MethodInfo`
- [x] 3.3 ③ 档：扫本类 `partial` 且无 body 的方法声明，按 `[Forward]` 字段配对
- [x] 3.4 🔴 **生成顺序必须显式 sort**：`Z42ClassType.Methods` 是 hashed StrMap、迭代序不确定；
      确定序的 `OwnMethodNames` **只对 imported 类型和 impl-block 填，同包本地类是空的**。
      照迭代序生成 → zpkg 字节漂移 → 破坏自举 byte-identical（`common-pitfalls.md §1`）
- [x] 3.5 S2：`ToString`/`Equals`/`GetHashCode`/`GetType` 从转发面剔除
- [x] 3.6 S5：外层已声明同名 → **按名字**跳过（arity 不同也跳，见 1.C）+ info 诊断

## ④ 生成与发射

- [x] 4.1 生成转发方法源码 → `GenSink.Augment`（同命名空间 partial 碎片）
- [x] 4.2 S1：**不产生子类型、不产生隐式转换**——只生成方法，不碰类型关系
- [x] 4.3 S3：不传递不递归，只提升一层
- [x] 4.4 **`[build] generated_dir`**（User 已裁决：走 toml 不走 CLI flag，没配置就用默认值）：
      目录型配置，**默认 `<output_dir>/generated`** ⇒ **恒落盘**（配的是「写到哪」不是「写不写」）。
      `ManifestLoader._parseBuild` 工程级(:216) + 工作区级(:167) 两处，沿用 `cache_dir`/`dist_dir`
      的「目录 + 从 output_dir 派生默认值」形状。文件名沿用现有 CU 命名
      `<generated_dir>/<pkg>/__gen$<Name>$augment.z42`。
      `generated_dir = ""` 显式表示不落盘（只读 FS / CI 不要额外产物时用），但**默认是写**。
- [x] 4.5 🔴 **`generated_dir` 必须在 `[sources] include` 扫描范围之外**——否则下次构建把生成的
      partial 碎片当用户源码再编一遍 ⇒ 同名成员两份碎片 ⇒ 直接踩 1.C 的**静默覆盖**（用户方法被
      吃掉 + 假类型错）。默认值在 `artifacts/` 下天然安全；但**这是用户可配路径**，必须校验并
      **报错拒绝**，不能只 warn。

## ⑤ 诊断

- [x] 5.1 S4：多个 `[Forward]` 提供同名成员 → **声明处**报错（不是用时才报）
- [x] 5.2 白名单 `methodof` 指向的成员不在该字段类型上 → 报错
      （注：methodof 自身的 E0459/E0460 已覆盖「方法不存在/歧义」，这条补的是「存在但不属于这个字段」）
- [x] 5.3 **跨包类型用了 ①② 档 → 报错**（见 1.E）：诊断必须说清「跨包转发请改用 `partial`
      声明档」，并点出原因是跨包元数据不带参数名与 `ref`/`out`——不是「暂不支持」这种含糊话。
- [x] 5.4 S5 跳过时的 info 诊断（让用户知道「这个名字我没生成，因为你自己声明了」）
- [x] 5.5 `generated_dir` 落在源码扫描范围内 → **报错**（见 4.5；这个配置配错的后果是
      「你的方法静默消失」，不能只 warn）

## ⑥ 测试

- [x] 6.1 三档各一条 e2e
- [x] 6.2 S1–S5 各一条（S1 断言**不可**隐式转换 / S4 断言报错 / S5 断言用户那份仍在且可调用）
- [x] 6.3 🔴 **白名单编译检查的门**：`[Forward(methodof(I.NoSuch))]` 必须编译错——守住 2.2
- [x] 6.4 跨 zpkg：转发一个来自别的包的类型的成员
- [x] 6.5 **自举字节不动点**：同一源码连编两次，生成顺序稳定（守 3.4）
- [x] 6.6 `generated_dir` 三态：默认（文件出现在 `<output_dir>/generated/`）/ 自定义路径 /
      `""` 关闭（一个字节都不写）。每态都要**再编一次仍字节不动点**——证明落盘产物不参与编译输入。
      另一条负例：把 `generated_dir` 指进 `src/` → 必须报错（守 4.5/5.5）
- [x] 6.7 jit 双验 `xtask test stdlib --mode jit`

## ⑦ 文档

- [x] 7.1 `docs/book/src/language/member-forwarding.md`：三档 / 五条语义规则 / **与多继承的界线**
      （S1 为什么必须 —— D 的 alias this 教训）/ 已知损耗
- [x] 7.2 `docs/book/src/SUMMARY.md` 挂目录
- [x] 7.3 `partial-types.md` 的「跨碎片重载静默覆盖」一节补一句：**本特性依赖 S5 规避它**
- [x] 7.4 归档随本 PR 一起提交

## ⑧ 自举纪律

- [x] 8.1 `[Forward]` 是新 attribute + 新 generator，**不是新语法**（不加 token）⇒ 不触发
      support/use 两阶段纪律。但 `ForwardAttribute` 落在 z42.core ⇒ **z42c/xtask 若要用它，
      须等含本变更的 nightly 发布**（stdlib API 面那根轴，`bootstrap-seed.md`）
- [x] 8.2 落地后跑 `xtask test bootstrap`

## GREEN 门

- [x] G1 `xtask test` 全绿
- [x] G2 `xtask test stdlib --mode jit` 全绿
- [x] G3 自举字节不动点 gen1 == gen2
- [x] G4 `xtask test lines` / `walkers` / `vscode-syntax` 全绿
- [x] G5 `xtask test bootstrap` 无越界
- [x] G6 6.3 的白名单门**跑过退回对照**（剥掉工厂 → 必须变红）
