# VM Golden 测试

z42 VM 端到端测试（`src/tests/**/source.z42` + `expected_output.txt`）。Interpreter + JIT 双模式跑同一测试集。

## 命令

```bash
./xtask test e2e                   # 默认：自动重建 stdlib + golden zbc 后跑（interp + jit）
./xtask test e2e --mode interp            # 仅 interp
./xtask test e2e --mode jit               # 仅 JIT
./xtask test e2e --no-rebuild      # 跳过重建（反复跑同一测试时加速）
```

## 默认自动重建

`./xtask test e2e` 入口自动按依赖顺序：

1. `./xtask build stdlib` — z42c（z42 自举）编译 stdlib zpkgs → sync 到 `artifacts/build/libraries/dist/release/`
2. `./xtask build test` — 用最新 z42c 把所有 golden `source.z42` → `source.zbc`
3. `cargo build` VM
4. 逐个跑 golden test

> 来自 2026-05-04 `fix-test-vm-stale-artifacts`：早期入口不强制刷新依赖产物，多次出现"stdlib zpkg 旧 / golden zbc 旧"导致测试输出对当前代码不真实（假绿/假红）。
>
> `--no-rebuild` 仅供**确认上一次重建是新的**且只在反复迭代单个测试时使用。

## 跑单个 golden

直接调 VM：

```bash
cargo run --manifest-path src/runtime/Cargo.toml -- src/tests/<category>/<name>/source.zbc
cargo run --manifest-path src/runtime/Cargo.toml -- src/tests/<category>/<name>/source.zbc --mode jit
```

或用分发版 binary 跑：

```bash
./artifacts/build/runtime/release/z42vm src/tests/<category>/<name>/source.zbc
```

## 只重生 zbc（不跑测试）

```bash
./xtask build test                 # 重生 golden（内部先 build stdlib + driver，缺则自建）
```

## 测试目录组织

```
src/tests/
├── basic/                # 基础语法 + 控制流 + 数组
├── operators/            # 算术 / 比较 / 逻辑 / 复合赋值
├── classes/              # OOP / interface / inheritance
├── generics/             # L3 泛型
├── closures/             # L3 闭包 / lambda
├── exceptions/           # try/catch/throw
├── native/               # native interop
└── ...
```

## 编写新 golden

```bash
# 1. 选好类别：src/tests/<category>/<name>/source.z42
# 2. 写 expected_output.txt（可选；空 = 用 Assert.* 自验证）
# 3. ./xtask build test —— 编译 source.zbc
# 4. ./xtask test e2e —— 验证
```

## stdlib-bound 测试 vs vm-core 测试

按归属规则：用到 stdlib 类的测试放 `src/libraries/<lib>/tests/`；纯 VM 行为 / 多包协作放 `src/tests/`。
