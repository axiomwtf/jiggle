# Jiggle Language Specification
**Version:** 0.2.1
**Ownership:** Jiggle is owned by AXIOM ![Axiom Logo](./axiom.svg)
---

## 1. Overview

Jiggle is a polyglot bundle format and compiler system. A single `project.jgl` file contains multiple language blocks which are unpacked into a virtual filesystem (VFS), bundled as a `.tar.xz`, and compiled into a single native binary. The compiler also auto-generates a `build.sh` build script.

 **The output is always, unconditionally, exactly one file: `jgl_binary` by default, or the project name specified via `name` in `project._jgl` (e.g. `name calc` produces `calc`).** No JARs. No DLLs. No sidecars. No "requires runtime" asterisks. Every language supported by Jiggle compiles down to a single binary. Runtimes that cannot be statically linked (JVM, .NET, CPython, etc.) are declared as system dependencies — the binary checks for them at startup and tells the user exactly what to install if they are missing, but the binary itself is still one file. If `jglc` cannot produce one binary, it fails with an error. It does not produce partial output.

---

## 2. File Extension

All Jiggle files use the `.jgl` extension. `jgl` is short for Jiggle.

Example: `project.jgl`

`._jgl` is not a file extension. It is the naming sigil for `._jgl` blocks inside a `.jgl` file.

The compiler reads only `project.jgl`. Any other `._jgl` blocks defined in it are discarded. Only the `project._jgl` manifest block is read.

---

## 3. Block Syntax

A Jiggle file is composed of blocks. Each block has a header and a body wrapped in `{}`.

### 3.1 Unnamed Blocks

```
.language {
    // code here
}
```

Unnamed blocks are auto-named `vX.language` in the VFS, where X is the sequential index of that language's unnamed blocks (e.g. `v1.rs`, `v2.rs`).

### 3.2 Named Blocks

```
name._language {
    // code here
}
```

Named blocks are extracted as `name.language` in the VFS. The underscore in `._language` is the naming sigil. Named blocks are referenceable by name in the manifest.

### 3.3 Manifest Block

The manifest block uses the `._jgl` sigil. By convention it is named `project` to match `project.jgl`.

```
project._jgl {
    name calc
    target native

    import "auth.jgl"
}
```

 **Manifest keywords:**
 - `name` — sets the output binary name (e.g. `name calc` produces `calc` instead of `jgl_binary`). Defaults to `jgl_binary` if not specified.
 - `import` — import and merge another `.jgl` file
 - `target` — compile target (`cross`, `native`, or a specific triple such as `x86_64-unknown-linux-gnu`)
 - `dep[lang] { ... }` — declare dependencies for a language, see section 3.5

### 3.4 Custom Languages

A custom language is defined inside the manifest with a block named `<extension>._sh`. The block name gives the file extension to handle. The body holds two things, in this order: the compile command for that extension, then a **required** `dep.<file>` block that declares the language's dependency format (see 3.5).

```
project._jgl {
    target cross

    vmx._sh {
        vmx-compile {in} -o {out} --deps {deps}

        dep.vmx.toml {
            {name} = "{version}"
        }
    }
}
```

This makes every `.vmx` block in the VFS compile with `vmx-compile`. Placeholders in the command:
- `{in}` — the input file path in the VFS
- `{out}` — the output path for the compiled object
- `{deps}` — the path of the generated dependency file (`vmx.toml` above)

Custom blocks are written into `build.sh` in the order they appear in the manifest. A custom extension that is already built in (for example `rs` or `py`) is a compiler error. A custom language without a `dep.<file>` block is a compiler error, even if the project declares no dependencies for it. In that case the generated dep file is simply empty.

### 3.5 Dependencies

Dependencies are declared inside `project._jgl` with a `dep[lang]` block. `lang` is the language extension (`rs`, `js`, `py`, or a custom extension).

```
project._jgl {
    target cross

    dep[rs] {
        serde = "1.0"
        regex = "1"
    }

    dep[js] {
        express = "^4.18.0"
    }
}
```

**Entry syntax.** One dependency per line: `name = "version"`. Use `"*"` for any version. `//` comments are allowed. The version string is passed through to the language's own ecosystem, so its meaning (semver range, exact pin) is whatever that ecosystem says.

**Generated dep files.** During compilation Jiggle translates each `dep[lang]` block into that ecosystem's native dependency file and adds it to the VFS root:

| Language | Generated file | Notes |
|----------|----------------|-------|
| Rust | `Cargo.toml` | `[dependencies]` table |
| JavaScript | `package.json` | `dependencies` object. `ts`, `tsx`, `jsx` share it, so use `dep[js]` |
| Go | `go.mod` | `require` entries |
| Python | `requirements.txt` | `name==version`, or bare name for `"*"` |
| Java / Kotlin / Scala | `pom.xml` | `name` is `group:artifact` |
| C# | `packages.config` | NuGet |
| Ruby | `Gemfile` | |
| PHP | `composer.json` | |
| Zig | `build.zig.zon` | |
| Nim | `jgl.nimble` | |
| Lua | `jgl-scm-1.rockspec` | |
| C / C++ | `vcpkg.json` | |
| Fortran | `fpm.toml` | |
| Custom | the file named in `dep.<file>` | formatted by the user's template |

Languages with no ecosystem file (`sh`, `md`, `adoc`, `bs`) cannot have a `dep[lang]` block. That is a compiler error.

**Custom dep format.** For a custom language, `dep.<file> { ... }` names the generated file and gives a per-dependency line template. The template is rendered once per dependency, in declaration order, with `{name}` and `{version}` substituted. The rendered lines are joined with newlines and written as `<file>` at the VFS root.

**Fetching.** The build script restores each language's dependencies before compiling that language (for example `cargo fetch`, `npm install --omit=dev`, `go mod download`, `pip install -r requirements.txt`). Dependency restore needs network access at build time. For custom languages, the command is responsible for reading `{deps}`.

---

## 4. Supported Languages

| Extension | Language | Notes |
|-----------|----------|-------|
| `._rs` / `.rs` | Rust | Native compile via `rustc`; Cargo used for dep resolution when `Cargo.toml` is present |
| `._js` / `.js` | JavaScript | Embedded as C data segment in the unified binary |
| `._ts` / `.ts` | TypeScript | Precompiled to JS at build time via `bun build` (preferred) or `npx tsc` (fallback), JS embedded as C data segment; `node` used at runtime instead of `npx tsx` for performance |
| `._jsx` / `.jsx` | JSX | Embedded as C data segment |
| `._tsx` / `.tsx` | TSX | Embedded as C data segment |
| `._java` / `.java` | Java | Compiled to JAR via `javac`; JAR embedded as C data segment; JVM declared as a runtime dependency |
| `._kt` / `.kt` | Kotlin | Compiled to JAR via `kotlinc`; JAR embedded as C data segment; JVM declared as a runtime dependency |
| `._cs` / `.cs` | C# | Compiled to `.dll` via `csc`; dll embedded as C data segment; .NET runtime declared as a runtime dependency |
| `._go` / `.go` | Go | Compiled to C archive (`-buildmode=c-archive`) and statically linked |
| `._c` / `.c` | C | Compiled to `.o` via `clang` or `gcc` and linked |
| `._cpp` / `.cpp` | C++ | Compiled to `.o` via `clang++` or `g++` and linked; functions must be wrapped in `extern "C"` for cross-language FFI |
| `._py` / `.py` | Python | Embedded as C data segment in the unified binary |
| `._lua` / `.lua` | Lua | Embedded as C data segment in the unified binary |
| `._sh` / `.sh` | Shell | Embedded as C data segment in the unified binary |
| `._md` / `.md` | Markdown | Becomes static data embedded in binary |
| `._adoc` / `.adoc` | AsciiDoc | Becomes static data embedded in binary |
| `._bs` / `.bs` | BrightScript | Roku target; does not participate in the single binary pipeline on non-Roku targets |
| `._zig` / `.zig` | Zig | Compiled to `.o` via `zig build-obj` and linked |
| `._nim` / `.nim` | Nim | Compiled to `.o` via `nim c --noLinking` and linked |
| `._rb` / `.rb` | Ruby | Embedded as C data segment in the unified binary |
| `._php` / `.php` | PHP | Embedded as C data segment in the unified binary |
| `._scala` / `.scala` | Scala | Compiled to JAR via `scalac`; JAR embedded as C data segment; JVM declared as a runtime dependency |
| `._f90` / `.f90` | Fortran | Compiled to `.o` via `gfortran` and linked |
| `._<ext>._sh` | Custom | Defined in the manifest, see section 3.4 |
| `._jgl` | Jiggle manifest | Imports, target, and custom language block definitions |

---

## 5. Auto Lint Suppression

The Jiggle compiler automatically injects style/lint suppressors per language so Jiggle's own naming conventions do not cause warnings or errors in the target language toolchain.

| Language | Injected Suppressor |
|----------|-------------------|
| Rust | `#![allow(non_snake_case, non_camel_case_types, non_upper_case_globals)]` |
| Kotlin | `@Suppress("all")` |
| C# | `#pragma warning disable` |
| JavaScript | `/* eslint-disable */` |
| TypeScript | `// @ts-nocheck` |
| JSX | `/* eslint-disable */` + `// @ts-nocheck` |
| TSX | `/* eslint-disable */` + `// @ts-nocheck` |
| Go | `//nolint:all` |
| Python | `# noqa: ALL` |
| C/C++ | `#pragma GCC diagnostic ignored "-Wall"` |
| Ruby | `# rubocop:disable all` |
| PHP | `// phpcs:ignoreFile` |
| Scala | `//scalastyle:off` |
| Zig | None |
| Nim | None |
| Fortran | None |
| Custom | None (set by the user) |
| Lua | None (Lua does not care) |
| MD/ADOC | None (not code) |
| BrightScript | None |

Suppressors are injected at the top of the extracted file, not around user code.

---

## 6. VFS and Compilation Pipeline

```
project.jgl
        ↓
Read project.jgl (discard non-project ._jgl blocks)
        ↓
Parse manifest and resolve imports (depth-first)
        ↓
Parse blocks
        ↓
Unpack VFS (named and unnamed blocks → files, lint suppressors injected)
        ↓
Register custom languages from manifest
        ↓
Resolve dependencies (dep[lang] → generated dep files in the VFS root)
        ↓
Bundle into project.tar.xz
        ↓
Auto-generate build.sh
        ↓
Compile all files into one binary (or separate runtime packages for JVM/C#)
```

---

## 7. Auto-Generated Build Script

The Jiggle compiler outputs `build.sh` (Unix) alongside the `.tar.xz`. Pass `--target-win` to cross-compile for Windows and produce `jgl_binary.exe`. The build script:

- Extracts `project.tar.xz` into `jgl_build/`
- Restores each language's dependencies from the generated dep files
- Compiles all native languages (Rust, C, C++, Go, Zig, Nim, Fortran) into object files or archives
- Precompiles TypeScript (`.ts`) to JavaScript (`.js`) via `bun build` (preferred) or `npx tsc` (fallback) for faster runtime execution, embeds the JS as C data segments
- Embeds scripting languages (JS, TS, TSX, JSX, Python, Lua, Ruby, PHP, BrightScript, Shell) as C data segments compiled to `.o` files
- Compiles JVM languages (Java, Kotlin, Scala) to JARs and embeds them as C data segments; the binary unpacks and launches them via the JVM at runtime (JVM declared as a runtime dependency). The JAR data is embedded in the binary — no separate JAR file is produced as output.
- Compiles C# to a `.dll` and embeds it as a C data segment; the binary unpacks and launches it via the .NET runtime at runtime (.NET declared as a runtime dependency). The DLL data is embedded in the binary — no separate DLL file is produced as output.
- Links everything into a single binary. The binary is named from the `name` field in `project._jgl` (defaults to `jgl_binary`). Pass `--r` to `build.sh` for a release build (`-O3 -s`); the default is debug (`-O0 -g`). After linking, the binary is copied to the project root and `jgl_build/` is cleaned up.

**The output is always one file.** Regardless of which languages are used, the final product is a single binary. Language payloads (JARs, DLLs, scripts) are embedded as data segments within the single binary. Runtimes that cannot be statically linked (JVM, .NET, Python, Lua, Node.js) are declared as runtime dependencies — the binary checks for them at startup and tells the user what to install if they are missing.

When Rust is present it acts as the linker: all other native objects and embedded script objects are passed as link arguments to `rustc`. When Rust is absent, `clang` or `gcc` handles final linking.

The user never writes a Makefile, CMake config, Gradle build, or package.json scripts. Jiggle handles everything.

> **Note:** The spec previously described a `build.bat` output. This has been removed. Windows cross-compilation is handled by passing `--target-win` to `jglc`, which produces a `jgl_binary.exe` via the `x86_64-pc-windows-gnu` target inside `build.sh`.

---

## 8. CLI

```
jglc compile  [--project <file>] [--output <dir>] [--bundle-only] [--target-win]
jglc build    (alias for compile)

jglc validate [--project <file>]
jglc check    (alias for validate)

jglc generate [--project <file>] [--output <dir>] [--target-win]
jglc gen      (alias for generate)
```

- `compile` — full pipeline: parse → VFS → bundle → build scripts → run `build.sh`
- `compile --bundle-only` — stop after generating the bundle and build scripts; do not run `build.sh`
- `validate` — parse and validate without producing any output
- `generate` — generate the bundle and `build.sh` without running it

Default project file: `project.jgl`. Default output directory: `.`.

Run `build.sh --r` for a release-optimized build (`-O3 -s`). Without `--r`, `build.sh` builds in debug mode (`-O0 -g`). In both cases, the binary is copied to the project root and `jgl_build/` is cleaned up.

---

## 9. Conflict Resolution

- Named blocks from different imported `.jgl` files with the same name cause a **compiler error** requiring manual resolution in the manifest
- Unnamed blocks from different files are auto-numbered sequentially and never conflict
- Import resolution is depth-first
- `dep[lang]` blocks for the same language, including those from imported `.jgl` files, are merged. The same dependency name with the same version is deduplicated. The same name with different versions is a **compiler error** requiring manual resolution in the manifest
- A generated dep file whose name matches an existing VFS file is a **compiler error**

---

## 10. Example

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

.rs {
    fn helper() -> &'static str {
        "v1.rs unnamed block"
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

---

## 11. Notes

- BrightScript targets Roku only and does not participate in the single binary pipeline on non-Roku targets
- MD and ADOC blocks are embedded as static string data accessible at runtime
- The manifest block is conventionally named `project`, matching `project.jgl`
- `dep[lang]` is manifest-only. Outside `project._jgl` it is a parse error
- `jgl` is short for Jiggle
- The compiler is written in Rust (edition 2024) and lives in `jglc/`
