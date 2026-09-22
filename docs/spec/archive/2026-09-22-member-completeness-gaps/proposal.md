# proposal：member-completeness-gaps（三合一 gap 扫描产出）

一轮成员/访问器/接口完整性 gap 扫描（延续 #727/#737/#743 属性访问器线）产出的三个真洞，**均无格式
bump**，一并处理。三者独立、各自一个逻辑 commit。

## #1 跨包接口属性/索引器访问器（`add-crosspkg-interface-accessors`）

**洞**：`ClassDescBuilder._interfaceDesc` 的接口方法块只处理 `MethodDecl` ⇒ 用户接口的属性/索引器访问器
（get_X/set_X/get_Item/set_Item）**跨包一个都过不去**（`IEnumerator.get_Current` 能跨包纯靠 `BuiltinTypeDefs`
硬编码注入）。经接口静态类型读跨包接口属性 → 导入侧 `it.Methods` 无 get_X → 解析失败。

**修**：接口方法块循环补 `PropertyDecl`→get_X/set_X、`IndexerDecl`→get_Item/set_Item（作普通方法进 TYPE
record，与本包 `MemberCollector._fillInterface` 一致）。set_X value 参=属性类型、void 返回；接口访问器恒实例。
数组增长收敛到新 `_IfaceMethodBuf` holder。**无格式 bump**——走既有通用方法块（reader/reconcile/loader 三段
本就按名全量搬运）。**校正 #727 文档写错的「需格式 bump」**。

## #2 接口非法成员诊断（`add-interface-member-check`，新码 E0478）

**洞**：`MemberCollector._fillInterface` 只认 Assoc/Method/Property/Indexer ⇒ 接口里写**字段**（`static int X;`）
或**嵌套类型**被**静默跳过**（`interface I { static int X; }` 编译通过但 X 无处可用）。

**修**：else 分支补 FieldDecl / ClassDecl → **E0478**。events 已在 parser lower 成 add_/remove_ MethodDecl，不漏。

## #3 比较/相等/位运算符重载（`add-comparison-operators`）

**洞**：`_operatorMethodName`（声明侧）/ `_operatorMethodNameTc`（派发侧）只映射 `+ - * / %` ⇒ `operator ==`
落到非法名 `op_==`、`==` 派发返 "" 从不派发。**今天定义不了 `operator ==` `<` `>` 等**（implicit/explicit 转换
运算符已支持）。

**修**：两个 map 对齐 C# 名补 `== != < > <= >=`（比较/相等）+ `& | ^ << >>`（位）。绑定器已从 `opMs.Signature.Ret`
取返回类型（`==`→bool 天然正确），无 binder 改动。**零回归**：无 op_X 方法的类型 `_resolveOverload` 返 null
回落 BinaryTypeTable（引用相等/原生）；record == 实测新旧一致（records 值类型走原生、op_Equality 非可解析符号）。

## 无格式 bump / 自举

三者纯编译期 + zbc TYPE record（既有布局）。#1 改接口方法块内容（IEnumerator/IPipelineContext 的 zbc 字节增，
golden 自动重生），但 `IfaceMethod*` 是既有 zbc 1.28 字段，**无格式 bump**。z42c/stdlib 源不新用比较运算符/
自定义 setter（bootstrap 轴①）。
