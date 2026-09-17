# stdlib 接口面设计准则

> 对齐：2026-09-17 ｜ 代码：`src/libraries/z42.io/src/Stream.z42`、`BinaryReader.z42`、
> `src/libraries/z42.json/src/JsonValue.z42`
>
> 与[实现分层](architecture.md)正交：那管「实现住哪一层」，这管「对外暴露多少接口、怎么暴露」。
> 与[包划分](organization.md)正交：那管「类型住哪个包」。

新加 stdlib 接口时遵循。三条准则解决的是同一个病：**接口数量随维度相乘而爆炸**。

## 准则 1：Source × Operation 漏斗

### 反模式

把「数据从哪来」（source：文件路径 / 字节数组 / 流 / 网络）和「对数据做什么」（operation：读全文 /
读行 / 读全字节 / parse）耦进同一个签名（甚至函数名），注定 **m×n** 个接口：

```z42
// ❌ 每加一种源 × 每种操作都新开一个接口
string ReadTextFromFile(string path);
string ReadTextFromBytes(byte[] data);
string ReadTextFromStream(Stream s);
string[] ReadLinesFromFile(string path);   // …继续乘
```

### 准则

**别把 source 编进 operation 的签名 / 名字；把 source 做成一个「值」传进来。** 让所有 source 汇聚到
同一抽象，每个 operation 只对这个抽象写一次：

- m 个 source → m 个**适配器 / 构造器**（把自己变成该抽象）
- n 个 operation → n 个**只接该抽象**的函数
- 总数 **m + n**，不是 m×n

operation 应「接受最一般的类型」，调用方有任何源都能用。

### 范例：`Std.IO.Stream`

这条准则在 z42 stdlib 是事实标准：

**唯一字节漏斗 `Stream`**，每个源实现一次——`FileStream` / `MemoryStream(byte[])`（**零拷贝只读
视图**，构造器不复制，直到调用方 `ToArray()` 才快照）/ `BufferedStream` / `ProcessOutputStream` /
`ProcessStdinStream`（`z42.io`）/ `NetworkStream` / `TlsStream` / `_HttpBodyStream`（`z42.net`）/
`CompressionEncoderStream` / `CompressionDecoderStream`（`z42.compression`）。**跨三个包共 10 个子类，
没有一个重新实现 operation。**

**operation 只在基类写一次**：`Stream` 上这 5 个是**非 virtual** 的具体方法，子类不重写——
`ReadAllBytes()` / `ReadExactly(int)` / `WriteAllBytes(byte[])` / `CopyTo(Stream)` /
`CopyTo(Stream, int)`。子类要填的是另外 12 个 virtual 钩子（`CanRead` / `CanWrite` / `CanSeek` /
`Read` / `Write` / `ReadByte` / `WriteByte` / `Flush` / `Close` / `Length` / `Position` / `Seek`）。

这条 virtual / 非 virtual 的划线是漏斗能收敛的**机械保证**：把 operation 写成 virtual，就等于邀请每个
子类各实现一遍，漏斗当场退化回 m×n。新增 operation 时先问「它能不能只用那 12 个钩子写出来」——能，
就写成非 virtual 放基类。

**编码轴单独收掉**（见准则 2）：`StreamReader(Stream)` / `StreamReader(Stream, Encoding)` 在 `Stream`
之上叠 char 层。

> 新加「从某种源读 / 写」的能力时：**实现一个 `: Stream` 适配器**，复用全部既有 operation。不要新开
> 一组 `XxxFromYyy` 接口，也不要为此开新包。

## 准则 2：正交轴各自收敛，别相乘

`文本 = 字节 + 编码`。若写 `ReadTextUtf8` / `ReadTextAscii` × 每种源，编码就成了第三根轴（m×n×k）。
正解是把每根正交轴**各收敛成一个参数 / 一层**，而非乘进函数名：

- **编码** → `Encoding` 参数 / `TextReader` 层，不是 `…Utf8` / `…Ascii` 后缀。
- **格式**（json / toml / yaml）→ 解析器也接 `Stream`，绝不为「从文件 parse」「从字节 parse」各开一个。
  漏斗思想**递归适用**。

`z42.json` / `z42.toml` / `z42.yaml` 三个包都遵循这条，各自只有「字符串入口 + 流入口」两个，源的
多样性全部由 `Stream` 那一侧吸收。

⚠️ **但流入口取的是另一个名字，不是重载**：`JsonValue.Parse(string)` 旁边站着
`JsonValue.ParseStream(Stream)`，`BinaryReader(byte[])` 旁边站着静态工厂
`BinaryReader.OverStream(Stream)`。源码注释把这个改名归因于一条编译器限制：多个候选同为 arity-1 时，
重载决议挑**先声明**的那个而不看实参类型，于是 `(byte[])` 与 `(Stream)` 两条会互相抢。

**该限制现在不存在**——实测两个 arity-1 静态重载与两个 arity-1 构造器都能按实参类型正确决议，
`byte[]` 对 class 这一组也不例外：

```z42
public class S { public int V; }
public static class F {
    public static string Take(byte[] d) { return "bytes"; }
    public static string Take(S s)      { return "S"; }
}
// F.Take(new byte[0]) → "bytes"；F.Take(new S()) → "S"；两个 arity-1 构造器同理
```

所以：**新接口直接写重载**。已有的 `ParseStream` / `OverStream` 要不要收敛回去见本页末。

## 准则 3：便利糖是薄委托，不是重复逻辑

高频组合（「文件读全文」）保留一行便利版是对的，但守纪律：

- 便利函数 / 重载是 **2 行委托**（适配器 + 核心 operation），**不重复任何逻辑**；
- **只给少数主路径**铺糖，不铺满 m×n 网格；
- 逻辑永远只活在 m+n 那一层。

范例：`BinaryReader(byte[] data)` 的全部实现是 `this._stream = new MemoryStream(data);`——一行委托，
读取逻辑一行都没有重复。**罪不在重载 / 便利本身，在每个源重新实现一遍逻辑。**

## 与语言演进的关系

- **现在**：抽象载体用 **`class` 基类 / `interface`**（`Stream`）——无需泛型，现在就能做。
- **以后**：traits / 泛型落地后可演进成 Rust 式 `Read` trait + 泛型 / 默认方法 operation。接口 / 基类
  模型**前向兼容**：今天的「非 virtual 具体 operation + 少量 virtual 钩子」正好对应 trait 的「默认方法
  + required 方法」，迁移是换语法不是重画结构。

## 待处理

`JsonValue.ParseStream` / `ParseRelaxedStream` / `BinaryReader.OverStream` 三处的绕行名，其理由
（准则 2 里那条重载决议限制）现已不成立。收敛回重载会改动公开 API 面——旧名留多久、怎么弃用——需要
单独一个 change 定，不要顺手改。
