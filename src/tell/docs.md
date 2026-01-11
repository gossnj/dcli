# Noridoc: tell Library

Path: @/dcli/src/tell

### Overview

Lightweight library for managing console output in command-line applications. Provides level-based output filtering and formatted printing for CLI tools.

### How it fits into the larger codebase

Used by @/dcli/src/dcli core library and all CLI tools (@/dcli/src/dclisync, @/dcli/src/dcliah, etc.) for console output. Allows applications to control verbosity - normal output vs verbose/debug output. Independent utility library with no dependencies on other dcli modules.

### Core Implementation

**Primary API** (@/dcli/src/tell/src/lib.rs): `Tell` trait and `TellLevel` enum.

**TellLevel Enum**: Variants `Debug`, `Progress`, `Info`, `Warn`, `Error`. Determines output priority/visibility.

**Tell Trait**: Implemented on types that can print to console. Likely includes methods like `tell()` or `print_at_level()` that respect level filtering.

**Macros** (@/dcli/src/tell/src/macros.rs): Convenience macros for common output patterns. Simplify calling syntax for CLI applications.

**Usage Pattern**: CLI tools set verbosity level, then use tell macros/trait for all output. Only messages at or above the threshold level are printed.

### Things to Know

**Minimal Scope**: This is purely a console output utility. No logging to files, no structured logging, no async support. Just level-filtered println!-style output.

**CLI Integration**: Typically paired with command-line argument like `--verbose` that enables Debug/Info level output. Without verbose flag, only Progress/Warn/Error shown.

**Not Used by FFI**: FFI layer at @/dcli/src/dcli-ffi uses eprintln! directly for debug output since there's no console to manage. tell library only relevant for CLI applications.

Created and maintained by Nori.
