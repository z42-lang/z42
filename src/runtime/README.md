# z42 Runtime — Rust VM

## 职责

执行 z42 编译产物（`.zbc`/`.zpkg`）。当前实现解释器（interp）；JIT 和 AOT 为桩，待 interp 全绿后填充。

## 目录结构与核心文件

### 顶层
| 文件 | 职责 |
|------|------|
| `src/main.rs` | CLI 入口，加载产物并交给 `Vm` 执行 |
| `src/vm.rs` | `Vm` 结构体：持有 `Module`，按 `ExecMode` 分发到 interp/jit/aot |
| `src/lib.rs` | 库入口，re-export 公开 API |
| `src/aot.rs` | AOT 后端桩（未实现） |

### src/metadata/ — IR 元数据与加载层
| 文件 | 职责 |
|------|------|
| `types.rs` | 运行时值类型：`Value`、`ExecMode`；对象模型：`ScriptObject`、`TypeDesc`、`NativeData`、`FieldSlot` |
| `bytecode.rs` | IR 数据结构：`Module`、`Function`、`Instruction`、`Terminator` |
| `formats.rs` | `.zbc`/`.zpkg` 磁盘格式数据结构（镜像 C# `PackageTypes.cs`） |
| `loader.rs` | 统一加载入口：`load_artifact(path)` → `Module`；`build_type_registry` 预构建 `TypeDesc` 注册表 |
| `merge.rs` | 多模块合并：字符串池重映射 + 函数拼接 |
| `project.rs` | 项目清单类型（`.z42.toml` Rust 侧类型） |

### src/interp/ — 字节码解释器（当前唯一可用后端）
| 文件 | 职责 |
|------|------|
| `mod.rs` | 公开 API、`Frame`、核心执行循环；用户异常（`PENDING_EXCEPTION`）；静态字段（`STATIC_FIELDS`） |
| `ops.rs` | 寄存器级辅助：`int_binop`、`numeric_lt`、`collect_args` 等 |

### src/corelib/ — 内置函数实现
统一入口 `exec_builtin(name, args)` 供解释器和 JIT 调用（对应 CoreCLR `classlibnative/`）。

| 文件 | 职责 |
|------|------|
| `convert.rs` | `value_to_str`、`require_str/usize`、parse/to_str |
| `io.rs` | `println`、`print`、`readline`、`concat`、`len` |
| `string.rs` | `str_length`（`__str_length`）、`str_substring`、`str_split`、`str_join`、`str_format` 等 |
| `math.rs` | `abs`、`max`、`min`、`pow`、`sqrt`、三角函数等 |
| `collections.rs` | `list_*` / `dict_*` 集合操作 |
| `fs.rs` | `file_*` / `path_*` / `env_*` / `process_exit` / `time_now_ms` |
| `string_builder.rs` | `sb_new`/`sb_append`/`sb_append_line`/`sb_to_string`；`NativeData::StringBuilder` 作为后端存储 |
| `object.rs` | `obj_get_type`、`obj_ref_eq`、`obj_hash_code`、`assert_*` |

### 桩模块（未实现）
| 目录 | 说明 |
|------|------|
| `src/jit/` | JIT 后端，interp 全绿后填充 |
| `src/gc/` | 垃圾回收，Phase 1 用 Rust `Rc` 管理生命周期 |
| `src/exception/` | 结构化异常，当前通过 `thread_local PENDING_EXCEPTION` 临时处理 |
| `src/thread/` | 多线程，Phase 1 单线程执行 |

### crates/ — Native interop Rust crates（C1 落地）
本目录是 z42 native interop 三层 ABI 的 Rust 侧公开接口；详见 [`crates/README.md`](crates/README.md)。

| 子 crate | 职责 | 状态 |
|---------|------|------|
| `crates/z42-abi/` | Tier 1 C ABI 的 Rust `#[repr(C)]` 镜像（`no_std`，无依赖） | ✅ C1 接口锁定 |
| `crates/z42-rs/` | Tier 2 用户面向 trait/type（`Z42Type`、`Z42Traceable`、`Visitor`） | ✅ 骨架 |
| `crates/z42-macros/` | proc macro 入口（`Z42Type` derive、`methods`/`trait_impl`、`module!`） | 🟡 入口已注册，展开报 `compile_error!` 指向 C3 |

C 头文件位于 [`include/z42_abi.h`](include/z42_abi.h)；`.z42abi` manifest schema 在 [`docs/design/compiler/manifest-schema.json`](../../docs/design/compiler/manifest-schema.json)。

## 构建与测试

```bash
cargo build --workspace --manifest-path src/runtime/Cargo.toml
cargo test  --workspace --manifest-path src/runtime/Cargo.toml
z42 xtask.zpkg test vm
```

### Cargo features

`default = ["jit", "native-interop", "mimalloc-alloc"]`（桌面）。平台预设经
`--no-default-features --features <wasm|ios|android>` 裁剪。

| feature | 作用 |
|---------|------|
| `jit` | Cranelift JIT 后端（desktop x64/arm64） |
| `native-interop` | Tier 1 原生扩展 ABI（dlopen + libffi） |
| `mimalloc-alloc` | z42vm 二进制的 `#[global_allocator]` 走 mimalloc。z42c 自编译**分配受限**（profile：系统 malloc ~31%、`--mode jit`≈`interp`），换 mimalloc 后 z42c 编译 −40%、字符串重负载 ~3×。仅二进制生效（嵌入 lib 用宿主分配器）；wasm/移动预设不含（C 构建不入 wasm 沙箱 / 移动体积敏感） |
