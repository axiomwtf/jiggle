# jglc — jiggle compiler

A compiler that generates unified native binaries whose C code can call methods defined in other languages (Java, Kotlin, Scala, C#, Python, Lua, TypeScript, JavaScript) through type-aware FFI bridges, plus C/C++, Go, Zig, Rust, Nim, and Fortran sources compiled natively.

## Stack

- Language: Rust (edition 2024), default toolchain 1.99.0 via rustup (`stable-x86_64-unknown-linux-gnu`).
- Build system: Cargo.
- FFI: generated C stubs calling into embedded sources via subprocess bridges (JSON/positional args, result files framed with a build nonce).
- Cross-compile target: `x86_64-pc-windows-gnu` (rust-std installed; needs the mingw linker `x86_64-w64-mingw32-gcc` to link).

## How to run

```bash
# Build the compiler (release, at nice -n 15 to yield to foreground work)
cd /home/void01x/omp/jglc
nice -n 15 cargo build --release

# Install the binary
cp target/release/jglc ~/.local/bin/jglc

# Generate + build a project
jglc generate --project <project.jgl> --output <out_dir>
cd <out_dir>
bash build.sh --r   # --r runs the linked binary interactively
```

## Windows cross-compile

```bash
RUSTUP_HOME=/home/void01x/.rustup CARGO_HOME=/home/void01x/.cargo \
  PATH="/home/void01x/.rustup/toolchains/stable-x86_64-unknown-linux-gnu/bin:$PATH" \
  nice -n 15 cargo build --release --target x86_64-pc-windows-gnu
```
Requires the mingw linker (`x86_64-w64-mingw32-gcc`) on PATH.

## Conventions

- Commits use conventional-commit prefixes (`fix:`, `feat:`, `docs:`, `chore:`, `refactor:`, `test:`) matching the repo log.
- Never commit `target/`, `dist/`, `node_modules/`, `*.jgl_binary`, or generated bundles.
- Rust quality bars: no `unwrap()`/`expect()` outside `#[cfg(test)]`; libs return typed `thiserror` errors; binaries use `anyhow`; every `unsafe` carries a `// SAFETY:` comment.
- Never background a shell; every compile runs at `nice -n 15`, serially.

## Source layout

- `src/main.rs` — CLI entry.
- `src/build_scripts.rs` — generates `build.sh` per project including FFI bridge C stubs.
- `src/ffi.rs` — `JglType`, `MethodSig`, `Param`, method-signature parsing for Java/Kotlin/Scala/C#/Python/Lua/TypeScript.
- `src/parser.rs`, `src/compiler.rs`, `src/manifest.rs`, `src/vfs.rs` — block parsing, compilation, virtual filesystem.
- `src/deps.rs`, `src/error.rs` — dependency restoration, error types.

## Notes

- Java/Kotlin/Scala need `kotlinc`/`scalac`/`javac`; if missing, those language probes are skipped (environmental, not defects).
- C# and Java bridges require `dotnet`/`java` runtimes at target runtime; their DLLs/JARs are embedded in the binary.
- Python/Lua/TypeScript bridges require `python3`/`lua`/`node` at runtime.
