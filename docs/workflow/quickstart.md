# Quick Start

Get z42, build the dev CLI, then compile and run a program. Single source of truth
for first-time setup — the root [README](../../README.md) and this directory's
[README](README.md) both point here.

## 1. Get z42

z42's build/test/dev tooling is **itself written in z42** (the `xtask` CLI), so
you bootstrap by first downloading a prebuilt launcher — the one native primer
(chicken-and-egg: you need a working z42 to run the z42-implemented tooling):

```bash
git clone https://github.com/z42-lang/z42 && cd z42
./scripts/install-z42.sh                       # → ./.z42/  (z42 launcher + z42c + z42vm + stdlib)
                                               #   Windows: scripts\install-z42.bat
```

> `install-z42.sh` downloads the prebuilt package (version from
> `versions.toml [toolchain.z42].launcher`, default `nightly`) into a
> project-local, gitignored `./.z42/` — it never touches your system.

## 2. Build the xtask CLI, then drive everything through it

`z42 publish` compiles the project if needed and emits a native `./xtask` apphost.
The apphost stub ships with the **desktop workload**, not the base SDK, so install
it once first — match `--version` to your SDK (`nightly` if you took the default).
The produced `./xtask` auto-locates the `./.z42` runtime, so **no `PATH` export is
needed to run it**:

```bash
.z42/z42 workload install desktop --version nightly   # one-time: provides the apphost stub
.z42/z42 publish scripts/xtask.z42.toml               # build + deploy → ./xtask  (--rid defaults to host)

./xtask build all     # compiler + runtime + stdlib (from source)
./xtask test          # full gate (compiler + vm + cross-zpkg + stdlib)
./xtask -h            # all commands (build / test / deps / bench / package)
```

> **Don't want the workload?** Run the CLI straight through the launcher instead —
> no apphost needed (this is exactly how CI drives xtask):
> ```bash
> .z42/bin/z42c build scripts/xtask.z42.toml --release   # → artifacts/xtask/xtask.zpkg
> .z42/z42 artifacts/xtask/xtask.zpkg -- test            # run any xtask command
> ```

## 3. Compile + run a z42 program

The bare `z42` / `z42c` commands aren't on `PATH` yet. Either call them by path
(`.z42/z42c …`), or add the install dir once for convenience:

```bash
export PATH="$PWD/.z42:$PWD/.z42/bin:$PATH"  # optional: puts z42 / z42c / z42vm on PATH

z42c build path/to/app.z42.toml --release    # → <out_dir>/<name>.zpkg  (see examples/*.z42.toml)
z42 <out_dir>/<name>.zpkg                     # run it via the launcher
```

A green `./xtask test` already proves the toolchain compiles and runs z42
end-to-end. See [examples/](../../examples/) for project layouts.

> **Prerequisites:** git + Rust stable (`rustc --version`) + [`gh`](https://github.com/cli/cli) (authed — downloads the prebuilt primer). A C toolchain (`build-essential` / Xcode CLT) is needed for C-backed stdlib deps.
> **Building the whole toolchain from source** (no prebuilt download) and the full
> bootstrap details live in [building/](building/).
> Full build / test / packaging / CI / release workflows: [docs/workflow/](.).
> Collaboration workflow: [.claude/CLAUDE.md](../../.claude/CLAUDE.md).
