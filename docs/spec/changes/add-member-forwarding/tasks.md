# Tasks: `[Forward]` 成员转发（add-member-forwarding）

> 状态：🟡 **DRAFT —— 待 User 确认后才进 IMPL** | 创建：2026-09-12 | 见 [proposal.md](proposal.md)
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
- [ ] 1.D **①② 档的签名保真度逐项确认**：默认值 / `params` / `ref`·`out`·`in` / 泛型方法，
      `MethodSymbol`（只带 `Z42FuncType`）到底带不带得出来。**带不出来的形态 v1 直接报错拒绝**，
      绝不生成一个签名不等价的方法——那是比不支持更坏的结果。
      （已知：**参数名一定丢**，见 proposal「已知损耗」。）

## ② Generator 框架（唯一的框架改动）

- [ ] 2.1 `GeneratorDriver` 触发点扫描扩展到**字段级** `AttributedDecl`
      （现仅顶层 `Inner is ClassDecl`，:137-140）
- [ ] 2.2 🔴 **不得剥掉字段级触发 attr 的合成工厂**（`fld$<Class>$<Field>`，`AttributeSynth:75-76`）——
      剥了白名单的 `methodof` 就不再被编译 ⇒ 编译期检查消失 ⇒ 白名单退回字符串时代。
      **本条要有一条会变红的测试盯着**（白名单写个不存在的方法 → 必须编译错）。
- [ ] 2.3 `ForwardAttribute : Attribute` 真类（合成工厂需要真类型，不能只当标记名）

## ③ 转发面收集

- [ ] 3.1 ① 档：解析 `[Forward<I>]` 的接口 → 取接口方法全集
- [ ] 3.2 ② 档：读 `Attr.Args` 的**原始 AST**（`MethodOfExpr` 节点 → `OwnerName`/`Member`/`ParamTypes`），
      不经运行期 `MethodInfo`
- [ ] 3.3 ③ 档：扫本类 `partial` 且无 body 的方法声明，按 `[Forward]` 字段配对
- [ ] 3.4 🔴 **生成顺序必须显式 sort**：`Z42ClassType.Methods` 是 hashed StrMap、迭代序不确定；
      确定序的 `OwnMethodNames` **只对 imported 类型和 impl-block 填，同包本地类是空的**。
      照迭代序生成 → zpkg 字节漂移 → 破坏自举 byte-identical（`common-pitfalls.md §1`）
- [ ] 3.5 S2：`ToString`/`Equals`/`GetHashCode`/`GetType` 从转发面剔除
- [ ] 3.6 S5：外层已声明同名 → **按名字**跳过（arity 不同也跳，见 1.C）+ info 诊断

## ④ 生成与发射

- [ ] 4.1 生成转发方法源码 → `GenSink.Augment`（同命名空间 partial 碎片）
- [ ] 4.2 S1：**不产生子类型、不产生隐式转换**——只生成方法，不碰类型关系
- [ ] 4.3 S3：不传递不递归，只提升一层
- [ ] 4.4 落盘开关（按 proposal 决策点 1 的裁决落地；若选 (a) 则 `--emit-generated <dir>`，默认关）

## ⑤ 诊断

- [ ] 5.1 S4：多个 `[Forward]` 提供同名成员 → **声明处**报错（不是用时才报）
- [ ] 5.2 白名单 `methodof` 指向的成员不在该字段类型上 → 报错
      （注：methodof 自身的 E0459/E0460 已覆盖「方法不存在/歧义」，这条补的是「存在但不属于这个字段」）
- [ ] 5.3 1.D 判定为不可保真的签名形态 → 明确报错说明「v1 不转发这种签名」，不静默生成不等价方法
- [ ] 5.4 S5 跳过时的 info 诊断（让用户知道「这个名字我没生成，因为你自己声明了」）

## ⑥ 测试

- [ ] 6.1 三档各一条 e2e
- [ ] 6.2 S1–S5 各一条（S1 断言**不可**隐式转换 / S4 断言报错 / S5 断言用户那份仍在且可调用）
- [ ] 6.3 🔴 **白名单编译检查的门**：`[Forward(methodof(I.NoSuch))]` 必须编译错——守住 2.2
- [ ] 6.4 跨 zpkg：转发一个来自别的包的类型的成员
- [ ] 6.5 **自举字节不动点**：同一源码连编两次，生成顺序稳定（守 3.4）
- [ ] 6.6 jit 双验 `xtask test stdlib --mode jit`

## ⑦ 文档

- [ ] 7.1 `docs/book/src/language/member-forwarding.md`：三档 / 五条语义规则 / **与多继承的界线**
      （S1 为什么必须 —— D 的 alias this 教训）/ 已知损耗
- [ ] 7.2 `docs/book/src/SUMMARY.md` 挂目录
- [ ] 7.3 `partial-types.md` 的「跨碎片重载静默覆盖」一节补一句：**本特性依赖 S5 规避它**
- [ ] 7.4 归档随本 PR 一起提交

## ⑧ 自举纪律

- [ ] 8.1 `[Forward]` 是新 attribute + 新 generator，**不是新语法**（不加 token）⇒ 不触发
      support/use 两阶段纪律。但 `ForwardAttribute` 落在 z42.core ⇒ **z42c/xtask 若要用它，
      须等含本变更的 nightly 发布**（stdlib API 面那根轴，`bootstrap-seed.md`）
- [ ] 8.2 落地后跑 `xtask test bootstrap`

## GREEN 门

- [ ] G1 `xtask test` 全绿
- [ ] G2 `xtask test stdlib --mode jit` 全绿
- [ ] G3 自举字节不动点 gen1 == gen2
- [ ] G4 `xtask test lines` / `walkers` / `vscode-syntax` 全绿
- [ ] G5 `xtask test bootstrap` 无越界
- [ ] G6 6.3 的白名单门**跑过退回对照**（剥掉工厂 → 必须变红）
