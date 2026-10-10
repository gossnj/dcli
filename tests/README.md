# dcli validation

Run commands from the repository root. The Cargo workspace is pinned to Rust 1.83.0 and uses the checked-in lockfile. `DESTINY_API_KEY` is needed at compile time; the validation-only value below is for isolated test builds only and should not be used for binaries intended for normal use.

```sh
DESTINY_API_KEY=validation-only-key cargo +1.83.0 test --locked --workspace --manifest-path src/Cargo.toml
DESTINY_API_KEY=validation-only-key cargo +1.83.0 build --locked --workspace --manifest-path src/Cargo.toml
python3 tests/test_build_scripts.py
python3 tests/test_cli_smoke.py
python3 tests/test_native_abi.py src/target/debug/libdcli_ffi.so
python3 tests/native_init_diagnostics.py src/target/debug/libdcli_ffi.so
```

On macOS, pass `src/target/debug/libdcli_ffi.dylib` to both native-library checks. `test_cli_smoke.py` expects the seven debug executables produced by the workspace build. The build-script checks use temporary stub tools to verify missing-key and environment-key handling; they do not package or copy libraries. Native ABI checks compile and link a C contract against both public headers, while initialization diagnostics exercise the built FFI library.

The `runapps` and `runapps.bat` scripts provide a manual smoke run of installed command-line tools found on `PATH`.

## SQLite data-layer benchmark

Run `python3 tests/data_layer_bench.py` to compare SQLite query and write behavior using the checked-in activity-store schema and deterministic temporary fixtures. It covers mostly-complete queues (one percent pending), full-pending queues, and empty queues at the configured sizes. The JSON includes result/order checks and query plans, first-query timing, warm median and p95, index-build time, checkpointed database/WAL sizes, and replicated write-transaction costs.

The defaults use 10,000 and 100,000 matches, 15 warm query repetitions, three write replicates, and 1,000 inserted rows per write transaction. These can be adjusted with `--sizes`, `--repeats`, `--write-replicates`, and `--write-rows`; for example: `python3 tests/data_layer_bench.py --sizes 10000 100000 --repeats 15 --write-replicates 3 --write-rows 1000`. The full-pending query materializes all pending IDs, so the largest full-pending case returns 100,000 IDs. Reported result-object size is an estimate, not peak process memory. Timings are observational rather than CI thresholds; this SQLite-only benchmark does not measure Rust/FFI overhead, network or device performance, or complete application workloads. The tool uses only the Python standard library and removes temporary databases when it exits.
