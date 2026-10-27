# The `simsapa` CLI (`backend_cli/`) and the `@simsapa/cli` npm package

## Release builds per target

One binary per npm platform package, each written by its `Makefile` target to
`dist/cli/<npm-platform>/`, where the npm packing script picks it up:

| `make` target | Rust target | Built on | Output |
|---|---|---|---|
| `cli-linux-x64` | `x86_64-unknown-linux-musl` | Linux (cargo-zigbuild) | `dist/cli/linux-x64/simsapa` |
| `cli-linux-arm64` | `aarch64-unknown-linux-musl` | Linux (cargo-zigbuild) | `dist/cli/linux-arm64/simsapa` |
| `cli-darwin-arm64` | `aarch64-apple-darwin` | an Apple Silicon Mac | `dist/cli/darwin-arm64/simsapa` |
| `cli-win32-x64` | `x86_64-pc-windows-msvc` | Windows, MSVC build tools | `dist/cli/win32-x64/simsapa.exe` |

`make cli` (debug) and `make cli-release` (host target) build into
`backend_cli/target/` and copy nothing.

The release profile (`strip = true`, `lto = "thin"`, `codegen-units = 1`) is in
`backend_cli/Cargo.toml`. Cargo reads `[profile.*]` only from the crate being
built as the root, so it never applies to the GUI's `bridges` build or to the
backend's tests.

### Linux: static musl through cargo-zigbuild

The Linux binaries are fully static (`file` reports "statically linked", `ldd`
"not a dynamic executable"), so they have no glibc version floor and run on
Alpine and other musl-based containers as well as on glibc distributions.

The only C code in the dependency tree that needs a cross compiler is the
bundled SQLite (`libsqlite3-sys`) and `mimalloc`; `bzip2` resolves to the
pure-Rust `libbz2-rs-sys`. The C cross compiler is **zig**, driven by
[cargo-zigbuild](https://github.com/rust-cross/cargo-zigbuild), chosen over
`cross` because it needs no Docker and one zig install covers both
architectures. One-time setup on the build machine:

``` sh
pacman -S zig                       # or the distribution's zig package
cargo install cargo-zigbuild --locked
rustup target add x86_64-unknown-linux-musl aarch64-unknown-linux-musl
```

Each target takes about 4 minutes from clean and about 1 minute after a change
to the CLI crate only (the thin LTO link dominates).

To smoke-test the arm64 binary on an x64 machine, run it through
`qemu-aarch64-static` (from `qemu-user-static`).

**The musl builds use `mimalloc` as the global allocator**
(`#[cfg(target_env = "musl")]` in `backend_cli/src/main.rs`, dependency under
`[target.'cfg(target_env = "musl")'.dependencies]`). musl's own allocator is
the slower one under `simsapa serve`'s request load. The glibc, macOS and
Windows builds keep the system allocator.

> **Trap: a musl binary is slower to *start*, not to *search*.** Every one-shot
> command (`search`, `word`, `query`, `fulltext-search`) costs about 100 ms more
> wall time on musl than on glibc, and `mimalloc` does not change that, so the
> allocator is not the cause; the cause has not been isolated. Search requests
> to a running `simsapa serve` are as fast as on glibc. Do not switch the Linux
> builds to glibc to "fix search speed": an agent making many queries should use
> `serve` anyway (the per-invocation cost of opening the databases dominates).
>
> If a glibc build is ever needed, cargo-zigbuild produces one against an old
> glibc without a container: `cargo zigbuild --release --target
> x86_64-unknown-linux-gnu.2.28`. It is dynamically linked, needs glibc ≥ 2.28,
> and performs like a native glibc build.

### What the binary contains

Stripped sizes at `1.1.0-alpha.3`:

| Target | Size |
|---|---|
| linux-x64 (musl) | 25.3 MiB |
| linux-arm64 (musl) | 22.8 MiB |
| darwin-arm64 | not yet measured |
| win32-x64 | not yet measured |

The CLI embeds only what `backend` compiles in with `include_str!` /
`include_bytes!` (HTML templates, the inline reader CSS, the JSON/TOML data
files). Two things it must **not** contain:

- **The `assets/` folder.** The `include_dir!` of `assets/` lives in `bridges`
  only. Check: `grep -ac pdf.worker dist/cli/linux-x64/simsapa` prints `0`.
- **The DOCX-export fonts.** `docx_export.rs` `include_bytes!`s six TTF files,
  but only `bridges` calls it, so the linker drops them from the CLI. **Do not
  check this by grepping for the font names**: `Crimson Pro` and `Abhaya Libre`
  also occur as CSS `font-family` names in the inline reader CSS, so a name grep
  is never 0. Check for the font *data* instead — a 64-byte slice from the
  middle of each TTF must not occur in the binary:

  ``` sh
  python3 -I -c "
  import glob
  b = open('dist/cli/linux-x64/simsapa', 'rb').read()
  for f in glob.glob('assets/fonts/*/*.ttf'):
      d = open(f, 'rb').read()
      if d[len(d)//2:len(d)//2+64] in b: print('EMBEDDED', f)"
  ```

  No output means no font is embedded.

The linux-x64 binary links no `libasound` (the backend's `audio` feature is
off); being static, it links nothing at all.

## Output on stdout

Every stdout write goes through `output::write_stdout`, via the `out!` /
`outln!` macros. **Never use `print!` / `println!` in `backend_cli/src/`**
(a unit test rejects them): they panic when the write fails, and a reader that
closes the pipe early (`simsapa search … | head`) is routine for an agent.

After the first failed write, all further output is dropped and the command
runs to the end. A closed pipe keeps the command's own exit code; any other
write failure (a full disk behind `> out.json`) prints an error on stderr and
the command exits 1.

> **Trap: do not "fix" this by restoring the default `SIGPIPE` action.** It
> would apply to every socket in the process as well, and it would kill an
> `import`, `index` or `setup` mid-write when its stdout went away. Finishing
> the work matters more than the output.
