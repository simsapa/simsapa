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

## Output on stdout and stderr

**Never use `print!`, `println!`, `eprint!` or `eprintln!` outside test code
in `backend_cli/`, `api/` or `backend/`** — everything the `simsapa` binary
runs. They panic when the write fails (exit 101), and both cases are routine
for an agent: a reader that closes the pipe early (`simsapa search … | head`),
or a caller that closes stderr.

`test_no_print_macros_in_sources` in `backend_cli/src/output.rs` enforces this
over all three crates, skipping comment lines and `#[cfg(test)] mod … { … }`
blocks; it names each offending file and line. Fix the call site rather than
adding an exception. The rule and its replacements are also in `AGENTS.md`,
"No `println!` / `eprintln!` in code the `simsapa` CLI runs".

- **stdout** goes through `output::write_stdout`, via the CLI's `out!` /
  `outln!` macros. After the first failed write, all further output is
  dropped and the command runs to the end. A closed pipe keeps the command's
  own exit code; any other write failure (a full disk behind `> out.json`)
  prints an error on stderr and the command exits 1.
- **stderr** goes through `simsapa_backend::logger::write_stderr`, via the
  backend's `errln!` macro, which drops a failed write. The backend uses it for
  its own stderr messages (the logger's fallbacks, the pre-logger messages in
  `lib.rs`), because the CLI reaches that code too. The backend's few stdout
  messages (pre-logger, in `get_create_simsapa_dir()`) use
  `logger::write_stdout`, which likewise drops a failed write.
- The CLI's `tracing` subscriber is built with `.log_internal_errors(false)`:
  tracing-subscriber reports a failed write with an `eprintln!` of its own,
  which panics on a closed stderr. indicatif, dialoguer and console already
  ignore write errors, and Rocket's logging is off. **A new dependency that
  prints can bring the panic back with no macro in our code** — check what it
  does on a write error before adding it.

> **Trap: do not "fix" this by restoring the default `SIGPIPE` action.** It
> would apply to every socket in the process as well, and it would kill an
> `import`, `index` or `setup` mid-write when its stdout went away. Finishing
> the work matters more than the output.

## The npm packages

`@simsapa/cli` uses the per-platform optional-dependency layout (as esbuild,
biome and turbo do). Sources are in `npm/`, one folder per package:

| Folder | Package | Contents |
|---|---|---|
| `npm/cli/` | `@simsapa/cli` | `bin/simsapa.js` (the launcher), `README.md` (the npmjs.com page) |
| `npm/cli-linux-x64/` | `@simsapa/cli-linux-x64` | `bin/simsapa`, `os: ["linux"]`, `cpu: ["x64"]` |
| `npm/cli-linux-arm64/` | `@simsapa/cli-linux-arm64` | `bin/simsapa`, `os: ["linux"]`, `cpu: ["arm64"]` |
| `npm/cli-darwin-arm64/` | `@simsapa/cli-darwin-arm64` | `bin/simsapa`, `os: ["darwin"]`, `cpu: ["arm64"]` |
| `npm/cli-win32-x64/` | `@simsapa/cli-win32-x64` | `bin/simsapa.exe`, `os: ["win32"]`, `cpu: ["x64", "arm64"]` |

The main package's `optionalDependencies` pin all four at its own exact
version, and npm installs only the one whose `os`/`cpu` match. There is no
`postinstall` script and no download at install time. The Linux binaries are
static musl builds, so the Linux packages carry no `libc` field and install on
glibc and musl systems alike.

`cli-win32-x64` lists `arm64` in `cpu` on purpose: on Windows on ARM, Node is
usually the native arm64 build, so without it npm would skip the package. The
x64 binary runs there under the system's x64 emulation, and the launcher maps
`win32 arm64` to it.

### The launcher

`npm/cli/bin/simsapa.js` is dependency-free. It maps `process.platform` /
`process.arch` to the platform package, `require.resolve`s
`<package>/bin/simsapa[.exe]`, and runs it with `spawnSync(…, { stdio:
"inherit" })`:

- The binary's exit status is the launcher's exit status. A binary killed by a
  signal has the launcher re-raise that signal on itself, so the caller sees
  the same death (SIGKILL → shell status 137).
- **The launcher installs a no-op `SIGINT` listener for the duration of the
  run.** Ctrl-C signals the whole foreground process group, so the binary gets
  it directly. Without a listener Node would exit at once and return the shell
  prompt while the binary is still cleaning up (`setup` deletes its partial
  download), and the shell would see Node's 130 instead of the binary's own
  status. With it, a Ctrl-C'd `serve` shuts down and the launcher reports its
  exit 0.
- An unsupported platform prints the supported list. A supported platform
  whose package is missing (installed with `--omit=optional`) also says to
  reinstall without that flag.

`npm/cli/bin/simsapa.test.js` tests the mapping and both messages; it runs in
`make js-test` (`jest.config.js` has `npm/` as a root and matches `*.test.js`).
The main package's `files` lists only `bin/simsapa.js`, so the test is never
packed.

### Packing: `make npm-packages`

`scripts/npm-pack.sh` packs into `dist/npm/*.tgz` and publishes nothing:

1. Reads the version from the `[package]` section of `backend_cli/Cargo.toml`
   and **refuses to pack unless it equals `backend/Cargo.toml`'s**. The binary
   reports the backend's version, not its own crate's: `--version` and the
   database release lookup both call `get_app_version()`, which is
   `env!("CARGO_PKG_VERSION")` evaluated in the backend crate. An npm version
   taken from a drifted `backend_cli/Cargo.toml` would label a binary that says
   otherwise.
2. Writes that version into the tracked `npm/*/package.json` files, in
   `version` and in the main package's `optionalDependencies` pins, if any of
   them differs, and names each file it changed. `scripts/set-version.py --app`
   already sets them with every other app-version place (AGENTS.md,
   "Versions"), so this is a backstop for a version changed by hand; a file it
   names belongs in the version commit. The files are written in
   `JSON.stringify(…, null, 2)` form, so keep them in that form or the script
   rewrites them.
3. Copies each `npm/<package>/` to `dist/npm/stage/<package>/`, and packs from
   there.
4. Copies `dist/cli/<platform>/simsapa[.exe]` into each platform package's
   `bin/` with mode 0755 (npm keeps the mode in the tarball, and the binary is
   not a `bin` entry, so npm would not set it on install), plus `LICENSE.txt`.
5. Runs `npm pack` in each staged folder.

A platform whose binary is missing from `dist/cli/` is skipped with a warning,
and the script ends by saying how many of the four it packed. The darwin and
win32 binaries are built on their own machines (`make cli-darwin-arm64`,
`make cli-win32-x64`); copy them into `dist/cli/darwin-arm64/` and
`dist/cli/win32-x64/` on the Linux machine and pack all four there.

> **Trap: don't add `npm/*/bin/` to `.gitignore`.** `npm/cli/bin/` holds the
> tracked launcher. The ignore entries are `npm/cli-*/bin/` and `npm/**/*.tgz`;
> `dist/` is ignored as a whole.

Packed sizes for 1.1.0-alpha.3:

| Tarball | Packed | Binary unpacked |
|---|---|---|
| `simsapa-cli-1.1.0-alpha.3.tgz` | 9,393 B | — |
| `simsapa-cli-linux-x64-1.1.0-alpha.3.tgz` | 9,855,437 B (9.4 MiB) | 26,577,032 B |
| `simsapa-cli-linux-arm64-1.1.0-alpha.3.tgz` | 9,414,547 B (9.0 MiB) | 23,913,464 B |
| `simsapa-cli-darwin-arm64-…` | not yet measured | |
| `simsapa-cli-win32-x64-…` | not yet measured | |

No official per-package size limit for the public npm registry was found.

### Dry run

Install the tarballs into a throwaway prefix rather than the real global
folder:

```sh
make npm-packages
PREFIX=$(mktemp -d)
npm install -g --prefix "$PREFIX" \
  dist/npm/simsapa-cli-linux-x64-<version>.tgz dist/npm/simsapa-cli-<version>.tgz
"$PREFIX/bin/simsapa" --version
"$PREFIX/bin/simsapa" paths
"$PREFIX/bin/simsapa" search suttas anicca --simsapa-dir <data folder>
rm -rf "$PREFIX"
```

The platform packages not given on the command line are not on the registry
yet; npm tolerates their 404 because they are optional. Installing the main
tarball alone exercises the missing-package message.

`serve` writes its port into the data folder's `api-port.txt`. A dry run of
`serve` against a data folder the GUI also uses leaves that port behind; put
the old value back afterwards.

### Publishing checklist

Publishing is manual and done by the maintainer. Nothing in the repository
publishes.

1. **Scope.** The `@simsapa` organisation must exist on npmjs.com and the
   publishing account must be allowed to publish to it (`npm whoami`,
   `npm org ls simsapa`). With two-factor authentication on the account, pass
   `--otp <code>` to each `npm publish`, or use a granular access token.
2. **Version.** The npm version is the app version. Set it with
   `scripts/set-version.py --app <version>`, which writes it into the
   `Cargo.toml`s, `Cargo.lock`s, `cpp/gui.cpp` and all five `package.json`s in
   the form each needs; `scripts/set-version.py` alone shows every place and
   exits 1 if they disagree. Commit those files before packing. An npm version
   can never be published twice (not even after an unpublish), so a broken
   publish is fixed with a new version.
3. **Build all four binaries** and collect them in `dist/cli/` (see "Release
   builds per target").
4. **`make npm-packages`** must report all four platform packages; never
   publish an incomplete set.
5. **Dry run** the Linux x64 tarballs as above.
6. **Publish the four platform packages first, then the main one**, from the
   tarballs that were dry-run:

   ```sh
   cd dist/npm
   for p in linux-x64 linux-arm64 darwin-arm64 win32-x64; do
     npm publish simsapa-cli-$p-<version>.tgz --access public --tag latest
   done
   npm publish simsapa-cli-<version>.tgz --access public --tag latest
   ```

   Platform packages go first because the main package names them at its exact
   version. If the main package is published first, anyone installing in
   between gets no binary: npm skips the missing optional dependency without an
   error, and `simsapa` then reports a missing platform package.

   **`--tag latest` is required while the version is a prerelease
   (`-alpha.N`).** npm 11 refuses to publish a prerelease without `--tag`, and
   a package whose only versions sit under another tag (such as `next`) has no
   `latest`, so a plain `npm install -g @simsapa/cli` fails.
7. **Check:** `npm view @simsapa/cli dist-tags` shows `latest: <version>`, and
   `npm install -g @simsapa/cli` on a clean machine installs a working
   `simsapa`.
