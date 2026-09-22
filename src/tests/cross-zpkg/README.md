# cross-zpkg/ — 多 zpkg 端到端测试

## 职责

验证多 zpkg 协作场景（target lib + ext lib + main app 三方编译 + VM 运行），
覆盖普通 golden test (`run/`) 无法表达的跨包路径。

L3-Impl2 (`impl Trait for Type` 跨 zpkg 传播) 是首个驱动用例。

## 测试目录约定

```
<test_name>/
├── target/                   # 提供 class / interface 的 lib
│   ├── z42.toml              # name + pack=true + [sources]
│   └── src/*.z42
├── ext/                      # 依赖 target，提供 impl 块
│   ├── z42.toml              # depends on target
│   └── src/*.z42
├── main/                     # 依赖 target + ext 的 exe
│   ├── z42.toml              # entry = "<Namespace>.<Func>" + pack=true
│   └── src/Main.z42
└── expected_output.txt       # main 运行后的预期 stdout
```

**负例 fixture（期望编译失败）**：放 `expected_build_error.txt` **代替** `expected_output.txt`，
内容 = `main` 构建 stderr 必须包含的子串（通常是诊断码 + 一句关键词）。判定：`main` **编过了**
→ 判红（说明该报的诊断没响）；编不过但错误文本对不上 → 也判红（否则「随便哪种编译失败都算过」
= 没有判别力的门）。这类 fixture 不进 run 波（没有产物可跑）。范例：`dup_fqn_crosspkg/`。

**告警 fixture（期望编过、且某条警告响了）**：加 `expected_build_warning.txt`，内容 = `main`
构建输出必须包含的子串。判定：编**不过** → 判红（回归）；编过但输出里没这条告警 → 也判红。
与 `expected_build_error.txt` 互斥（后者优先），且**照常进 run 波**（警告不阻断编译，有产物可跑），
所以这类 fixture 仍要写 `expected_output.txt`。范例：`deprecated_free_function/`。

> 加这条是因为此前**跨包警告类行为全仓无处设门**——只能断言「编不过」或断言 stdout，
> 而「该警告没响」两者都抓不到。跨包 `[Deprecated]` 自由函数静默失效数月无人发现，正是这个盲区。

**版本 skew fixture（编译时依赖 ≠ 运行时依赖）**：两个可选标记文件，都在 **run 波之前**
生效，且**必须同时改两处**——临时 `Z42_LIBS` 与 `main/<dist>`（packed exe build 会把依赖
zpkg colocate 进 main dist，而惰性加载器**先搜 entry zpkg 同目录**，只改 libs 那份等于没改）。

| 标记文件 | 语义 | 用来演示 |
|----------|------|---------|
| `skew-absent.txt` | 每行一个 zpkg 文件名，运行前**删掉** | 依赖包整个不在场（`available!()` 降级分支） |
| `skew-replace.txt` | 每行一个 zpkg 文件名，运行前用 `oldtarget/` 建出的同名产物**顶替** | 依赖包在场但**成员更少**——版本 skew 的常态 |

`oldtarget/` 是与 `target/` **同工程名**的旧版依赖工程（无 fixture 内依赖，独立构建），
**不参与 `main` 的编译**：main 仍按 `target/`（新版）编译，运行时却加载到 `oldtarget/`。
目录不存在 = 该 fixture 不用这一层，自动跳过。范例：`missing_ctor_skew/`（类型和字段都在、
唯独构造器没了 → `MissingSymbolException`）配对照组 `missing_ctor_present/`；
`wrong_ctor_arity_skew/` 用它造「构造器还在、但**签名变了**」（v2 的 `Widget()` 与 v1 的
`Widget(int)` 都是各自包里的唯一构造器 ⇒ 占用**同一个裸键**，解析得到、却是错的那个），
配对照组 `wrong_ctor_arity_present/`。

**z42.toml 必须**：

- `pack = true` — cross-zpkg 引用基于 packed 模式的 TSIG section（debug 默认 indexed 没有 TSIG）
- `[sources] include = ["src/**/*.z42"]` 或保持默认（默认就是这个 glob）
- main 的 `entry` 用 `<Namespace>.<Func>` 形式（不是文件路径）

## 运行

```bash
z42 xtask.zpkg test cross-zpkg              # interp 模式
z42 xtask.zpkg test cross-zpkg jit          # jit 模式
```

驱动逻辑（`z42 xtask.zpkg test cross-zpkg`）：

1. 构建 target → ext → main（每步把上一步的 zpkg 复制到下一步的 `libs/`）
2. 收集 stdlib + target + ext 的 zpkg 到临时 libs_dir
3. 用 `Z42_LIBS=<temp>` 启动 z42vm 跑 main 的 zpkg
4. 比对 stdout 与 `expected_output.txt`

## 现有测试

| 测试 | 覆盖 | 关键路径 |
|------|------|---------|
| `01_impl_propagation` | L3-Impl2 跨 zpkg `impl IGreet for Robot` | IMPL section 序列化 → Phase 3 merge → IrGen QualifyClassName → VM lazy loader |
| `dup_fqn_crosspkg` | **负例**：两个包同 FQN → E0601 | ImportedSymbolLoader 包名累积 → SymbolTable 判据 → 两个 choke point |
| `available_skew` / `available_present` | `available!()` 按**实际依赖图**折常量 + 剪分支 | 加载期常量折叠 → CFG 剪枝（`skew-absent.txt`） |
| `missing_ctor_skew` / `missing_ctor_present` | 构造器缺失不再静默写未构造对象 | ObjNew ctor 解析 → `symres::missing_ctor_exception`（`skew-replace.txt` + `oldtarget/`） |
| `wrong_ctor_arity_skew` / `wrong_ctor_arity_present` | 构造器**解析到了、签名却对不上**不再照常调用（裸键在 skew 下会命中错的构造器） | ObjNew ctor 解析后 → `symres::wrong_ctor_arity_exception`（`skew-replace.txt` + `oldtarget/`） |
| `call_arity_instance_skew` / `call_arity_sealed_skew` / `call_arity_present` | **实例方法**解析到了另一个签名（primary 裸键撞上）不再照常执行——普通类走 `VCall`、sealed 类走去虚化后的直接 `Call`；未修复时两者都输出 `label null7` | `VCall` → `resolve_vcall` 出口 + `install_ic`；`Call` → resolver 预填 / 冷路径写回 / cross-cell / JIT tier 3 → `symres::call_arity` + `wrong_arity_exception`（sret 由 `METHOD_FLAG_SRET` 精确计入） |
| `call_arity_static_skew` | **事实守卫**：常规静态方法签名变了 ⇒ 键（全签名 mangle）也变 ⇒ 走「缺符号」抛异常，天然不受裸键撞车影响 | `MemberCollector._fillClass` 静态分支 `MangleKey` → `undefined function` |
| `ctorless_objnew_skew` / `_present` / `_absent` | **零实参**的构造器缺失不再静默（关掉 `argc == 0` 那条缝）。`_absent` 是**过度收紧守卫**：真·零构造器跨包类不得误报 | 装配期 `CtorKnownFixup` 置 `ObjNew.ctor_known`（zbc 1.39） → `symres::missing_ctor_exception`（`skew-replace.txt` + `oldtarget/`）|
| `static_ctor_crosspkg_static_call` | 依赖包类型的**第一次使用是调静态方法**（含不碰静态字段的方法）时静态 ctor 照常执行，结果与调用顺序无关；未使用的类型不执行 | `LazyLoader::insert_type` 入表即登记 cctor → `ensure_callee_owner_init` / `ensure_static_owner_init` 屏障 |
| `static_ctor_crosspkg_field_first` | 守卫：依赖包类型的第一次使用是**直接读静态字段**时静态 ctor 照常执行（登记点从 `try_lookup_type` 挪到加载器入表处后不退化） | 静态字段名预解析 → 依赖包加载 → `LazyLoader::insert_type` 登记 → `ensure_static_owner_init` |
| `inherited_ctor_cross_pkg` | 构造器继承与隐式 `base()` 跨包 / 同包跨文件：主包类继承依赖包基类构造器（含默认值）、依赖包内继承并导出、依赖包基类只有初始化器、继承 stdlib `Exception` | `CtorInheritance`（收集期合成 ctor 符号 → TSIG 导出）→ `DeclBinder` 隐式 `base()` |
| `ctor_init_cross_pkg` | 零实参 `: base()` 指向**依赖包**基类时照常调用（修前丢调用）；依赖包里「静态 ctor + 无参实例 ctor」的类 `new` 时仍选中实例 ctor（守卫，修前亦对） | `DeclBinder._bindMethodBody`（`HasCtorInit` 门）→ `OverloadBinder._ctorKey`（排除静态 ctor） |
| `crosspkg_ctor_default` | **跨包构造器**省略可选实参 → 注入作者声明的默认值（此前整支缺失，读到零值） | `ConstructTyper._bindNew` → `OverloadBinder._crossPkgDefault`（`$Default` ConstBlob 解码） |
| `missing_type_skew` | `new` 一个解析不到的类型不再合成零字段空壳 | ObjNew 类型解析 → `symres::missing_type_exception`（`skew-absent.txt`） |
| `missing_base_skew` / `crosspkg_base_fields_main` | 基类解析不到不再静默退化成「只有自己的成员」 | 继承 fixup → `TypeDescCold::base_unmerged` → `symres::missing_base_exception`（`skew-absent.txt`） |
| `module_init_free_function` | 依赖包的 `[ModuleInit]` 在**只调它的自由函数**时也跑 —— 🔴 这条是 add-module-init-hook 的判别力核心：cctor 屏障推不出自由函数的 owner 类型，纯惰性方案在这条路上永远不跑初始化器 | `LazyLoader::insert_type`（登记）→ `VmContext::ensure_module_inits`（屏障执行） |
| `module_init_on_free_function` | `[ModuleInit]` 标在**顶层自由函数**上（豁免 `static`）端到端真的会跑 —— 编译期放行是一回事，合成的 `$Module.$cctor` 按自由函数发射名（`_q(RegKey)`）去调它是另一回事 | `ModuleInitSynth._targetIrName` 的自由函数分支 |
| `module_init_once` | 先于本包代码；三种触达形态（静态方法 / 自由函数 / 静态字段）各来一次，初始化器仍**只跑一次** | 同上 + `CctorRegistry::claim` |
| `module_init_load_order` | 跨包初始化顺序 = **实际加载顺序**（先触达 B 则 B 先跑），与清单声明顺序无关 | 同上 |
| `module_init_bad_target` | **负例**：`[ModuleInit]` 标在实例方法上 → E0486 | `ModuleInitScan.CheckPackage`（`expected_build_error.txt`） |
| `module_init_duplicate` | **负例**：一个包里两个文件各一个 `[ModuleInit]` → E0485（**跨 CU** 判定，per-file 阶段看不见） | 同上 |
| `ctor_visibility_cross_pkg` | **负例**：跨包调用 `internal` 构造器 → E0404；public 构造器与主构造器放行 | `ConstructTyper._bindNew` → `AccessChecker.CheckAccess`（`expected_build_error.txt`） |
