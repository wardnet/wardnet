# ClusterFuzzLite

Coverage-guided fuzzing for wardnet's deserialization boundaries —
bundle archiver, SQLite restore, and bundle-manifest JSON.

## Layout

- `project.yaml` — tells CFLite the language is Rust.
- `Dockerfile` — builds on top of `gcr.io/oss-fuzz-base/base-builder-rust`,
  copies the repo, and stages `build.sh`.
- `build.sh` — invoked inside the image at build time. Runs
  `cargo build` (not `cargo fuzz build`) against `source/daemon/fuzz/`
  with sanitizer flags set via target-specific `RUSTFLAGS` so
  proc-macro crates are not instrumented, then copies the resulting
  binaries into `$OUT/`.

Fuzz targets and harnesses live at `source/daemon/fuzz/` — a
**standalone** cargo workspace (the daemon workspace excludes it)
so `libfuzzer-sys` + nightly sanitizer rustflags stay out of the
normal daemon build path.

## Running locally

```
cd source/daemon/fuzz
cargo +nightly fuzz run archiver_unpack
cargo +nightly fuzz run sqlite_restore
cargo +nightly fuzz run bundle_manifest
```

`cargo fuzz` needs nightly Rust even though the daemon pins stable —
that's what the `+nightly` override is for.

## Triaging a crash

CI fuzzing runs (`fuzzing-scheduled.yml` twice daily in batch mode,
`fuzzing-maintenance.yml` weekly for coverage reports + corpus
pruning) store the corpus and crash reproducers in the companion repo
`wardnet/wardnet-fuzz-corpus`. When a fuzz job fails:

1. Download the crash reproducer artifact from the failed Actions run
   (it's attached as `artifacts.zip`).
2. Reproduce locally:
   ```
   cd source/daemon/fuzz
   cargo +nightly fuzz run <target> path/to/crash-input
   ```
3. Fix the bug. Keep the crash input as a regression seed by
   committing it to `source/daemon/fuzz/regression-inputs/<target>/`.
4. File a GitHub issue with the stack trace + reproducer path.

## Secrets

The PR and weekly workflows need a fine-grained PAT scoped to
`wardnet/wardnet-fuzz-corpus` with `Contents: read and write`,
stored as the `FUZZ_CORPUS_PAT` secret on the main wardnet repo.

Fine-grained PATs expire (GitHub's max is 1 year). When it does,
the fuzzing workflows fail loudly on `git push`; main CI is
unaffected and the corpus itself doesn't corrupt. To rotate:

1. Generate a new PAT at
   https://github.com/settings/personal-access-tokens/new with the
   same scopes.
2. `gh secret set FUZZ_CORPUS_PAT --repo wardnet/wardnet` and paste
   the new token.
3. Delete the old token.

## Scope — what's fuzzed and what isn't

Fuzzing targets the trust boundaries where wardnet parses
attacker-influenceable bytes. Currently that's the backup/restore
pipeline — four stacked parsers (age → gzip → tar → JSON+SQLite) on a
path an operator can be tricked into triggering with a crafted bundle.

Candidates to add incrementally as new features land:

- **DNS response parsing** — once DNS forwarding is implemented.
- **VPN provider config parsers** — `.ovpn` / similar user-uploaded files.
- **Update manifest JSON** — fetched from the release endpoint.
- **TOML config parsing** — lower priority; admin-supplied.

Deliberately NOT fuzzed:

- HTTP request bodies — axum + serde + utoipa handle deserialization,
  and serde_json itself is heavily fuzzed upstream in rust-fuzz.
  Handler-level fuzzing would mostly rediscover known bugs.
- Repository methods / service orchestration — no untrusted input.
- OUI database — bundled at build time from our own data.

Scorecard's `Fuzzing` check is binary: 3 targets and 300 targets score
identically, so there's no pressure to over-expand.

## References

- ClusterFuzzLite docs: https://google.github.io/clusterfuzzlite/
- cargo-fuzz book: https://rust-fuzz.github.io/book/cargo-fuzz.html
- libFuzzer tutorial: https://llvm.org/docs/LibFuzzer.html
