# z42

A **full-stack systems programming language** designed for productivity and performance.

- **z** — the last letter, standing for the final evolution
- **42** — the answer to the ultimate question

> 🚧 **z42 is under active, relentless iteration.** The language, compiler, VM, and toolchain are evolving rapidly and not yet stable for production use. What's landing here is being built with uncompromising taste — expect the final result to be **genuinely stunning**. Star the repo and watch it unfold.

---

## Why z42?

z42 combines C#'s productivity, Rust's runtime discipline, and Python's iteration speed —
one language that scales from throwaway scripts to embedded systems components:

| | z42 |
|---|-----|
| **Productive by default** | C#-style syntax, static typing + inference, automatic GC — no ownership annotations, errors caught at compile time |
| **Optimal on every axis** | Memory, CPU, and startup each pushed toward optimal: compact bytecode and object layout, generational low-pause GC, cache-friendly interpreter dispatch, JIT hot paths competitive with C#/Java |
| **Pick your execution mode** | One bytecode, three modes — interpret (instant startup, small footprint), JIT (peak throughput), AOT (stable latency) — mixed per namespace |
| **Native & embedding first-class** | Embeddable Rust VM, zero-overhead `extern` FFI (≤ 1 indirect jump), C-compatible structs |
| **Iterate without restarting** | Hot code reload across execution modes: functions and types are GC-managed runtime objects, so replaced definitions unload naturally; `eval()` for scripting |
| **Concurrent** | GC-safe multithreading; structured async/await planned |
| **Fits your project** | Per-project language customization — forbid features (e.g. nullable types), tighten rules (e.g. exhaustive matches) |
| **Friendly to AI collaborators** | A familiar syntax models already know, compile-time errors as fast agent feedback, and a docs-as-code repo built for human + AI development |

Performance goal: production-fast (game engines, servers, embedded) **without unsafe** — see
[`docs/design/philosophy.md`](docs/design/philosophy.md) for concrete targets and trade-offs.

---

## Quick Start

Download the launcher, build the `xtask` dev CLI, then compile and run a program —
full steps in **[docs/workflow/quickstart.md](docs/workflow/quickstart.md)**:

```bash
git clone https://github.com/z42-lang/z42 && cd z42
./scripts/install-z42.sh                              # → ./.z42/  (launcher + z42c + z42vm + stdlib)
.z42/z42 workload install desktop --version nightly    # apphost stub (ships with the desktop workload)
.z42/z42 publish scripts/xtask.z42.toml                # build + deploy → ./xtask (native apphost)
./xtask test                                          # ./xtask auto-locates ./.z42 — no PATH export
```

> No desktop workload? Drive the CLI through the launcher instead (what CI does):
> `.z42/bin/z42c build scripts/xtask.z42.toml --release && .z42/z42 artifacts/xtask/xtask.zpkg -- test`

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
