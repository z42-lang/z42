# z42.yaml

## 职责

YAML 1.2 子集 reader / writer，纯脚本实现，形态对齐 `z42.toml` 与 `z42.json`。
不做：复杂 key（`? sequence-as-key`）、序列项下的多行嵌套映射等（见 `docs/reference/src/stdlib/yaml.md`「不支持」节）。

## 功能索引
公开 API 的完整签名见 [reference 包页](../../../docs/reference/src/stdlib/yaml.md)（由源码 doc 注释生成）。

语法覆盖：block / flow 映射与序列；plain / 单双引号字符串（含转义）；`null` / bool / int（含 `0xFF` / `0o755`）/ float / timestamp；
block scalar `|` / `>`（含 chomping 与缩进指示）；注释；anchor `&` / alias `*`；`!!str` 等显式 tag；merge key `<<: *anchor`。

## 基础用法

```z42
using Std.IO;
using Std.Yaml;

void Main() {
    string yaml = "name: alice\nfriends:\n  - bob\n  - charlie\nage: 30\n";
    YamlValue v = YamlValue.Parse(yaml);
    Console.WriteLine("name: " + v.Get("name").AsString());
    Console.WriteLine("age: "  + v.Get("age").AsInt().ToString());
    YamlValue friends = v.Get("friends");
    int i = 0;
    while (i < friends.Length()) {
        Console.WriteLine("- " + friends.At(i).AsString());
        i = i + 1;
    }
}
```

merge key 组合配置（Docker Compose / Helm / K8s 常见写法；显式 key 覆盖被合并的 key）：

```z42
string yaml = "x-common: &common\n"
    + "  restart: unless-stopped\n"
    + "services:\n"
    + "  web:\n"
    + "    <<: *common\n"
    + "    image: nginx\n";
YamlValue cfg = YamlValue.Parse(yaml);   // services.web 含 restart + image
```

## 如何测试验证

```bash
xtask test stdlib z42.yaml    # 本库全部 [Test]
```

## 关联文档

- 用法与规则（参考手册）：[z42.yaml](../../../docs/reference/src/stdlib/yaml.md)
- 设计 / 机制（实现内幕）：[标准库三层架构](../../../docs/internals/src/stdlib/architecture.md) · [包划分与 interop 归属](../../../docs/internals/src/stdlib/organization.md)

## 核心文件

| 文件 | 类型 | 职责 |
|------|------|------|
| `src/YamlValue.z42` | `class YamlValue` | 值类型（scalar / sequence / mapping）+ 公开入口 |
| `src/YamlParser.z42` | `sealed class YamlParser` | 缩进敏感的 block / flow 解析器 |
| `src/YamlWriter.z42` | `sealed class YamlWriter` | block-style 序列化 |
| `src/YamlException.z42` | `class YamlException` | 解析错误（带位置） |

## 依赖关系
`z42.core` + `z42.io`（`ParseStream` / `WriteTo` 的 Stream 重载）。

## 待办
- 复杂 key（`? sequence-as-key`）
- 序列项下多行嵌套映射（目前每个 `- ` 只支持同行一对 `k: v`）
