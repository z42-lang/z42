# proposal：fix-generic-ctor-args

## 一句话

`new Box<int>("not an int")` **编译期零诊断**，而同一个类上的 `b.Set("not an int")` 报 E0402
—— 同一件事两条路两个口径。把 ctor 这条路接上方法那条早就有的「代换后补查」。

## 症状

```z42
class Box<T> {
    private T _v;
    public Box(T v) { this._v = v; }
    public void Set(T v) { this._v = v; }
}

Box<int> n = new Box<int>("not an int");   // 修前：零诊断 → 运行期 InvalidCastException
n.Set("not an int");                       // 一直是：E0402 cannot assign string to Int32
```

**对照极干净**：同一个类、同一个型参、同一个错误实参，构造器放行、实例方法拦下。

## 根因

ctor 签名里的形参是**未代换**的裸 `T`（`Z42GenericParamType`），而
`Conversion` 分支 B 恰恰擦除「具体实参 → 裸型参形参」⇒ `CheckArgTypes` 放行。

方法那条路早有对策：`MemberResolver` 的 `Z42InstantiatedType` 收者分支用
`_substGenericSig` 按 receiver 的 TypeArgs 代换出**诊断专用**签名，再走
`CheckSubstitutedArgs`（`add-generic-type-arg-inference` 阶段 A）。**ctor 漏了这一支。**

## 改动

- `MemberResolver._substGenericSig`：`private` → `internal`（ctor 路径要用）。
- `ConstructTyper._bindCtorArgs`：新增 `inst` 形参（本次 `new` 的实例化类型；`null` = 非泛型
  实例化 / base-ctor 调用那条 ⇒ 整条跳过，行为恒等），并在**两个返回点**都调新的
  `_chkCtorSubstArgs`。

🔴 **两个返回点都要调**：局部声明的 ctor（有 Decl）走 `_adaptArgs` 分支并**提前 return**，
只在函数尾部补一次**等于没改**——初稿就是这么错的，**构建通过、行为一字未变**，
靠 `--dump-bound` 排除「类型不是实例化类型」之后回头读控制流才发现。
⇒ **改一个函数里的检查前，先确认它有几个返回点**（同 ⑧ 那次「nsMap 有四个构建点」）。

⚠️ 沿用方法路径的两条纪律：
- **代换结果只作诊断入参、绝不回灌**（`_withDefaults` / `BoxArgs` / params 的
  normal-expanded 翻转 / ConvertInstr 插入 都对形参类型敏感，回灌是确定性自举字节漂移）。
- 用 `CheckSubstitutedArgs` 而非 `CheckArgTypes`：**只补查原签名含型参的那些位**，
  具体类型的位上面已经查过，整条重查会同 span 同消息报两条。

⚠️ `_adaptArgs` 那个返回点传 `rawArgs = null`：它做过命名实参重排，位置可能已与 rawArgs
不对应；该参数只用于取 span，null 时回落 `args[i].Span`（仍指向实参本身）。

## 爆炸半径

全仓自编译 + 全量测试零新增诊断（stdlib 里没有「给泛型 ctor 传错型实参」的存量）。

## 三处联动（记忆里预告过，如期判红）

活示例 `examples/types/generics/gaps/ctorgap.z42` 是**演示这个缺陷**的：
示例注释 + `run.console` transcript + 第 18 章正文标题「泛型构造器的实参不检查」全要改。
⚠️ transcript 的列号**以实跑为准**（我手写猜 30、实际 31，被 examples 门禁抓下）。

顺带订正：第 18 章「两个会崩的组合」里的「`new T()` 且 T 是基元」**已随 #803 修好**。
