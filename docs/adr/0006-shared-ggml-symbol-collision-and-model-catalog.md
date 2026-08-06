# 0006 — whisper.cpp/llama.cpp symbol collision, and a curated model catalog

**Status: current again.** Briefly superseded by
[ADR-0007](0007-ollama-backed-local-cleanup.md) (which moved local cleanup
to an Ollama-backed provider, removing `llama-cpp-2` and this workaround
along with it), then restored by [ADR-0008](0008-local-cleanup-in-process-again.md)
once product direction settled on "Local means zero extra installs." The
`dynamic-link` fix described below is back in `core/Cargo.toml`/
`macos/Package.swift` and must be kept working — re-validate against
`./scripts/build-macos-app.sh release` (not just `cargo test`, which
tolerates the collision as a warning) whenever `whisper-rs` or
`llama-cpp-2` change.

Tracked in [0004-phase3-cleanup-pass.md](../todos/0004-phase3-cleanup-pass.md).

## The linker collision

`whisper-rs` (STT, Phase 2) and `llama-cpp-2` (local Cleanup Pass, Phase 3)
each vendor and statically compile their own copy of ggml (the tensor
library both whisper.cpp and llama.cpp are built on). Statically linking
both into one binary — which `cargo build`/`cargo test` didn't catch, but
the real `swift build` app link did — produced ~600 duplicate-symbol linker
errors (`ggml_init`, `gguf_get_val_*`, `llama_*`, all defined twice).

Two fixes were rejected:
- **Linker flags** (`-multiply_defined suppress`, `-ld_classic`): Apple's
  current linker (ld-prime) dropped support for suppressing duplicate
  symbols; there is no equivalent to GNU ld's `--allow-multiple-definition`
  available here.
- **`llama-cpp-sys-2`'s `system-ggml-static` feature**: this links against
  an externally-provided ggml build, which would need to be the *same*
  ggml `whisper-rs-sys` already vendors — but the two crates pin different
  upstream ggml/llama.cpp snapshots with no guaranteed ABI compatibility.
  Forcing them to share one build is a deep, fragile hole not worth
  digging into.

**Fix:** built `llama-cpp-sys-2` with its `dynamic-link` feature
(`core/Cargo.toml`) instead of the default static build. This compiles
llama.cpp/ggml into separate `.dylib`s (`libllama.dylib`,
`libllama-common.dylib`, plus their own `libggml*.dylib`s) rather than
merging their object code into `libvoicedrop_core.a`. Two ggml copies can
coexist at runtime as long as they're never merged into the same static
archive/image at link time — dylibs keep separate symbol tables. whisper's
ggml stays statically linked (only one copy ends up in the static portion
of the final binary); llama's ggml lives in its own dylibs alongside it.

`macos/Package.swift` adds the matching `-L`/`-l`/`-rpath` linker flags,
computed from an absolute `target/release` path at manifest-parse time
(`repoRoot`/`targetReleaseDir`) rather than a relative `-L`, because the
`-rpath` needs to resolve at *run* time regardless of whether the binary
ends up as a bare `swift build` output or inside `VoiceDrop.app`.

**This is dev-only wiring, not a distribution story.** Phase 9
(distribution) needs to copy these dylibs into
`VoiceDrop.app/Contents/Frameworks` with proper `install_name`/`@rpath`
fixups (`install_name_tool`) instead of pointing the running app at the
build directory — noted as an explicit gap, not silently deferred.

Verified end-to-end through the actual bundled `.app` (not just `cargo
test`, which gave a false green on the original static-link version — its
link step tolerates duplicate symbols as warnings): the local Cleanup Pass
ran a real llama.cpp inference inside the running app with no dyld errors.

## Curated model catalog, not an arbitrary picker

Alongside this, `core/src/models.rs` adds a small catalog of downloadable
models (one STT entry, three Cleanup Pass candidates: Qwen2.5-0.5B,
Qwen2.5-1.5B, Llama-3.2-3B) plus download/delete functions, as the backing
plumbing for a future model picker in Phase 5's Settings Window ("choose a
suggested model, press Download"/"press Delete"). This is deliberately a
short, curated list rather than an arbitrary-URL picker — every entry is a
model actually run against this codebase — and deliberately reuses the
already-working llama.cpp path rather than introducing a dedicated
grammar-correction model family (e.g. Gramformer/CoEdIT): those are
T5/encoder-decoder architectures with no llama.cpp support, which would
mean fighting the exact same class of native-linking problem this ADR just
resolved, for a model family with far less GGUF tooling.

Verified with a real download+delete cycle against Qwen2.5-1.5B (not
previously present on the test machine): correct `Content-Length`-based
progress reporting, correct file placement, correct cleanup.

## Consequences

- Any future crate added to `voicedrop-core` that vendors its own ggml/
  llama.cpp/whisper.cpp copy needs the same `dynamic-link`-style treatment,
  or the same collision recurs.
- `cargo test`/`cargo build` alone are not sufficient to catch this class
  of bug — always validate native-dependency changes against
  `./scripts/build-macos-app.sh release` before considering them done.
- The model catalog's ids (`ModelCatalogEntry::id`) are a stable on-disk
  contract once shipped — renaming one orphans existing users' downloaded
  files.
