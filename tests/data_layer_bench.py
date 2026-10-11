#!/usr/bin/env python3
"""Compare baseline and proposed SQLite indexes using the repository schema.

This is an observational benchmark, not a CI performance test. It uses only
Python's standard library and creates all databases in a temporary directory.
"""

from __future__ import annotations

import argparse
import json
import math
import shutil
import sqlite3
import statistics
import sys
import tempfile
import time
from pathlib import Path


SCHEMA = Path(__file__).resolve().parents[1] / "src/dcli/actitvity_store_schema.sql"
CANDIDATE_INDEXES = (
    "activity_queue_pending_character_index",
    "team_result_activity_index",
)
INDEX_DDL = (
    "CREATE INDEX activity_queue_pending_character_index "
    "ON activity_queue(character, activity_id DESC) WHERE synced = 0",
    "CREATE INDEX team_result_activity_index ON team_result(activity)",
)
CHARACTER_ID = 1
TEAMS_PER_ACTIVITY = 4
QUEUE_QUERY = (
    "SELECT activity_id FROM activity_queue "
    "WHERE character = ? AND synced = 0 ORDER BY activity_id DESC"
)
TEAM_QUERY = (
    "SELECT team_id, score, standing FROM team_result WHERE activity = ? "
    "ORDER BY standing ASC"
)


def percentile95(samples: list[float]) -> float:
    """Nearest-rank p95, in milliseconds."""
    ordered = sorted(samples)
    return ordered[max(0, math.ceil(0.95 * len(ordered)) - 1)] * 1000


def timed_query(
    connection: sqlite3.Connection,
    sql: str,
    args: tuple[int, ...],
    repeats: int,
) -> dict[str, object]:
    # Deliberately label the first execution as non-cold: OS and SQLite caches
    # cannot be reliably flushed in a portable standard-library benchmark.
    started = time.perf_counter()
    first_rows = connection.execute(sql, args).fetchall()
    first_ms = (time.perf_counter() - started) * 1000
    samples: list[float] = []
    rows = first_rows
    del first_rows
    for _ in range(repeats):
        started = time.perf_counter()
        rows = connection.execute(sql, args).fetchall()
        samples.append(time.perf_counter() - started)
    return {
        "first_query_non_cold_ms": first_ms,
        "warm_median_ms": statistics.median(samples) * 1000,
        "warm_p95_ms": percentile95(samples),
        "result_rows": len(rows),
        "materialized_result_bytes": sys.getsizeof(rows)
        + sum(sys.getsizeof(row) + sum(sys.getsizeof(value) for value in row) for row in rows),
    }


def connect(path: Path) -> sqlite3.Connection:
    connection = sqlite3.connect(path)
    connection.execute("PRAGMA foreign_keys = ON")
    return connection


def initialize(path: Path) -> sqlite3.Connection:
    connection = connect(path)
    connection.execute("PRAGMA journal_mode = WAL")
    connection.executescript(SCHEMA.read_text(encoding="utf-8"))
    # Build candidate indexes after loading the fixture; preserve every other
    # schema index so the baseline stays representative.
    for name in CANDIDATE_INDEXES:
        connection.execute(f'DROP INDEX IF EXISTS "{name}"')
    connection.execute(
        "INSERT INTO member(member_id, platform_id, display_name) VALUES (1, 1, 'fixture')"
    )
    connection.execute(
        "INSERT INTO character(character_id, member, class) VALUES (?, 1, 0)",
        (CHARACTER_ID,),
    )
    return connection


def populate(connection: sqlite3.Connection, matches: int, distribution: str) -> None:
    activity_rows = (
        (activity_id, f"2026-01-{(activity_id % 28) + 1:02d}T00:00:00Z", 1, 1, 1, 1)
        for activity_id in range(1, matches + 1)
    )
    queue_rows = (
        (activity_id, 0 if is_pending(activity_id, distribution) else 1, activity_id, CHARACTER_ID)
        for activity_id in range(1, matches + 1)
    )
    team_rows = (
        ((activity_id - 1) * TEAMS_PER_ACTIVITY + team_id, team_id, activity_id, 1000 - team_id * 25, team_id)
        for activity_id in range(1, matches + 1)
        for team_id in range(1, TEAMS_PER_ACTIVITY + 1)
    )
    with connection:
        connection.executemany(
            "INSERT INTO activity(activity_id, period, mode, platform, director_activity_hash, reference_id) "
            "VALUES (?, ?, ?, ?, ?, ?)",
            activity_rows,
        )
        connection.executemany(
            "INSERT INTO activity_queue(id, synced, activity_id, character) VALUES (?, ?, ?, ?)",
            queue_rows,
        )
        connection.executemany(
            "INSERT INTO team_result(id, team_id, activity, score, standing) VALUES (?, ?, ?, ?, ?)",
            team_rows,
        )


def is_pending(activity_id: int, distribution: str) -> bool:
    if distribution == "full_pending":
        return True
    if distribution == "empty_queue":
        return False
    return activity_id % 100 == 0


def build_candidate_indexes(connection: sqlite3.Connection) -> float:
    started = time.perf_counter()
    with connection:
        for ddl in INDEX_DDL:
            connection.execute(ddl)
    return (time.perf_counter() - started) * 1000


def query_plan(connection: sqlite3.Connection, sql: str, args: tuple[int, ...]) -> list[str]:
    return [row[3] for row in connection.execute("EXPLAIN QUERY PLAN " + sql, args)]


def index_names(connection: sqlite3.Connection) -> set[str]:
    return {row[0] for row in connection.execute("SELECT name FROM sqlite_master WHERE type = 'index'")}


def checkpoint_sizes(connection: sqlite3.Connection, path: Path) -> dict[str, int]:
    connection.execute("PRAGMA wal_checkpoint(TRUNCATE)").fetchall()
    wal = Path(str(path) + "-wal")
    return {
        "database_bytes": path.stat().st_size,
        "wal_bytes_after_checkpoint": wal.stat().st_size if wal.exists() else 0,
    }


def measure_scenario(
    directory: Path, matches: int, distribution: str, repeats: int
) -> dict[str, object]:
    paths = {label: directory / f"query-{matches}-{distribution}-{label}.sqlite" for label in ("baseline", "proposed")}
    connections: dict[str, sqlite3.Connection] = {}
    output: dict[str, object] = {
        "matches": matches,
        "queue_distribution": distribution,
        "pending_queue_rows": sum(1 for i in range(1, matches + 1) if is_pending(i, distribution)),
        "treatments": {},
        "results_equivalent_and_ordered": False,
    }
    try:
        for label, path in paths.items():
            connection = initialize(path)
            populate(connection, matches, distribution)
            existing_indexes = index_names(connection)
            build_ms = build_candidate_indexes(connection) if label == "proposed" else 0.0
            treatment_indexes = index_names(connection)
            connections[label] = connection

            queue_measure = timed_query(connection, QUEUE_QUERY, (CHARACTER_ID,), repeats)
            activity_for_team_query = max(1, matches // 2)
            team_measure = timed_query(connection, TEAM_QUERY, (activity_for_team_query,), repeats)
            size = checkpoint_sizes(connection, path)
            output["treatments"][label] = {
                "candidate_index_build_ms": build_ms,
                "indexes": sorted(treatment_indexes),
                "queue_query": queue_measure,
                "queue_query_plan": query_plan(connection, QUEUE_QUERY, (CHARACTER_ID,)),
                "team_result_query": team_measure,
                "team_result_query_plan": query_plan(connection, TEAM_QUERY, (activity_for_team_query,)),
                "database_size_after_checkpoint": size,
                "indexes_before_candidate_build": sorted(existing_indexes),
            }
        baseline = connections["baseline"]
        proposed = connections["proposed"]
        queue_baseline = baseline.execute(QUEUE_QUERY, (CHARACTER_ID,)).fetchall()
        queue_proposed = proposed.execute(QUEUE_QUERY, (CHARACTER_ID,)).fetchall()
        team_activity = max(1, matches // 2)
        teams_baseline = baseline.execute(TEAM_QUERY, (team_activity,)).fetchall()
        teams_proposed = proposed.execute(TEAM_QUERY, (team_activity,)).fetchall()
        if queue_baseline != queue_proposed or teams_baseline != teams_proposed:
            raise AssertionError(f"query result/order mismatch for {matches=} {distribution=}")
        baseline_indexes = index_names(baseline)
        proposed_indexes = index_names(proposed)
        baseline_existing = set(output["treatments"]["baseline"]["indexes_before_candidate_build"])
        proposed_existing = set(output["treatments"]["proposed"]["indexes_before_candidate_build"])
        added_indexes = proposed_indexes - proposed_existing
        removed_indexes = proposed_existing - proposed_indexes
        if (
            baseline_existing != proposed_existing
            or baseline_indexes != baseline_existing
            or added_indexes != set(CANDIDATE_INDEXES)
            or removed_indexes
        ):
            raise AssertionError(
                "treatments must start with equal index sets and proposed must preserve them, adding only candidates; "
                f"added={sorted(added_indexes)}, removed={sorted(removed_indexes)}"
            )
        output["index_set_change"] = {
            "existing_indexes_unchanged": True,
            "added_indexes": sorted(added_indexes),
            "removed_indexes": sorted(removed_indexes),
        }
        output["results_equivalent_and_ordered"] = True
    finally:
        for connection in connections.values():
            connection.close()
    return output


def write_once(
    source_path: Path,
    output_path: Path,
    matches: int,
    proposed: bool,
    write_rows: int,
) -> float:
    shutil.copyfile(source_path, output_path)
    connection = connect(output_path)
    connection.execute("PRAGMA journal_mode = WAL")
    if proposed:
        build_candidate_indexes(connection)
    started = time.perf_counter()
    with connection:
        connection.executemany(
            "INSERT INTO activity(activity_id, period, mode, platform, director_activity_hash, reference_id) "
            "VALUES (?, '2026-02-01T00:00:00Z', 1, 1, 1, 1)",
            ((matches + index,) for index in range(1, write_rows + 1)),
        )
        connection.executemany(
            "INSERT INTO activity_queue(synced, activity_id, character) VALUES (0, ?, ?)",
            ((matches + index, CHARACTER_ID) for index in range(1, write_rows + 1)),
        )
        connection.executemany(
            "INSERT INTO team_result(team_id, activity, score, standing) VALUES (?, ?, ?, ?)",
            ((team, matches + index, 1000 - team * 25, team)
             for index in range(1, write_rows + 1)
             for team in range(1, TEAMS_PER_ACTIVITY + 1)),
        )
    elapsed_ms = (time.perf_counter() - started) * 1000
    connection.close()
    return elapsed_ms


def measure_write_cost(
    directory: Path, matches: int, replicates: int, write_rows: int
) -> dict[str, object]:
    fixture = directory / f"write-fixture-{matches}.sqlite"
    connection = initialize(fixture)
    populate(connection, matches, "empty_queue")
    connection.execute("PRAGMA wal_checkpoint(TRUNCATE)").fetchall()
    connection.close()
    results: dict[str, list[float]] = {"baseline": [], "proposed": []}
    for replicate in range(replicates):
        treatments = ((False, "baseline"), (True, "proposed"))
        if replicate % 2:
            treatments = tuple(reversed(treatments))
        for proposed, label in treatments:
            path = directory / f"write-{matches}-{label}-{replicate}.sqlite"
            results[label].append(write_once(fixture, path, matches, proposed, write_rows))
            path.unlink(missing_ok=True)
            Path(str(path) + "-wal").unlink(missing_ok=True)
            Path(str(path) + "-shm").unlink(missing_ok=True)
    fixture.unlink(missing_ok=True)
    return {
        "matches": matches,
        "transaction_rows": {
            "activity": write_rows,
            "activity_queue": write_rows,
            "team_result": write_rows * TEAMS_PER_ACTIVITY,
        },
        "replicates": replicates,
        "elapsed_ms": {
            label: {"samples": samples, "median": statistics.median(samples)}
            for label, samples in results.items()
        },
    }


def run(sizes: list[int], repeats: int, write_replicates: int, write_rows: int) -> dict[str, object]:
    if repeats < 1 or write_replicates < 1 or write_rows < 1:
        raise ValueError("repeats, write-replicates, and write-rows must all be positive")
    scenarios: list[dict[str, object]] = []
    write_cost: list[dict[str, object]] = []
    with tempfile.TemporaryDirectory(prefix="dcli-index-benchmark-") as temp:
        directory = Path(temp)
        for matches in sizes:
            if matches < 1:
                raise ValueError("match counts must be positive")
            for distribution in ("mostly_complete", "full_pending", "empty_queue"):
                scenarios.append(measure_scenario(directory, matches, distribution, repeats))
            write_cost.append(measure_write_cost(directory, matches, write_replicates, write_rows))
    return {
        "benchmark": "dcli activity-store index comparison",
        "sqlite_version": sqlite3.sqlite_version,
        "python_version": __import__("platform").python_version(),
        "schema": SCHEMA.name,
        "candidate_indexes": list(INDEX_DDL),
        "timing_note": "First query is labelled non-cold; OS/SQLite caches are not portably flushed. Warm timings exclude that execution.",
        "memory_note": "Queue materialized_result_bytes estimates Python fetchall list/tuple/value object sizes, not peak RSS or Rust client memory. Full-pending scenarios materialize every pending row.",
        "scenarios": scenarios,
        "write_cost": write_cost,
        "unmeasured": [
            "Rust/FFI call overhead, network/device performance, full app workloads, and peak RSS are outside this SQLite-only harness."
        ],
    }


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--sizes", nargs="+", type=int, default=[10000, 100000])
    parser.add_argument("--repeats", type=int, default=15, help="warm query executions per query and treatment")
    parser.add_argument("--write-replicates", type=int, default=3)
    parser.add_argument("--write-rows", type=int, default=1000, help="new activities per write-cost transaction")
    args = parser.parse_args()
    print(json.dumps(run(args.sizes, args.repeats, args.write_replicates, args.write_rows), indent=2))


if __name__ == "__main__":
    main()
