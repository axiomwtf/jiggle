<p align="center">
  <img src="./axiom.svg" alt="Axiom" width="320">
</p>

# Jiggle

**Many languages in. One binary out.**

Jiggle (`.jgl`) is a polyglot bundle format and compiler system by [Axiom](https://github.com/axiomwtf). A single `project.jgl` file holds blocks written in different languages. `jglc` unpacks them into a virtual filesystem, bundles them as a `.tar.xz`, generates a `build.sh`, and compiles everything into one native binary.

> **Status:** `0.2.0-draft`. The spec and the compiler are both early and will change.

## Why

- **One output.** The result is always exactly one file: `jgl_binary` (or `jgl_binary.exe`). If `jglc` can't produce one binary, it fails with an error instead of producing partial output.
- **No build files.** You never write a Makefile, CMake config, Gradle build or `package.json` scripts. Jiggle generates what it needs.
- **Mix languages freely.** Use Rust for the engine, TypeScript for the UI, Lua for scripting and Markdown for docs, all in the same file.
- **Declarative dependencies.** A `dep[lang]` block in the manifest is translated into each ecosystem's native file (`Cargo.toml`, `package.json`, `go.mod`, `requirements.txt` and more).

## Example

```
project._jgl {
    target cross

    dep[js] {
        express = "^4.18.0"
    }
}

main._rs {
    fn main() {
        println!("Hello from Jiggle");
    }
}

server._js {
    const app = require('express')
    app.listen(3000)
}

.lua {
    print("lua unnamed block, becomes v1.lua")
}

readme._md {
    # My Jiggle Project
    Built with Jiggle.
}
```

- `name._lang { ... }` is a named block, extracted as `name.lang`.
- `.lang { ... }` is an unnamed block, auto-named `v1.lang`, `v2.lang` and so on.
- `project._jgl { ... }` is the manifest. It sets the target, imports and dependencies.

## Usage

```bash
jglc compile                  # full pipeline, runs build.sh
jglc compile --bundle-only    # bundle + build.sh only, don't run it
jglc compile --target-win     # cross-compile for Windows (jgl_binary.exe)
jglc validate                 # parse and validate, no output
jglc generate                 # generate bundle and build.sh without running it
```

Aliases: `build` for `compile`, `check` for `validate`, `gen` for `generate`. Every command takes `--project <file>` (default `project.jgl`), and `compile` and `generate` take `--output <dir>` (default `.`).

## Supported languages

| Kind | Languages |
|------|-----------|
| Compiled and linked natively | Rust, C, C++, Go, Zig, Nim, Fortran |
| Embedded as data in the binary | JavaScript, TypeScript, JSX, TSX, Python, Lua, Ruby, PHP, Shell |
| Embedded, needs a runtime | Java, Kotlin, Scala (JVM), C# (.NET) |
| Static data | Markdown, AsciiDoc |
| Special | BrightScript (Roku target only) |
| Custom | Define your own in the manifest, see the spec |

Runtimes that can't be statically linked (JVM, .NET) are declared as dependencies. The binary checks for them at startup and tells the user what to install. The output is still one file.

## Building from source

The compiler is written in Rust (edition 2024) and lives in [`jglc/`](./jglc).

```bash
cd jglc
cargo build --release
```

Dependency restore at build time needs network access.

## Spec

The full language specification is in [`SPEC.md`](./SPEC.md).

## License

MPL-2.0
