# Noridoc: tell Library Source

Path: @/dcli/src/tell/src

### Overview

Source implementation of the tell console output library. Contains the Tell trait definition, TellLevel enum, and convenience macros for level-filtered console output.

### How it fits into the larger codebase

Implementation of the tell library declared in @/dcli/src/tell/Cargo.toml. Used by all CLI tools in @/dcli/src for console output. Provides the abstraction layer that allows tools to control verbosity via command-line flags.

### Core Implementation

**lib.rs** (2791 lines): Core Tell trait and TellLevel enum definitions. Likely includes:
- `TellLevel` enum: Debug, Progress, Info, Warn, Error levels
- `Tell` trait: Methods for outputting messages at specific levels
- Global or per-instance verbosity threshold

**macros.rs** (2082 lines): Convenience macros for tell usage. Likely patterns like:
```rust
tell_info!("message");
tell_debug!("message");
tell_error!("message");
```

Macros simplify call sites by wrapping Tell trait methods with automatic level specification.

**Usage Pattern**:
```rust
// Set verbosity level
let tell = Tell::new(TellLevel::Info);

// Only prints if level >= Info
tell_info!(tell, "Processing data...");

// Doesn't print if level < Debug
tell_debug!(tell, "Detailed debug info");
```

### Things to Know

**No File Output**: Pure console output to stdout/stderr. No file logging, no structured logging format.

**Thread Safety**: Likely not explicitly thread-safe. CLI tools typically single-threaded or use tell from main thread only.

**Performance**: Minimal overhead. Level check is simple comparison, skipped output has negligible cost.

**Integration with Indicatif**: CLI tools combine tell with indicatif progress bars from @/dcli/src/dcli. Tell handles text output, indicatif handles progress visualization.

Created and maintained by Nori.
