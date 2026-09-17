# z42.yaml

YAML 1.2 subset reader / writer. Pure-script stdlib mirroring
`z42.toml` and `z42.json`.

## Public API

```z42
using Std.Yaml;

YamlValue v = Yaml.Parse(text);   // string → YamlValue tree
string out  = Yaml.Stringify(v);  // YamlValue tree → block-style YAML
```

`YamlValue` is a discriminated-union scalar / sequence / mapping with
`Is*()` predicates, `As*()` accessors, and `Get / At / Add / Set` for
nested traversal. See [docs/reference/src/stdlib/yaml.md](../../../docs/reference/src/stdlib/yaml.md)
for the full API + supported syntax + Deferred items.

## Quick example

```z42
using Std.IO;
using Std.Yaml;

void Main() {
    string yaml = "name: alice\nfriends:\n  - bob\n  - charlie\nage: 30\n";
    YamlValue v = Yaml.Parse(yaml);
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

## Scope

- ✅ Block mapping / sequence with indentation-based nesting
- ✅ Flow mapping `{}` / flow sequence `[]`
- ✅ Plain / single-quoted / double-quoted strings (with `\n \t \"
  \\ \uXXXX` escapes)
- ✅ Scalars: `null` (`~`) / bool / int / float / string / timestamp
  (ISO 8601 prefix); int hex `0xFF` / octal `0o755` literals
- ✅ Block scalars `|` literal / `>` folded (with `-` strip / `+`
  keep chomping + optional indent indicator)
- ✅ Comments (standalone + end-of-line)
- ✅ Anchors `&name` / aliases `*name` (per-doc scope, DeepClone
  resolution)
- ✅ Tags `!!str` / `!!int` / `!!float` / `!!bool` / `!!null` for
  explicit scalar coercion; unknown / local tags fall through to
  inference
- ✅ Multi-document streams via `YamlValue.ParseAll` (`---`
  separator, `...` end marker)
- ✅ Merge keys `<<: *anchor` (YAML 1.1 extension) — Docker Compose /
  Helm / K8s pattern; explicit keys override merged; quoted `"<<"`
  stays literal
- ✅ Stream overloads (`ParseStream` / `ParseAllStream` / `WriteTo`)
- ❌ Complex keys (`? sequence-as-key` syntax) — see
  `yaml-future-complex-keys` in
  [yaml.md](../../../docs/reference/src/stdlib/yaml.md#不支持)

## Composing configs with merge keys

```z42
string yaml = "x-common: &common\n"
    + "  restart: unless-stopped\n"
    + "  logging:\n"
    + "    driver: json-file\n"
    + "services:\n"
    + "  web:\n"
    + "    <<: *common\n"
    + "    image: nginx\n"
    + "  worker:\n"
    + "    <<: *common\n"
    + "    image: python\n";
YamlValue cfg = YamlValue.Parse(yaml);
// cfg.services.web has restart + logging + image; cfg.services.worker too.
```
