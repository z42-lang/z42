# <img src="docs/logo/z42-icon.svg" width="32" align="absmiddle" alt="z42 logo"> z42

A **full-stack systems programming language** designed for productivity and performance.

- **z** — the last letter, standing for the final evolution
- **42** — the answer to the ultimate question

> 🚧 **z42 is under active development.** The language, compiler, VM, and toolchain are evolving rapidly and are not yet stable for production use. Star the repository to follow progress.

---

## Why z42?

z42 is designed to combine productive application development with an embeddable,
performance-oriented runtime.

| What matters | z42's approach |
|---|---|
| **Development experience** | Familiar C#-style syntax, local type inference, and integrated build, test, and REPL tools support everyday development. |
| **Execution performance** | A common bytecode format supports interpretation and JIT compilation; value types and escape-analysis stack allocation reduce heap allocation where applicable. |
| **Program reliability** | Static type checking catches type errors before execution, garbage collection manages object lifetimes, and exceptions provide structured error handling. |
| **Native integration** | A C API lets native applications load and invoke z42 code, while explicit native bindings let z42 programs use host functionality. |
| **Code organization** | Namespaces and packages organize larger programs, while generics and interfaces support reusable libraries. |

z42 is under active development. AOT compilation, structured async/await, and hot reload
are planned. See the [design philosophy](docs/internals/src/philosophy.md) for design goals
and trade-offs, and the [roadmap](docs/roadmap.md) for planned work.

---

## Quick Start

**Use z42** (macOS arm64 / Linux / Windows x64) — installs the latest nightly into `~/.z42`:

```bash
curl -fsSL https://z42-lang.github.io/z42/install.sh | sh          # Windows: irm https://z42-lang.github.io/z42/install.ps1 | iex
z42 new hello && cd hello && z42 run
```

Continue with the [Learn book](https://z42-lang.github.io/z42/learn/) for a guided introduction
to projects and the language.

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

---

## Documentation

Three books, one site. Which one you want depends on what you are doing —
see [`docs/README.md`](docs/README.md) for the full split.

| Book | For | Online |
|---|---|---|
| [`docs/learn/`](docs/learn) | Writing z42 programs — read in order, install → first project → language | <https://z42-lang.github.io/z42/learn/> |
| [`docs/reference/`](docs/reference) | Looking things up — language rules, stdlib API, CLI, `z42.toml` fields, error codes | <https://z42-lang.github.io/z42/reference/> |
| [`docs/internals/`](docs/internals) | Changing z42 itself — architecture, mechanisms, decisions, build/test/release | <https://z42-lang.github.io/z42/internals/> |

Beyond the books: [`docs/roadmap.md`](docs/roadmap.md) (plan + deferred index),
[`docs/agent/`](docs/agent) (collaboration rules for AI + human contributors).

---

## Repository Structure

```
z42/
├── src/
│   ├── compiler/          # z42 self-hosting compiler (.z42 source → zpkg)
│   ├── runtime/           # Rust VM (interp / JIT)
│   ├── libraries/         # Standard library (.z42 source)
│   ├── tests/             # End-to-end golden tests
│   ├── benches/           # Whole-program performance scenarios
│   └── toolchain/         # Launcher (z42), builder (z42b), REPL, workloads, devtools
├── scripts/               # xtask dev CLI (build / test / package) + install primers
├── .github/               # CI workflows and composite actions
├── docs/                  # learn/ + reference/ + internals/ books, book/ site root,
│                          # roadmap.md, agent/ rules
├── examples/              # Companion projects for the learn book, run by `xtask test docs examples`
└── AGENTS.md              # AI entry (cross-tool standard): thin pointer to docs/agent/
```

---

## License

z42 is released under the [MIT License](LICENSE).
