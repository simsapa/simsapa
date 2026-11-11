# New bootstrap import sources go into the preflight check

**When a bootstrap step starts reading a new file or folder under
`bootstrap-assets-resources/` (or a new external service), add it to
`bootstrap_sources()` in `bootstrap_cli/src/bootstrap/preflight.rs` in the same
change.** Write the path **once**, as a constant in
`bootstrap_cli/src/bootstrap/source_paths.rs`, and build it from that constant
in the import step, the preflight entry, any CLI subcommand that re-runs the
step, and tests that read the real checkout — never as a string literal in any
of them, so the preflight checks exactly the path the step reads. A path inside
a source folder is a `*_SUBDIR` / `*_SUBFILE` constant (or a helper such as
`cst_toc_dir()`) relative to that folder, because CLI subcommands take the
folder as an argument. The preflight runs at the start of `bootstrap()`, **before**
`clean_and_create_folders()` empties `dist/`, and reports every missing source
at once. A source it does not list is found missing only when its step is
reached, often an hour in, after the previous build has already been wiped.

- Mark it **`required`** when the step stops the bootstrap without it, and
  **`optional`** when the step warns and skips. The two must agree: a required
  source's step must fail, not skip, when the source is missing.
- **Translations are required.** A source that carries sutta text or English
  translations (SuttaCentral, CST/OpenTipitaka, suttas-core, dhammatalks.org)
  is never optional: a skip silently ships a release without that content.
- List it under the step flag that runs it (`appdata`, `dpd`,
  `sc_languages`, `hungarian` in `PreflightSteps`), so `--skip-*` /
  `--only-languages` runs do not demand sources they never read.
- A new service the bootstrap connects to (like the SuttaCentral ArangoDB,
  checked in `run_preflight()`) gets a connection check there too.

After editing the list, run `cd bootstrap_cli && cargo test
test_full_bootstrap_sources -- --ignored`: it checks every required path against
the real `bootstrap-assets-resources` checkout, which catches a misspelled path.
It is `#[ignore]`d because it tests the local assets folder, not the code.

