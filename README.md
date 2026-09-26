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
| **Fast on every axis** | Compact bytecode and objects, low-pause generational GC, escape-analysis stack allocation, Cranelift JIT |
| **Two execution modes today** | Interpretation (instant startup) and JIT (peak throughput) from one bytecode, selectable per namespace; AOT is designed but not implemented |
| **Native-first** | Embeddable Rust VM, zero-overhead `extern` FFI, C-compatible structs |
| **Concurrent** | GC-safe multithreading; structured async/await planned |
| **Customizable** | Per-project language rules — turn language constructs off in `z42.toml` (`[syntax]`), and using one reports `E0301` |
| **AI-friendly** | Familiar syntax, compile-time errors as agent feedback, docs-as-code repository |

Planned but not yet implemented: AOT, hot patching (functions/types/modules as GC-managed objects),
and async/await. Performance targets and trade-offs are in
[`docs/internals/src/philosophy.md`](docs/internals/src/philosophy.md); per-feature status is in
[`docs/internals/src/features.md`](docs/internals/src/features.md).

---

## Quick Start

**Use z42** (macOS arm64 / Linux / Windows x64) — installs the latest nightly into `~/.z42`:

```bash
curl -fsSL https://z42-lang.github.io/z42/install.sh | sh          # Windows: irm https://z42-lang.github.io/z42/install.ps1 | iex
z42 new hello && cd hello && z42 run
```

**Work on z42 itself** — bootstrap a repo-local SDK, build the `xtask` dev CLI, run the gate
(full steps, per-platform notes and escape hatches in
**[docs/internals/src/devinfra/dev-setup.md](docs/internals/src/devinfra/dev-setup.md)**):

```bash
git clone https://github.com/z42-lang/z42 && cd z42
./scripts/install-z42.sh                     # → ./.z42/  (launcher + z42c + z42vm + stdlib)
.z42/z42 publish scripts/xtask.z42.toml      # build + deploy → ./xtask
./xtask build all                            # compiler + VM + stdlib, all from source
./xtask test                                 # full GREEN gate; ./xtask auto-locates ./.z42
```

**Editor support (VSCode)**: `./xtask deps install vscode` installs `.z42` syntax highlighting
as a repo-local workspace extension — reload the window and accept the prompt.

---

## Documentation

Three books, one site. Which one you want depends on what you are doing —
see [`docs/README.md`](docs/README.md) for the full split.

| Book | For | Online |
|---|---|---|
| [`docs/learn/`](docs/learn/) | Writing z42 programs — read in order, install → first project → language | <https://z42-lang.github.io/z42/learn/> |
| [`docs/reference/`](docs/reference/) | Looking things up — language rules, stdlib API, CLI, `z42.toml` fields, error codes | <https://z42-lang.github.io/z42/reference/> |
| [`docs/internals/`](docs/internals/) | Changing z42 itself — architecture, mechanisms, decisions, build/test/release | <https://z42-lang.github.io/z42/internals/> |

Beyond the books: [`docs/roadmap.md`](docs/roadmap.md) (plan + deferred index),
[`docs/agent/`](docs/agent/) (collaboration rules for AI + human contributors),
[`docs/spec/`](docs/spec/) (per-change work area: in-flight `changes/` + `archive/`).

---

## Repository Layout

```
z42/
├── src/
│   ├── compiler/          # z42 self-hosting compiler (.z42 source → zpkg)
│   ├── runtime/           # Rust VM (interp / JIT)
│   ├── libraries/         # Standard library + compiler front-end libs (.z42 source)
│   └── toolchain/         # Launcher (z42), builder (z42b), REPL, workloads, devtools
├── scripts/               # xtask dev CLI (build / test / package) + install primers
├── docs/                  # learn/ + reference/ + internals/ books, book/ site root,
│                          # roadmap.md, agent/ rules, spec/ change records
├── examples/              # Companion projects for the learn book, run by `xtask test examples`
└── .claude/               # Claude Code entry (workflow rules)
```

---

## License

z42 is released under the [MIT License](LICENSE).
