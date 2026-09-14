# 设计：fix-iface-self-completeness-gaps

## A2：impl 块接口成员齐备性

### 为什么不能只「迁移 pass」

`_checkIfaceMembersComplete` 走 **AST 的 `c.Bases`**（不是 `ct.InterfaceNames`），因为要拿泛型接口
的类型实参（`class Foo<U> : IColl<U>` 的 `T := U`）——`InterfaceNames` 只存裸名、实参被
`StubCollector` 丢掉。而 impl 块接口**根本不在 `c.Bases` 里**（它来自独立的 `ImplDecl`，由 `_passImpls`
并进 `ct.InterfaceNames`+`ct.Methods`）。⇒ 单纯把满足性校验迁到 `_passImpls` 之后没用，必须**新增一条
迭代 `ImplDecl` 的检查**。

### 新 pass `_passImplIfaceComplete`

- 迭代 `cu.Decls` 里的 `ImplDecl`（同 `_passImpls`），按 `TargetType`/`TraitType`（`NamedType`）
  解析 target 类 + trait 接口，调 `_checkOneIfaceMembers`。
- **必须在 `_passImpls`（并 trait + impl 方法）之后**。多-CU 编排是「每 pass 跨所有 CU 跑完再进下一
  pass」，故新 pass 独立成一轮跨 CU 循环——target 与其 impl 块可能分处不同 CU，要等 `_passImpls`
  全跑完才能看到全部并入的 trait/方法。
- **声明接口路径不受影响**（`c.Bases` 走 `_passSealedEnforce` 内的 `_checkIfaceMembersComplete`，
  逐字不变）——新 pass 纯**追加** impl 块覆盖，两条路互不重叠、无双报。

### `_checkOneIfaceMembers`/`_checkOneIfaceMethod` 的 `ClassDecl c` → `name`+`Span`

impl 块的实现方 `ClassDecl` 可能跨 CU、这里只有 `ImplDecl`。把错误定位参数从 `ClassDecl c` 改成
`string cName` + `Span cSpan`：声明路径传 `c.Name`/`c.Span`（等价，逐字不变），impl 路径传
`target.Name()`/`implD.Span`。函数体只用到 `c.Name`（== `ct.Name()`）与 `c.Span` 两处。

## A3-Func：`_substSelf` / `_substForIface` 下钻 `Z42FuncType`

两个替换函数原来只递归 `Z42ArrayType`（数组元素）+ `Z42InstantiatedType`（泛型实参），漏 `Z42FuncType`。
补分支：递归 `ParamTypes` + `Ret`，重建 `Z42FuncType`，`ParamsFrom`/`ParamDefaults`/`ParamCallers`
原样搬（同 `_substSelfSig`，漏搬会让 params 尾参在这条路上退化定长）。

- `_substSelf`（`MemberResolver`，`public static`）：调用点返回位替换，`Self`→接收者接口静态类型。
- `_substForIface`（`InheritanceResolver`）：满足性校验期望签名，`Self`→实现类。
- 对齐 `_containsSelf`（禁令扫描，早已下钻 Func）——「禁止侧比替换侧宽」是安全方向，本轮把替换侧补齐。

## A3-索引器：接口索引器端到端

三处缺失（缺一不可，任何一处漏则整条不通）：

1. **解析**（`MemberParser._parseIndexer`）：`get`/`set` 后判 `LBrace`/`FatArrow`（有体，类）
   vs 其它（无体 `get;`，接口/抽象，`_expectSemi()`）——镜像 `_parseProperty`。原来无条件走
   `_parseAccessorBody`（要 `{`）⇒ 接口 `get;` 的 `;` 被当块体、`_parseBlock` 吞掉接口闭合 `}`。
2. **收集**（`MemberCollector._fillInterface`）：`IndexerDecl` → `get_Item`/`set_Item` 方法符号
   （镜像 `_fillClass`），返回位/形参经 `tp`/`tpc`（含 `Self`）解析。接口成员恒 `public`。
3. **使用侧**：`ExprTyper._bindIndex`（读）+ `AssignTyper._bindAssign`（写）新增 `Z42InterfaceType`
   收者分支，解析 `get_Item`/`set_Item`。读侧返回位 `gret = _substSelf(gm.Signature.Ret, ixIf)`
   （`Self`→接口自身，同 #527）；型参擦除 `Unknown`（运行期经 DepIndex 派发）。

## 验证

- **单元真门**（`constraint_tests.z42`，`SemanticDump`）：A2×3（缺成员/正例/错签名）+ Func 参数满足性×2
  + DumpBody×3（Func 返回替换 / 索引器 get_Item / 索引器 Self 返回替换）。退回对照逐条坐实：
  disable A2 → 缺成员+错签名 FAIL；disable `_substSelf` Func → Func 返回 FAIL；disable `_substForIface`
  Func → Func 参数正例假红 FAIL；索引器/解析器退回态在迭代中亲见（`:<error>` / AST 嵌套）。
- **运行期 e2e**（`src/tests/interfaces/interface_indexer.z42`）：接口索引器读（get）+ 读写（get+set）
  经接口静态类型，interp + jit 双绿。
- **GREEN**：完整 `xtask test` 全绿、自举不动点 3/3 gen1==gen2；`test stdlib/cross-zpkg --mode jit`；
  `test bootstrap` NO boundary violation（解析器改动无越界）。
