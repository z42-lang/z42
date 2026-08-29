<p align="center">
  <img src="docs/assets/logo/svg/z42-icon.svg" width="112" alt="z42 logo">
</p>

# z42

A **full-stack systems programming language** designed for productivity and performance.

- **z** — the last letter, standing for the final evolution
- **42** — the answer to the ultimate question

> 🚧 **z42 is under active development.** The language, compiler, VM, and toolchain are evolving rapidly and are not yet stable for production use. Star the repository to follow progress.

---

## Why z42?

z42 combines C#'s productivity, Rust's runtime discipline, and Python's iteration speed —
a single language spanning ad-hoc scripts to embedded systems components:

| | z42 |
|---|-----|
| **Productive** | C#-style syntax, static typing with inference, automatic GC — no ownership annotations |
| **Fast on every axis** | Memory, CPU, and startup all optimized: compact bytecode and objects, low-pause generational GC, JIT competitive with C#/Java |
| **Three execution modes** | Interpretation (instant startup), JIT (peak throughput), AOT (stable latency) — one bytecode, selectable per namespace |
| **Native-first** | Embeddable Rust VM, zero-overhead `extern` FFI, C-compatible structs |
| **Hot patching** | Functions, types, and modules are GC-managed objects — patch at any granularity, superseded definitions unload automatically; `eval()` for scripting |
| **Concurrent** | GC-safe multithreading; structured async/await planned |
| **Customizable** | Per-project language rules — forbid features, require exhaustive matches |
| **AI-friendly** | Familiar syntax, compile-time errors as agent feedback, docs-as-code repository |

Performance goal: fast enough for production systems **without unsafe code** — targets and
trade-offs in [`docs/design/philosophy.md`](docs/design/philosophy.md).

---

## Quick Start

Download the launcher, build the `xtask` dev CLI, then compile and run a program —
full steps in **[docs/workflow/quickstart.md](docs/workflow/quickstart.md)**:

```bash
git clone https://github.com/z42-lang/z42 && cd z42
./scripts/install-z42.sh                     # → ./.z42/  (launcher + z42c + z42vm + stdlib)
.z42/z42 publish scripts/xtask.z42.toml      # build + deploy → ./xtask
./xtask test                                 # ./xtask auto-locates ./.z42
```

**Editor support (VSCode)**: `./xtask deps install vscode` installs `.z42` syntax highlighting
as a repo-local workspace extension — reload the window and accept the prompt.

---

## Documentation

Start from what you want to do. The knowledge base is consolidating into
[`docs/book/`](docs/book/) (mdBook); topic links below move there as chapters land.

**Using z42** — language & runtime:

| I want to... | Read this |
|--------------|-----------|
| **Understand the design philosophy** | [`docs/design/philosophy.md`](docs/design/philosophy.md) |
| **Learn the language** (syntax, types, semantics) | [`docs/design/language/language-overview.md`](docs/design/language/language-overview.md) |
| **Understand execution** (interp / JIT / AOT) | [`docs/design/runtime/execution-model.md`](docs/design/runtime/execution-model.md) |
| **Call native code / embed the VM** | [`docs/design/language/interop.md`](docs/design/language/interop.md) |

**Working on z42** — building & contributing:

| I want to... | Read this |
|--------------|-----------|
| **Build, test, and package the repo** | [`docs/workflow/`](docs/workflow/) |
| **Follow the collaboration workflow** | [`docs/agent/`](docs/agent/) |
| **See progress and what's planned** | [`docs/roadmap.md`](docs/roadmap.md) · [`docs/features.md`](docs/features.md) |

---

## Repository Layout

```
z42/
├── src/
│   ├── compiler/          # z42 self-hosting compiler (.z42 source → zpkg)
│   ├── runtime/           # Rust VM (interp / JIT / AOT)
│   ├── libraries/         # Standard library (.z42 source)
│   └── toolchain/         # Launcher, test runner, workloads
├── scripts/               # xtask dev CLI (build / test / package) + install primers
├── docs/
│   ├── book/              # Knowledge base (mdBook): language / compiler / runtime / stdlib
│   ├── design/            # Design documents (migrating into book/)
│   ├── workflow/          # Build / test / CI / release commands
│   └── agent/             # Collaboration rules for AI + human contributors
├── examples/              # Example programs
└── .claude/               # Claude Code entry (workflow rules)
```

---

## License

z42 is released under the [MIT License](LICENSE).
