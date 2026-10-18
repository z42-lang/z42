<!-- 由 z42 doc 生成，勿手改。源：src/libraries/z42.regex/ -->

# z42.regex —— 正则表达式编译与匹配。

> 包路径 `src/libraries/z42.regex/`；命名空间 `Std.Regex`

把 pattern 字符串编译成 `Regex` 对象，再做匹配 / 查找 / 替换 / 切分。语法是常见正则的
子集——够用于日志行解析、字段抽取、简单校验；不够用于需要 lookaround、backreference
或 Unicode 属性类的场景。

## 不支持

- **lookaround**：`(?=)` `(?!)` `(?<=)` `(?<!)`——编译报错
- **backreference**：pattern 里的 `\1`（静默退化成字面量）；replacement 里的 `$1` 是支持的
- **Unicode 属性类**：`\p{L}` / `\p{Greek}`；`\w` `\s` `\b` 全是 ASCII 定义
- **字符类内的转义类**：`[\d]` `[\w]` `[\s]`（静默退化成字面量）
- **`(?s)` / `(?x)` / 作用域 flag / pattern 中途改 flag**
- **占有量词**与**原子组**
- **`{,m}` 量词写法**
- **`\x` / `\u` 数值转义**（静默退化成字面量）
- **线性时间保证**：引擎是回溯式的，没有 ReDoS 防护，也没有步数 / 超时上限
- **并发复用同一个 `Regex`**：匹配状态存在实例字段上，多线程共享会互相踩

## Match

[`src/libraries/z42.regex/src/Match.z42:7`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.regex/src/Match.z42#L7)

一次匹配的结果：由 `Find` / `FindAll` 返回。

Group 0 = 整个 match；Group i（1-based）= 第 i 个捕获组。没参与本次匹配的组
返回空字符串 `""`（不是 null）。

```z42
public class Match {
    Match(string input, int start, int end, int[] groupStarts, int[] groupEnds, int groupCount, Regex owner)
    int Start()
    int End()
    int Length()
    string Value()
    string Group(int index)
    string GroupByName(string name)
    int GroupCount()
}
```

成员：[`Match`](#Std.Regex.Match.ctor(string,int,int,int[],int[],int,Regex)) · [`Start`](#Std.Regex.Match.Start()) · [`End`](#Std.Regex.Match.End()) · [`Length`](#Std.Regex.Match.Length()) · [`Value`](#Std.Regex.Match.Value()) · [`Group`](#Std.Regex.Match.Group(int)) · [`GroupByName`](#Std.Regex.Match.GroupByName(string)) · [`GroupCount`](#Std.Regex.Match.GroupCount())

<a id="Std.Regex.Match.ctor(string,int,int,int[],int[],int,Regex)"></a>
### Match(string input, int start, int end, int[] groupStarts, int[] groupEnds, int groupCount, Regex owner)

[`src/libraries/z42.regex/src/Match.z42:18`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.regex/src/Match.z42#L18)

构造一次匹配结果（记录输入、起止下标与各组捕获区间）。

<a id="Std.Regex.Match.Start()"></a>
### int Start()

[`src/libraries/z42.regex/src/Match.z42:29`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.regex/src/Match.z42#L29)

起始下标（含）。

<a id="Std.Regex.Match.End()"></a>
### int End()

[`src/libraries/z42.regex/src/Match.z42:31`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.regex/src/Match.z42#L31)

结束下标（不含）。

<a id="Std.Regex.Match.Length()"></a>
### int Length()

[`src/libraries/z42.regex/src/Match.z42:33`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.regex/src/Match.z42#L33)

End() - Start()。

<a id="Std.Regex.Match.Value()"></a>
### string Value()

[`src/libraries/z42.regex/src/Match.z42:36`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.regex/src/Match.z42#L36)

整个 match 的子串，等价 Group(0)。

<a id="Std.Regex.Match.Group(int)"></a>
### string Group(int index)

[`src/libraries/z42.regex/src/Match.z42:43`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.regex/src/Match.z42#L43)

返回第 index 个捕获组（0 = 整个 match，1..GroupCount = 捕获组）。

没参与本次匹配的组返回空字符串；越界抛 RegexException。

<a id="Std.Regex.Match.GroupByName(string)"></a>
### string GroupByName(string name)

[`src/libraries/z42.regex/src/Match.z42:56`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.regex/src/Match.z42#L56)

按名字查捕获组；没参与本次匹配返回空字符串，名字未声明则抛 RegexException。

<a id="Std.Regex.Match.GroupCount()"></a>
### int GroupCount()

[`src/libraries/z42.regex/src/Match.z42:65`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.regex/src/Match.z42#L65)

捕获组个数，不含 group 0。

## Regex

[`src/libraries/z42.regex/src/Regex.z42:24`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.regex/src/Regex.z42#L24)

编译后的 pattern：解析为 AST，提供匹配 / 查找 / 替换 / 切分方法。

线程内复用；同一个 `Regex` 实例不要跨线程并发调用（匹配过程写实例字段）。

```z42
public class Regex {
    static Regex Compile(string pattern)
    int GroupIndexOf(string name)
    int GroupCount()
    bool IsMatch(string input)
    Match Find(string input)
    Match[] FindAll(string input)
    string Replace(string input, string replacement)
    string[] Split(string input)
}
```

成员：[`Compile`](#Std.Regex.Regex.Compile(string)) · [`GroupIndexOf`](#Std.Regex.Regex.GroupIndexOf(string)) · [`GroupCount`](#Std.Regex.Regex.GroupCount()) · [`IsMatch`](#Std.Regex.Regex.IsMatch(string)) · [`Find`](#Std.Regex.Regex.Find(string)) · [`FindAll`](#Std.Regex.Regex.FindAll(string)) · [`Replace`](#Std.Regex.Regex.Replace(string,string)) · [`Split`](#Std.Regex.Regex.Split(string))

<a id="Std.Regex.Regex.Compile(string)"></a>
### static Regex Compile(string pattern)

[`src/libraries/z42.regex/src/Regex.z42:66`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.regex/src/Regex.z42#L66)

解析 pattern 并预处理开头的内联 flag；语法错误在此抛 RegexException，而不是匹配时。

<a id="Std.Regex.Regex.GroupIndexOf(string)"></a>
### int GroupIndexOf(string name)

[`src/libraries/z42.regex/src/Regex.z42:115`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.regex/src/Regex.z42#L115)

命名组名 → 1-based 下标；未声明返回 -1。

<a id="Std.Regex.Regex.GroupCount()"></a>
### int GroupCount()

[`src/libraries/z42.regex/src/Regex.z42:127`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.regex/src/Regex.z42#L127)

捕获组个数，不含 group 0。

<a id="Std.Regex.Regex.IsMatch(string)"></a>
### bool IsMatch(string input)

[`src/libraries/z42.regex/src/Regex.z42:132`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.regex/src/Regex.z42#L132)

是否匹配；等价于 Find(input) != null。

<a id="Std.Regex.Regex.Find(string)"></a>
### Match Find(string input)

[`src/libraries/z42.regex/src/Regex.z42:137`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.regex/src/Regex.z42#L137)

返回第一个 match；无匹配返回 null。`^` 开头（且未开 `(?m)`）时只尝试位置 0。

<a id="Std.Regex.Regex.FindAll(string)"></a>
### Match[] FindAll(string input)

[`src/libraries/z42.regex/src/Regex.z42:142`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.regex/src/Regex.z42#L142)

返回所有非重叠 match（可能为空数组）；空匹配也计入，每遇空匹配起点前进一格。

<a id="Std.Regex.Regex.Replace(string,string)"></a>
### string Replace(string input, string replacement)

[`src/libraries/z42.regex/src/Regex.z42:168`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.regex/src/Regex.z42#L168)

替换所有非重叠 match；replacement 支持 `$0` / `$1`…`$9` 占位符与 `$$`（字面 `$`）。

<a id="Std.Regex.Regex.Split(string)"></a>
### string[] Split(string input)

[`src/libraries/z42.regex/src/Regex.z42:239`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.regex/src/Regex.z42#L239)

按 match 切分；无匹配返回 [input]，空输入返回 [""]。

## RegexException

[`src/libraries/z42.regex/src/RegexException.z42:7`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.regex/src/RegexException.z42#L7)

Regex 编译 / 匹配错误。

由 Regex.Compile（非法 pattern）和 Match.Group / Match.GroupByName（下标越界 /
名字未声明）抛出。匹配过程本身不抛这个异常。

```z42
public class RegexException : Exception {
    RegexException(string message)
}
```

成员：[`RegexException`](#Std.Regex.RegexException.ctor(string))

<a id="Std.Regex.RegexException.ctor(string)"></a>
### RegexException(string message)

[`src/libraries/z42.regex/src/RegexException.z42:9`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.regex/src/RegexException.z42#L9)

用错误信息构造异常。

## RegexNode

[`src/libraries/z42.regex/src/RegexNode.z42:7`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.regex/src/RegexNode.z42#L7)

正则 AST 节点：用 `_kind` + 各 field 表示 discriminated union（z42 无 ADT）。

顶层 pattern 是 RegexNode[]（顺序即 concat）；ALT / QUANT / GROUP 的 child 同样是
RegexNode[]。序列用 raw `RegexNode[]` + 显式 count 字段存储。属引擎内部结构，不构成稳定 API。

```z42
public class RegexNode {
    int _kind
    int _ch
    bool _negated
    int[] _rangeStarts
    int[] _rangeEnds
    int _rangeCount
    RegexNode[] _leftSeq
    int _leftCount
    RegexNode[] _rightSeq
    int _rightCount
    RegexNode[] _childSeq
    int _childCount
    int _min
    int _max
    bool _greedy
    int _groupIndex
    RegexNode(int kind)
    static RegexNode Lit(int ch)
    static RegexNode Any()
    static RegexNode Charset(bool negated, int[] starts, int[] ends, int count)
    static RegexNode AnchorStart()
    static RegexNode AnchorEnd()
    static RegexNode WordBoundary()
    static RegexNode WordNonBoundary()
    static RegexNode Alt(RegexNode[] left, int leftCount, RegexNode[] right, int rightCount)
    static RegexNode Quant(RegexNode[] child, int childCount, int min, int max)
    static RegexNode QuantLazy(RegexNode[] child, int childCount, int min, int max)
    static RegexNode Group(RegexNode[] child, int childCount, int groupIndex)
    bool CharsetMatches(int ch)
}
```

成员：[`_kind`](#Std.Regex.RegexNode._kind) · [`_ch`](#Std.Regex.RegexNode._ch) · [`_negated`](#Std.Regex.RegexNode._negated) · [`_rangeStarts`](#Std.Regex.RegexNode._rangeStarts) · [`_rangeEnds`](#Std.Regex.RegexNode._rangeEnds) · [`_rangeCount`](#Std.Regex.RegexNode._rangeCount) · [`_leftSeq`](#Std.Regex.RegexNode._leftSeq) · [`_leftCount`](#Std.Regex.RegexNode._leftCount) · [`_rightSeq`](#Std.Regex.RegexNode._rightSeq) · [`_rightCount`](#Std.Regex.RegexNode._rightCount) · [`_childSeq`](#Std.Regex.RegexNode._childSeq) · [`_childCount`](#Std.Regex.RegexNode._childCount) · [`_min`](#Std.Regex.RegexNode._min) · [`_max`](#Std.Regex.RegexNode._max) · [`_greedy`](#Std.Regex.RegexNode._greedy) · [`_groupIndex`](#Std.Regex.RegexNode._groupIndex) · [`RegexNode`](#Std.Regex.RegexNode.ctor(int)) · [`Lit`](#Std.Regex.RegexNode.Lit(int)) · [`Any`](#Std.Regex.RegexNode.Any()) · [`Charset`](#Std.Regex.RegexNode.Charset(bool,int[],int[],int)) · [`AnchorStart`](#Std.Regex.RegexNode.AnchorStart()) · [`AnchorEnd`](#Std.Regex.RegexNode.AnchorEnd()) · [`WordBoundary`](#Std.Regex.RegexNode.WordBoundary()) · [`WordNonBoundary`](#Std.Regex.RegexNode.WordNonBoundary()) · [`Alt`](#Std.Regex.RegexNode.Alt(RegexNode[],int,RegexNode[],int)) · [`Quant`](#Std.Regex.RegexNode.Quant(RegexNode[],int,int,int)) · [`QuantLazy`](#Std.Regex.RegexNode.QuantLazy(RegexNode[],int,int,int)) · [`Group`](#Std.Regex.RegexNode.Group(RegexNode[],int,int)) · [`CharsetMatches`](#Std.Regex.RegexNode.CharsetMatches(int))

<a id="Std.Regex.RegexNode._kind"></a>
### int _kind

[`src/libraries/z42.regex/src/RegexNode.z42:19`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.regex/src/RegexNode.z42#L19)


<a id="Std.Regex.RegexNode._ch"></a>
### int _ch

[`src/libraries/z42.regex/src/RegexNode.z42:20`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.regex/src/RegexNode.z42#L20)


<a id="Std.Regex.RegexNode._negated"></a>
### bool _negated

[`src/libraries/z42.regex/src/RegexNode.z42:22`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.regex/src/RegexNode.z42#L22)


<a id="Std.Regex.RegexNode._rangeStarts"></a>
### int[] _rangeStarts

[`src/libraries/z42.regex/src/RegexNode.z42:23`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.regex/src/RegexNode.z42#L23)


<a id="Std.Regex.RegexNode._rangeEnds"></a>
### int[] _rangeEnds

[`src/libraries/z42.regex/src/RegexNode.z42:24`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.regex/src/RegexNode.z42#L24)


<a id="Std.Regex.RegexNode._rangeCount"></a>
### int _rangeCount

[`src/libraries/z42.regex/src/RegexNode.z42:25`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.regex/src/RegexNode.z42#L25)


<a id="Std.Regex.RegexNode._leftSeq"></a>
### RegexNode[] _leftSeq

[`src/libraries/z42.regex/src/RegexNode.z42:27`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.regex/src/RegexNode.z42#L27)


<a id="Std.Regex.RegexNode._leftCount"></a>
### int _leftCount

[`src/libraries/z42.regex/src/RegexNode.z42:28`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.regex/src/RegexNode.z42#L28)


<a id="Std.Regex.RegexNode._rightSeq"></a>
### RegexNode[] _rightSeq

[`src/libraries/z42.regex/src/RegexNode.z42:29`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.regex/src/RegexNode.z42#L29)


<a id="Std.Regex.RegexNode._rightCount"></a>
### int _rightCount

[`src/libraries/z42.regex/src/RegexNode.z42:30`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.regex/src/RegexNode.z42#L30)


<a id="Std.Regex.RegexNode._childSeq"></a>
### RegexNode[] _childSeq

[`src/libraries/z42.regex/src/RegexNode.z42:32`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.regex/src/RegexNode.z42#L32)


<a id="Std.Regex.RegexNode._childCount"></a>
### int _childCount

[`src/libraries/z42.regex/src/RegexNode.z42:33`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.regex/src/RegexNode.z42#L33)


<a id="Std.Regex.RegexNode._min"></a>
### int _min

[`src/libraries/z42.regex/src/RegexNode.z42:34`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.regex/src/RegexNode.z42#L34)


<a id="Std.Regex.RegexNode._max"></a>
### int _max

[`src/libraries/z42.regex/src/RegexNode.z42:35`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.regex/src/RegexNode.z42#L35)


<a id="Std.Regex.RegexNode._greedy"></a>
### bool _greedy

[`src/libraries/z42.regex/src/RegexNode.z42:36`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.regex/src/RegexNode.z42#L36)


<a id="Std.Regex.RegexNode._groupIndex"></a>
### int _groupIndex

[`src/libraries/z42.regex/src/RegexNode.z42:38`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.regex/src/RegexNode.z42#L38)


<a id="Std.Regex.RegexNode.ctor(int)"></a>
### RegexNode(int kind)

[`src/libraries/z42.regex/src/RegexNode.z42:41`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.regex/src/RegexNode.z42#L41)

构造指定 kind 的节点（各 field 初始化为默认值）。

<a id="Std.Regex.RegexNode.Lit(int)"></a>
### static RegexNode Lit(int ch)

[`src/libraries/z42.regex/src/RegexNode.z42:61`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.regex/src/RegexNode.z42#L61)

构造字符字面节点。

<a id="Std.Regex.RegexNode.Any()"></a>
### static RegexNode Any()

[`src/libraries/z42.regex/src/RegexNode.z42:68`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.regex/src/RegexNode.z42#L68)

构造 `.` 节点（匹配任意单字符）。

<a id="Std.Regex.RegexNode.Charset(bool,int[],int[],int)"></a>
### static RegexNode Charset(bool negated, int[] starts, int[] ends, int count)

[`src/libraries/z42.regex/src/RegexNode.z42:73`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.regex/src/RegexNode.z42#L73)

构造字符类 `[...]` 节点。

<a id="Std.Regex.RegexNode.AnchorStart()"></a>
### static RegexNode AnchorStart()

[`src/libraries/z42.regex/src/RegexNode.z42:83`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.regex/src/RegexNode.z42#L83)

构造 `^` 锚节点。

<a id="Std.Regex.RegexNode.AnchorEnd()"></a>
### static RegexNode AnchorEnd()

[`src/libraries/z42.regex/src/RegexNode.z42:85`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.regex/src/RegexNode.z42#L85)

构造 `$` 锚节点。

<a id="Std.Regex.RegexNode.WordBoundary()"></a>
### static RegexNode WordBoundary()

[`src/libraries/z42.regex/src/RegexNode.z42:88`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.regex/src/RegexNode.z42#L88)

构造 `\b` 零宽词边界节点：两侧 word-ness 不同时匹配。

<a id="Std.Regex.RegexNode.WordNonBoundary()"></a>
### static RegexNode WordNonBoundary()

[`src/libraries/z42.regex/src/RegexNode.z42:91`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.regex/src/RegexNode.z42#L91)

构造 `\B` 零宽非词边界节点：两侧 word-ness 相同时匹配。

<a id="Std.Regex.RegexNode.Alt(RegexNode[],int,RegexNode[],int)"></a>
### static RegexNode Alt(RegexNode[] left, int leftCount, RegexNode[] right, int rightCount)

[`src/libraries/z42.regex/src/RegexNode.z42:94`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.regex/src/RegexNode.z42#L94)

构造交替 `a|b` 节点。

<a id="Std.Regex.RegexNode.Quant(RegexNode[],int,int,int)"></a>
### static RegexNode Quant(RegexNode[] child, int childCount, int min, int max)

[`src/libraries/z42.regex/src/RegexNode.z42:105`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.regex/src/RegexNode.z42#L105)

构造贪婪量词节点。

<a id="Std.Regex.RegexNode.QuantLazy(RegexNode[],int,int,int)"></a>
### static RegexNode QuantLazy(RegexNode[] child, int childCount, int min, int max)

[`src/libraries/z42.regex/src/RegexNode.z42:116`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.regex/src/RegexNode.z42#L116)

构造惰性（非贪婪）量词节点（`*?`、`+?`、`??`、`{n,m}?`）：先试最短 count。

<a id="Std.Regex.RegexNode.Group(RegexNode[],int,int)"></a>
### static RegexNode Group(RegexNode[] child, int childCount, int groupIndex)

[`src/libraries/z42.regex/src/RegexNode.z42:127`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.regex/src/RegexNode.z42#L127)

构造捕获组节点。

<a id="Std.Regex.RegexNode.CharsetMatches(int)"></a>
### bool CharsetMatches(int ch)

[`src/libraries/z42.regex/src/RegexNode.z42:136`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.regex/src/RegexNode.z42#L136)

CHARSET 是否匹配 ch（含 _negated 反转）。仅 _kind == 3 时调用。

## RegexParser

[`src/libraries/z42.regex/src/RegexParser.z42:7`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.regex/src/RegexParser.z42#L7)

手写递归下降 parser：把 pattern 字符串解析为 RegexNode[] AST。

语法错误抛 `Std.Regex.RegexException`；capturing group 编号从 1 开始，
按 `(` 出现顺序分配。属引擎内部结构，不构成稳定 API。

```z42
public class RegexParser {
    int GroupCount
    string[] GroupNames
    int[] GroupIndices
    int NamedGroupCount
    RegexParser(string src)
    RegexNode[] ResultNodes
    int ResultCount
    void ParsePattern()
}
```

成员：[`GroupCount`](#Std.Regex.RegexParser.GroupCount) · [`GroupNames`](#Std.Regex.RegexParser.GroupNames) · [`GroupIndices`](#Std.Regex.RegexParser.GroupIndices) · [`NamedGroupCount`](#Std.Regex.RegexParser.NamedGroupCount) · [`RegexParser`](#Std.Regex.RegexParser.ctor(string)) · [`ResultNodes`](#Std.Regex.RegexParser.ResultNodes) · [`ResultCount`](#Std.Regex.RegexParser.ResultCount) · [`ParsePattern`](#Std.Regex.RegexParser.ParsePattern())

<a id="Std.Regex.RegexParser.GroupCount"></a>
### int GroupCount

[`src/libraries/z42.regex/src/RegexParser.z42:19`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.regex/src/RegexParser.z42#L19)

解析完成后 = 已分配的 group 数（不含 group 0 = 整个 match）。

<a id="Std.Regex.RegexParser.GroupNames"></a>
### string[] GroupNames

[`src/libraries/z42.regex/src/RegexParser.z42:22`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.regex/src/RegexParser.z42#L22)

命名组名 → 1-based 捕获下标 的并行数组；供 Regex.Compile 转发进编译结果。

<a id="Std.Regex.RegexParser.GroupIndices"></a>
### int[] GroupIndices

[`src/libraries/z42.regex/src/RegexParser.z42:23`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.regex/src/RegexParser.z42#L23)


<a id="Std.Regex.RegexParser.NamedGroupCount"></a>
### int NamedGroupCount

[`src/libraries/z42.regex/src/RegexParser.z42:24`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.regex/src/RegexParser.z42#L24)


<a id="Std.Regex.RegexParser.ctor(string)"></a>
### RegexParser(string src)

[`src/libraries/z42.regex/src/RegexParser.z42:27`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.regex/src/RegexParser.z42#L27)

用待解析的 pattern 字符串构造 parser。

<a id="Std.Regex.RegexParser.ResultNodes"></a>
### RegexNode[] ResultNodes

[`src/libraries/z42.regex/src/RegexParser.z42:65`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.regex/src/RegexParser.z42#L65)

解析结果：RegexNode[] AST，配合 ResultCount 使用。

<a id="Std.Regex.RegexParser.ResultCount"></a>
### int ResultCount

[`src/libraries/z42.regex/src/RegexParser.z42:67`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.regex/src/RegexParser.z42#L67)

实际节点数（与 ResultNodes 配对）。

<a id="Std.Regex.RegexParser.ParsePattern()"></a>
### void ParsePattern()

[`src/libraries/z42.regex/src/RegexParser.z42:70`](https://github.com/z42-lang/z42/blob/main/src/libraries/z42.regex/src/RegexParser.z42#L70)

入口：解析完整 pattern；调用后读 ResultNodes / ResultCount / GroupCount。

