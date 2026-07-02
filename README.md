# z42

A **full-stack systems programming language** designed for productivity and performance.

- **z** — the last letter, standing for the final evolution
- **42** — the answer to the ultimate question

> 🚧 **z42 is under active, relentless iteration.** The language, compiler, VM, and toolchain are evolving rapidly and not yet stable for production use. What's landing here is being built with uncompromising taste — expect the final result to be **genuinely stunning**. Star the repo and watch it unfold.

---

## Why z42?

| Problem | Solution |
|---------|----------|
| C# is bloated, Rust has ownership friction | Clean C# syntax, automatic GC, no lifetimes |
| Need systems code + scripts in one language | Bytecode runs directly (no VM overhead) or JIT/AOT compiles for speed |
| Embedding + native interop is painful | Zero-overhead `extern` FFI, C-compatible structs |
| Can't iterate on code without restart | Hot reload + eval() support (no restart needed) |
| One-size-fits-all language doesn't fit all | Per-project language customization (forbid features as needed) |

---

## Core Features

- **Execution modes:** Interpreter (fast startup), JIT (peak perf), AOT (stable latency) — mix per namespace
- **Bytecode-first:** Source → bytecode → execute/compile (not source → machine code)
- **Zero-overhead FFI:** `extern` methods call Rust impl directly (≤ 1 indirect jump)
- **Hot reload:** Update code without restarting (functions only, interpreter mode)
- **Multi-threaded:** GC-safe concurrency, structured async/await (L3)
- **Customizable:** Disable features per project (e.g., forbid nullable types, require exhaustive matches)
- **Type-safe:** Static typing + type inference; errors caught at compile time

---

## Quick Start

Download the launcher, build the `xtask` dev CLI, then compile and run a program —
full steps in **[docs/workflow/quickstart.md](docs/workflow/quickstart.md)**:

```bash
git clone https://github.com/codesigner-ui/z42 && cd z42
./scripts/install-z42.sh                     # → ./.z42/  (launcher + z42c + z42vm + stdlib)
.z42/z42 publish scripts/xtask.z42.toml       # build + deploy → ./xtask (native apphost)
./xtask test                                 # full gate; ./xtask auto-locates ./.z42, no PATH export
```

> **Prerequisites:** git + Rust stable + [`gh`](https://github.com/cli/cli) (authed).
> Build from source, packaging, CI, release: [docs/workflow/](docs/workflow/).
> Collaboration workflow: [.claude/CLAUDE.md](.claude/CLAUDE.md).

---

## Documentation

Start here based on what you want to know:

| I want to... | Read this |
|--------------|-----------|
| **Understand z42's design** | [`docs/design/philosophy.md`](docs/design/philosophy.md) |
| **See language syntax** | [`docs/design/language/language-overview.md`](docs/design/language/language-overview.md) |
| **Learn feature specs** | [`docs/features.md`](docs/features.md) |
| **Understand bytecode/IR** | [`docs/design/runtime/ir.md`](docs/design/runtime/ir.md) |
| **Understand execution modes** | [`docs/design/runtime/execution-model.md`](docs/design/runtime/execution-model.md) |
| **Learn native interop** | [`docs/design/language/interop.md`](docs/design/language/interop.md) |
| **Understand hot reload** | [`docs/design/runtime/hot-reload.md`](docs/design/runtime/hot-reload.md) |
| **See implementation progress** | [`docs/roadmap.md`](docs/roadmap.md) |

---

## Repository Layout

```
z42/
├── src/
│   ├── compiler/          # z42 self-hosting compiler (.z42 source → zpkg)
│   ├── runtime/           # Rust VM (interp / JIT / AOT)
│   ├── libraries/         # Standard library (.z42 source)
│   └── toolchain/         # Companion toolchain (launcher / test-runner / builder / debugger / workload)
├── scripts/               # xtask dev CLI (build / test / package) + install primers
├── docs/design/           # Language design documents
├── examples/              # Example programs
└── .claude/               # Collaboration docs (CLAUDE.md, workflow rules)
```

---

## Implementation Status

| Phase | Focus | Status |
|-------|-------|--------|
| **L1** | Core language + pipeline | ✅ Complete |
| **L2** | Ecosystem, stdlib, VM quality | 🚧 In Progress |
| **L3** | Generics, async, ADTs, Traits | 📋 Planned |

See [docs/roadmap.md](docs/roadmap.md) for detailed milestones.

---

## License

z42 is released under the [MIT License](LICENSE).
