# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

uutils coreutils is a cross-platform reimplementation of the GNU coreutils in Rust. It provides Unix command-line utilities like `ls`, `cat`, `mv`, `cp`, etc. The project aims to be a drop-in replacement for GNU coreutils while being cross-platform, reliable, performant, and well-tested.

**Important:** This project cannot contain any code from GNU or other GPL implementations. Never reference GNU source code directly.

## Build and Development Commands

### Building
```bash
# Build all utilities (multicall binary)
cargo build --release

# Build specific utilities as individual binaries
cargo build -p uu_cat -p uu_ls -p uu_mv

# Build with platform-specific features
cargo build --release --features unix    # Unix platforms
cargo build --release --features macos   # macOS specific
cargo build --release --features windows # Windows specific

# Using make (requires GNU Make)
make                    # Debug build
make PROFILE=release   # Release build
make UTILS='cat ls mv' # Build specific utilities
```

### Testing
```bash
# Run tests for all utilities
cargo test
cargo test --features unix  # Include platform-specific tests

# Test specific utilities
cargo test --features "chmod mv tail" --no-default-features

# Run with nextest (faster parallel execution)
cargo nextest run --features unix --no-fail-fast

# Using make
make test
make UTILS='cat ls' test
make SKIP_UTILS='dd df' test

# Run GNU test suite comparison
bash util/build-gnu.sh
bash util/run-gnu-test.sh
bash util/run-gnu-test.sh tests/touch/not-owner.sh  # Single test
```

### Code Quality
```bash
# Format code
cargo fmt --all

# Lint with clippy
cargo clippy --all-targets --all-features

# Check with cargo-deny (licenses, security, etc.)
cargo deny --all-features check all

# Spell checking with cspell
# Add spell-checker:ignore comments for words to ignore

# Run pre-commit hooks
pre-commit install  # Setup
# Commits will automatically run checks
```

### Testing Specific Scenarios
```bash
# Test with busybox test suite
make busytest
make UTILS='cat ls' busytest

# Test with specific GNU coreutils version
bash util/build-gnu.sh --release-build
bash util/run-gnu-test.sh tests/misc/sm3sum.pl  # Perl test
DEBUG=1 bash util/run-gnu-test.sh tests/misc/sm3sum.pl  # Debug mode
```

## Architecture and Code Organization

### Project Structure
- **`src/uu/`** - Each utility implemented as separate crate (e.g., `src/uu/cat/`, `src/uu/ls/`)
- **`src/uucore/`** - Shared library code between utilities (parsing, file ops, etc.)
- **`src/bin/coreutils.rs`** - Multicall binary that can invoke any utility
- **`tests/by-util/`** - Integration tests for each utility
- **`tests/uutests/`** - Test framework and utilities

### Utility Structure
Each utility follows this pattern:
```
src/uu/<utility>/
├── Cargo.toml           # Dependencies and metadata
├── LICENSE             # MIT license
├── locales/            # Internationalization files (.ftl)
│   ├── en-US.ftl
│   └── fr-FR.ftl
└── src/
    ├── main.rs         # Entry point (usually just macro call)
    └── <utility>.rs    # Main implementation
```

### Key Architectural Patterns

1. **Multicall Binary**: The main `coreutils` binary can function as any utility based on how it's invoked (like busybox)

2. **Platform Abstraction**: Many utilities have `platform/` subdirectories with OS-specific implementations:
   ```
   src/platform/
   ├── mod.rs
   ├── unix.rs
   ├── windows.rs
   └── macos.rs
   ```

3. **Shared Core**: `uucore` provides common functionality:
   - Argument parsing utilities
   - File system operations
   - Cross-platform compatibility layers
   - Error handling patterns
   - Formatting and display utilities

4. **Feature Flags**: Extensive use of Cargo features for:
   - Platform-specific utilities (`feat_os_unix`, `feat_os_windows`)
   - Optional functionality (`feat_acl`, `feat_selinux`)
   - Utility groupings (`feat_common_core`, `feat_Tier1`)

### Internationalization (i18n)
- Uses Fluent localization system
- Locale files in `locales/` directories as `.ftl` files
- Build process copies locales to target directory
- Supports multiple languages (en-US, fr-FR, etc.)

## Development Guidelines

### Code Style (from CONTRIBUTING.md)
- **Never `panic!`** - Use proper error handling instead of `.unwrap()` or `panic!`
- **Never `exit`** - Use `Result` types for error handling; don't call `std::process::exit`
- **Minimal `unsafe`** - Only for FFI calls, with `// SAFETY:` comments
- **Use `OsStr`/`Path`** - For file paths, not `String`/`str` (supports invalid UTF-8)
- **Avoid macros** - Use simpler alternatives when possible

### Error Handling Patterns
- Return `Result` types rather than exiting
- Use `uucore::error` utilities for consistent error messages
- Handle invalid UTF-8 in paths and arguments gracefully

### Testing Strategy
- Integration tests in `tests/by-util/test_<utility>.rs`
- Unit tests within utility source files
- GNU compatibility tests via `util/run-gnu-test.sh`
- Cross-platform testing in CI

### Performance Considerations
- Many utilities have `BENCHMARKING.md` files with performance notes
- Use efficient algorithms for large file operations
- Platform-specific optimizations where appropriate (e.g., splice on Linux)

## Common Development Tasks

### Adding a New Utility
1. Create directory structure in `src/uu/<name>/`
2. Add to workspace in main `Cargo.toml`
3. Implement following existing utility patterns
4. Add to appropriate feature sets in `Cargo.toml`
5. Create tests in `tests/by-util/test_<name>.rs`
6. Add to `GNUmakefile` utility lists

### Debugging Utilities
```bash
# Debug a specific utility
rust-gdb --args target/debug/coreutils ls
(gdb) b ls.rs:79
(gdb) run

# Run single utility with debugging
cargo run --bin <utility> -- <args>
cargo run --features cat -p uu_cat -- file.txt
```

### Platform-Specific Development
- Use `#[cfg(unix)]`, `#[cfg(windows)]` for conditional compilation
- Implement in `platform/` subdirectories when needed
- Test on multiple platforms via CI or local VMs

### Working with GNU Test Suite
- Install `quilt` for patch management
- Follow instructions from `util/build-gnu.sh` for first-time setup
- Use `util/remaining-gnu-error.py` to see failing tests
- GNU tests require individual utility binaries (not multicall)

Remember: This project maintains strict compatibility with GNU coreutils behavior while improving cross-platform support and code safety through Rust.
