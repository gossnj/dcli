import concurrent.futures
from contextlib import closing
import ctypes
import json
import os
from pathlib import Path
import sqlite3
import sys
import tempfile
import unittest

LIBRARY = ctypes.CDLL(sys.argv.pop(1))
LIBRARY.dcli_store_init.argtypes = [ctypes.c_char_p]
LIBRARY.dcli_store_init.restype = ctypes.c_void_p
LIBRARY.dcli_store_free.argtypes = [ctypes.c_void_p]
LIBRARY.dcli_string_free.argtypes = [ctypes.c_void_p]
LIBRARY.dcli_get_last_error.restype = ctypes.c_void_p
CHECKED_CLOSE = getattr(LIBRARY, 'dcli_store_close', None)
if CHECKED_CLOSE:
    CHECKED_CLOSE.argtypes = [ctypes.c_void_p]
    CHECKED_CLOSE.restype = ctypes.c_bool
DIAGNOSTIC_INIT = getattr(LIBRARY, 'dcli_store_init_with_diagnostics', None)
if DIAGNOSTIC_INIT:
    DIAGNOSTIC_INIT.argtypes = [ctypes.c_char_p, ctypes.POINTER(ctypes.c_void_p)]
    DIAGNOSTIC_INIT.restype = ctypes.c_void_p


def initialize(directory):
    output = ctypes.c_void_p()
    if DIAGNOSTIC_INIT:
        store = DIAGNOSTIC_INIT(str(directory).encode(), ctypes.byref(output))
    else:
        store = LIBRARY.dcli_store_init(str(directory).encode())
        output = ctypes.c_void_p(LIBRARY.dcli_get_last_error())
    succeeded = bool(store)
    if store:
        LIBRARY.dcli_store_free(store)
    try:
        report = json.loads(ctypes.string_at(output).decode()) if output.value else None
    finally:
        if output.value:
            LIBRARY.dcli_string_free(output)
    return succeeded, report


def activity(directory, queue=True):
    with closing(sqlite3.connect(directory / 'dcli.sqlite3')) as db, db:
        db.executescript('''CREATE TABLE version(version INTEGER); INSERT INTO version VALUES(10);
        CREATE TABLE scoreboard_result(activity INTEGER);
        CREATE TABLE team_result(activity INTEGER);
        CREATE TABLE preservation_marker(value TEXT); INSERT INTO preservation_marker VALUES('unchanged');''')
        if queue:
            db.execute('CREATE TABLE activity_queue(character INTEGER, activity_id INTEGER, synced INTEGER)')


def manifest(directory):
    with closing(sqlite3.connect(directory / 'manifest.sqlite3')) as db, db:
        db.execute('CREATE TABLE fixture(value INTEGER)')


class NativeInitializationDiagnosticsTests(unittest.TestCase):
    def validate(self, report, directory, success):
        self.assertIsNotNone(report, 'Initialization must return diagnostic context with its result')
        self.assertEqual(set(report), {'version', 'success', 'source_revision', 'schema_version_before', 'events', 'cleanup_verified'})
        self.assertIsInstance(report['cleanup_verified'], bool)
        self.assertEqual(report['version'], 1)
        self.assertEqual(report['success'], success)
        self.assertLessEqual(len(report['events']), 20)
        self.assertNotIn(str(directory), json.dumps(report))
        self.assertNotIn('preservation_marker', json.dumps(report))
        for event in report['events']:
            self.assertEqual(set(event), {'role', 'stage', 'outcome', 'elapsed_ms', 'category', 'sqlite_code'})
            self.assertIn(event['role'], ['runtime', 'activity', 'manifest'])
            self.assertIn(event['outcome'], ['ok', 'error'])
            self.assertGreaterEqual(event['elapsed_ms'], 0)
        return report

    def test_missing_manifest_identified_separately(self):
        with tempfile.TemporaryDirectory() as name:
            directory = Path(name)
            activity(directory)
            ok, report = initialize(directory)
            self.assertFalse(ok)
            report = self.validate(report, directory, False)
            failure = next(e for e in report['events'] if e['outcome'] == 'error')
            self.assertEqual((failure['role'], failure['stage'], failure['category']), ('manifest', 'manifest_exists', 'missing_file'))
            self.assertIsNone(failure['sqlite_code'])
            self.assertEqual(report['schema_version_before'], 10)
            self.assertTrue(report['cleanup_verified'])

    def test_index_failure_keeps_both_database_errors_and_data(self):
        with tempfile.TemporaryDirectory() as name:
            directory = Path(name)
            activity(directory, queue=False)
            ok, report = initialize(directory)
            self.assertFalse(ok)
            report = self.validate(report, directory, False)
            self.assertTrue(report['cleanup_verified'])
            errors = [e for e in report['events'] if e['outcome'] == 'error']
            self.assertEqual([(e['role'], e['stage']) for e in errors], [('activity', 'pending_index'), ('manifest', 'manifest_exists')])
            self.assertEqual(errors[0]['sqlite_code'] & 255, 1)
            with closing(sqlite3.connect(directory / 'dcli.sqlite3')) as db, db:
                self.assertEqual(db.execute('SELECT value FROM preservation_marker').fetchone()[0], 'unchanged')

    def test_corrupt_activity_preserves_sqlite_code(self):
        with tempfile.TemporaryDirectory() as name:
            directory = Path(name)
            (directory / 'dcli.sqlite3').write_bytes(b'not sqlite: private player#1234')
            manifest(directory)
            ok, report = initialize(directory)
            self.assertFalse(ok)
            report = self.validate(report, directory, False)
            self.assertFalse(report['cleanup_verified'])
            errors = [e for e in report['events'] if e['outcome'] == 'error' and e['role'] == 'activity']
            self.assertTrue(any(e['sqlite_code'] is not None and e['sqlite_code'] & 255 in (11, 26) for e in errors))
            self.assertNotIn('player#1234', json.dumps(report))
            self.assertTrue(any(e['role'] == 'manifest' and e['stage'] == 'open' and e['outcome'] == 'ok' for e in report['events']))

    def test_schema_read_fallback_and_reopen_behavior(self):
        with tempfile.TemporaryDirectory() as name:
            directory = Path(name)
            manifest(directory)
            ok, first = initialize(directory)
            self.assertTrue(ok)
            first = self.validate(first, directory, True)
            self.assertTrue(any(e['stage'] == 'schema_read' and e['outcome'] == 'error' and e['sqlite_code'] & 255 == 1 for e in first['events']))
            self.assertTrue(any(e['stage'] == 'schema_apply' and e['outcome'] == 'ok' for e in first['events']))
            ok, second = initialize(directory)
            self.assertTrue(ok)
            second = self.validate(second, directory, True)
            self.assertTrue(first['cleanup_verified'])
            self.assertTrue(second['cleanup_verified'])
            self.assertEqual(second['schema_version_before'], 10)
            self.assertFalse(any(e['stage'] == 'schema_apply' for e in second['events']))

    def test_corrupt_manifest_and_schema_failure_are_distinct(self):
        with tempfile.TemporaryDirectory() as name:
            directory = Path(name)
            activity(directory)
            (directory / 'manifest.sqlite3').write_bytes(b'not a SQLite database')
            ok, report = initialize(directory)
            self.assertFalse(ok)
            report = self.validate(report, directory, False)
            failures = [e for e in report['events'] if e['outcome'] == 'error']
            self.assertFalse(report['cleanup_verified'])
            self.assertEqual(failures[0]['role'], 'manifest')
            self.assertEqual(failures[0]['stage'], 'open')
            self.assertIn(failures[0]['sqlite_code'] & 255, (11, 26))
        with tempfile.TemporaryDirectory() as name:
            directory = Path(name)
            manifest(directory)
            with closing(sqlite3.connect(directory / 'dcli.sqlite3')) as db, db:
                db.executescript('CREATE TABLE version(version INTEGER); INSERT INTO version VALUES(9); CREATE VIEW activity_queue AS SELECT 1;')
            ok, report = initialize(directory)
            self.assertFalse(ok)
            report = self.validate(report, directory, False)
            self.assertTrue(report['cleanup_verified'])
            self.assertEqual(report['schema_version_before'], 9)
            self.assertTrue(any(e['stage'] == 'schema_apply' and e['outcome'] == 'error' and e['sqlite_code'] & 255 == 1 for e in report['events']))

    def test_concurrent_results_do_not_share_error_state(self):
        with tempfile.TemporaryDirectory() as left, tempfile.TemporaryDirectory() as right:
            a, b = Path(left), Path(right)
            activity(a)
            activity(b, queue=False)
            manifest(b)
            with concurrent.futures.ThreadPoolExecutor(max_workers=2) as executor:
                first, second = list(executor.map(initialize, [a, b]))
            for directory, (ok, report), expected in [(a, first, 'manifest'), (b, second, 'activity')]:
                self.assertFalse(ok)
                report = self.validate(report, directory, False)
                self.assertEqual({e['role'] for e in report['events'] if e['outcome'] == 'error'}, {expected})


def open_descriptors(directory):
    identities = {(path.stat().st_dev, path.stat().st_ino)
                  for path in directory.iterdir() if path.name in ('dcli.sqlite3', 'manifest.sqlite3')}
    descriptor_directory = Path('/dev/fd') if Path('/dev/fd').is_dir() else Path('/proc/self/fd')
    count = 0
    for entry in descriptor_directory.iterdir():
        try:
            info = os.fstat(int(entry.name))
        except (OSError, ValueError):
            continue
        count += (info.st_dev, info.st_ino) in identities
    return count


class NativeStoreLifecycleTests(unittest.TestCase):
    def test_checked_close_returns_after_both_connections_close(self):
        self.assertIsNotNone(CHECKED_CLOSE, 'The native ABI must expose checked close')
        with tempfile.TemporaryDirectory() as name:
            directory = Path(name)
            activity(directory)
            manifest(directory)
            with (directory / 'dcli.sqlite3').open('rb'):
                self.assertEqual(open_descriptors(directory), 1)
            for _ in range(25):
                store = LIBRARY.dcli_store_init(str(directory).encode())
                self.assertTrue(store)
                self.assertGreaterEqual(open_descriptors(directory), 2)
                self.assertTrue(CHECKED_CLOSE(store))
                self.assertEqual(open_descriptors(directory), 0,
                                 'Successful close must be a barrier, without sleep or polling')

    def test_failed_initialization_releases_partial_connections(self):
        for missing_manifest in [True, False]:
            with self.subTest(missing_manifest=missing_manifest):
                for _ in range(10):
                    with tempfile.TemporaryDirectory() as name:
                        directory = Path(name)
                        activity(directory, queue=missing_manifest)
                        if not missing_manifest:
                            manifest(directory)
                        ok, report = initialize(directory)
                        self.assertFalse(ok)
                        self.assertTrue(report['cleanup_verified'])
                        self.assertEqual(open_descriptors(directory), 0)

    def test_checked_close_accepts_null(self):
        self.assertIsNotNone(CHECKED_CLOSE)
        self.assertTrue(CHECKED_CLOSE(None))


if __name__ == '__main__':
    unittest.main()
