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

### Match(string input, int start, int end, int[] groupStarts, int[] groupEnds, int groupCount, Regex owner)

构造一次匹配结果（记录输入、起止下标与各组捕获区间）。

### int Start()

起始下标（含）。

### int End()

结束下标（不含）。

### int Length()

End() - Start()。

### string Value()

整个 match 的子串，等价 Group(0)。

### string Group(int index)

返回第 index 个捕获组（0 = 整个 match，1..GroupCount = 捕获组）。

没参与本次匹配的组返回空字符串；越界抛 RegexException。

### string GroupByName(string name)

按名字查捕获组；没参与本次匹配返回空字符串，名字未声明则抛 RegexException。

### int GroupCount()

捕获组个数，不含 group 0。

## Regex

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

### static Regex Compile(string pattern)

解析 pattern 并预处理开头的内联 flag；语法错误在此抛 RegexException，而不是匹配时。

### int GroupIndexOf(string name)

命名组名 → 1-based 下标；未声明返回 -1。

### int GroupCount()

捕获组个数，不含 group 0。

### bool IsMatch(string input)

是否匹配；等价于 Find(input) != null。

### Match Find(string input)

返回第一个 match；无匹配返回 null。`^` 开头（且未开 `(?m)`）时只尝试位置 0。

### Match[] FindAll(string input)

返回所有非重叠 match（可能为空数组）；空匹配也计入，每遇空匹配起点前进一格。

### string Replace(string input, string replacement)

替换所有非重叠 match；replacement 支持 `$0` / `$1`…`$9` 占位符与 `$$`（字面 `$`）。

### string[] Split(string input)

按 match 切分；无匹配返回 [input]，空输入返回 [""]。

## RegexException

Regex 编译 / 匹配错误。

由 Regex.Compile（非法 pattern）和 Match.Group / Match.GroupByName（下标越界 /
名字未声明）抛出。匹配过程本身不抛这个异常。

```z42
public class RegexException : Exception {
    RegexException(string message)
}
```

### RegexException(string message)

用错误信息构造异常。

## RegexNode

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

### int _kind


### int _ch


### bool _negated


### int[] _rangeStarts


### int[] _rangeEnds


### int _rangeCount


### RegexNode[] _leftSeq


### int _leftCount


### RegexNode[] _rightSeq


### int _rightCount


### RegexNode[] _childSeq


### int _childCount


### int _min


### int _max


### bool _greedy


### int _groupIndex


### RegexNode(int kind)

构造指定 kind 的节点（各 field 初始化为默认值）。

### static RegexNode Lit(int ch)

构造字符字面节点。

### static RegexNode Any()

构造 `.` 节点（匹配任意单字符）。

### static RegexNode Charset(bool negated, int[] starts, int[] ends, int count)

构造字符类 `[...]` 节点。

### static RegexNode AnchorStart()

构造 `^` 锚节点。

### static RegexNode AnchorEnd()

构造 `$` 锚节点。

### static RegexNode WordBoundary()

构造 `\b` 零宽词边界节点：两侧 word-ness 不同时匹配。

### static RegexNode WordNonBoundary()

构造 `\B` 零宽非词边界节点：两侧 word-ness 相同时匹配。

### static RegexNode Alt(RegexNode[] left, int leftCount, RegexNode[] right, int rightCount)

构造交替 `a|b` 节点。

### static RegexNode Quant(RegexNode[] child, int childCount, int min, int max)

构造贪婪量词节点。

### static RegexNode QuantLazy(RegexNode[] child, int childCount, int min, int max)

构造惰性（非贪婪）量词节点（`*?`、`+?`、`??`、`{n,m}?`）：先试最短 count。

### static RegexNode Group(RegexNode[] child, int childCount, int groupIndex)

构造捕获组节点。

### bool CharsetMatches(int ch)

CHARSET 是否匹配 ch（含 _negated 反转）。仅 _kind == 3 时调用。

## RegexParser

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

### int GroupCount

解析完成后 = 已分配的 group 数（不含 group 0 = 整个 match）。

### string[] GroupNames

命名组名 → 1-based 捕获下标 的并行数组；供 Regex.Compile 转发进编译结果。

### int[] GroupIndices


### int NamedGroupCount


### RegexParser(string src)

用待解析的 pattern 字符串构造 parser。

### RegexNode[] ResultNodes

解析结果：RegexNode[] AST，配合 ResultCount 使用。

### int ResultCount

实际节点数（与 ResultNodes 配对）。

### void ParsePattern()

入口：解析完整 pattern；调用后读 ResultNodes / ResultCount / GroupCount。

