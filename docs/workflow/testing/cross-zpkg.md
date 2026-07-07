# Cross-zpkg 端到端测试

多 zpkg 协作场景（target lib + ext lib + main app 三方）的端到端测试。验证：跨包类型解析、impl 传播、namespace 解析、TSIG 序列化往返等。

## 命令

```bash
./xtask test e2e --dir cross-zpkg
```

## 用例布局

```
src/tests/cross-zpkg/
└── <test-name>/
    ├── target/             # 被引用的 lib
    │   ├── target.z42.toml
    │   └── src/...
    ├── ext/                # 扩展 lib（如 impl Trait for Target）
    │   ├── ext.z42.toml
    │   └── src/...
    ├── main/               # 入口 app
    │   ├── main.z42.toml
    │   └── src/...
    ├── libs/               # 测试驱动 populate 的 zpkg 集合
    └── expected_output.txt
```

驱动脚本编译 target → ext（依赖 target）→ main（依赖 target + ext），把所有 zpkg 放到 `libs/`，然后用 VM 跑 main entry。

## 典型场景

- `impl Trait for Type` 跨 zpkg 传播（L3-Impl2）
- 跨包 generic 类型实例化（`MyList<Other.Type>`）
- 跨包 interface dispatch
- `using` 解析跨包 namespace（strict-using-resolution）
- 同名 namespace 冲突检测（E0601）

## 加新用例

复制现有目录作模板（如 `cross_zpkg_impl_propagation/`），改源文件与 expected。详见 [`src/tests/cross-zpkg/README.md`](../../../src/tests/cross-zpkg/) 内说明。
