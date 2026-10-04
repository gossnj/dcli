# dcli tests

This folder contains scripts for simple testing of the compiled apps.

* **runapps** Runs all dcli apps on via a bash shell
* **runapps.bat** Windows bat file to run all apps

Both scripts expect that the apps can be found in the system / user PATH.

## SQLite data-layer index benchmark

Run `python3 tests/data_layer_bench.py` from the repository root to compare the
schema's baseline indexes with the proposed partial pending-queue index and
`team_result(activity)` index. The standard-library-only script loads the
checked-in activity-store schema into temporary SQLite databases, generates
deterministic fixtures at 10,000 and 100,000 matches, and prints machine-readable
JSON to standard output. Temporary databases are removed when the run ends.

The run includes mostly-complete (1% pending), full-pending, and empty-queue
distributions; query result/order checks and query plans; first-query timing
(labelled non-cold), warm median and p95; index build time; database and WAL
sizes after checkpoint; and replicated write transaction costs before and
after the candidate indexes. It uses the full pending-queue query without a
limit, so a 100,000-match full-pending case fetches all 100,000 IDs into memory.
The JSON reports an estimate of the returned Python list/tuple/value object
size for each query; this is not peak process memory. Tune the workload with
`--sizes 10000 100000`, `--repeats 15`, `--write-replicates 3`, and
`--write-rows 1000`. Timings are observational and are not suitable as brittle
CI pass/fail thresholds. This SQLite-only benchmark does not measure Rust/FFI
overhead, network or device performance, full application workloads, or peak RSS.
