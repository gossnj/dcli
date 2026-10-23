package com.ottercreeksoftware.lastbanner.data.dcli;

import android.database.sqlite.SQLiteDatabase;
import java.io.File;
import java.util.ArrayList;
import java.util.List;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.atomic.AtomicReference;

/** Independent Java consumer of the real Android JNI ABI, using disposable SQLite stores. */
public final class DcliStore {
    public interface ProgressListener { void onProgress(long[] event); }
    private static native long nativeInit(String directory);
    private static native void nativeFree(long store);
    private static native boolean nativeClose(long store);
    private static native long nativeCreateSyncControl();
    private static native void nativeCancelSync(long control);
    private static native boolean nativeSyncControlIsCancelled(long control);
    private static native int nativeSyncControlErrorCode(long control);
    private static native void nativeFreeSyncControl(long control);
    private static native long[] nativeSyncPlayerV1(long store, String name, long control, ProgressListener listener);
    private static native long[] nativeSyncMemberSliceV1(long store, long member, int platform, long budgetMillis,
                                                       long control, ProgressListener listener);
    private static native boolean nativeBackfillScoreboardValues(long store);

    private interface Check { void run(File directory) throws Exception; }
    private static void require(boolean condition, String message) {
        if (!condition) throw new AssertionError(message);
    }
    private static long open(File directory) {
        directory.mkdirs();
        try (SQLiteDatabase db = SQLiteDatabase.openOrCreateDatabase(new File(directory, "manifest.sqlite3"), null)) {
            db.execSQL("CREATE TABLE IF NOT EXISTS fixture(value INTEGER)");
        }
        long store = nativeInit(directory.getAbsolutePath());
        require(store != 0, "synthetic store must open");
        return store;
    }
    private static long control() {
        long control = nativeCreateSyncControl();
        require(control != 0, "control allocation");
        return control;
    }
    private static void snapshot(long[] event) {
        require(event != null && event.length == 7, "event has seven unsigned fields");
        require(event[0] == 1, "event version");
        for (long field : event) require(field >= 0 && field <= 0xffffffffL, "unsigned field preserved");
    }
    private static void fullCancelled(long[] result) {
        require(result != null && result.length == 9, "full result shape");
        require(result[0] == 1 && result[1] == 1 && result[2] == 5, "valid cancelled outcome");
        require((result[7] & 8) != 0 && (result[7] & 4) == 0, "cancelled never means complete");
        require(result[8] == 0, "cancellation is not an error");
    }
    private static long[] slice(long store, long control, long budget) {
        return nativeSyncMemberSliceV1(store, 123, 3, budget, control, null);
    }
    private static void legacy(File directory) {
        nativeFree(open(directory));
    }
    private static void controlLifecycle(File directory) {
        long c = control();
        try {
            require(!nativeSyncControlIsCancelled(c), "new token is not cancelled");
            require(nativeSyncControlErrorCode(c) == 0, "new token has no error");
            nativeCancelSync(c);
            nativeCancelSync(c);
            require(nativeSyncControlIsCancelled(c), "cancel is idempotent");
        } finally { nativeFreeSyncControl(c); }
    }
    private static void deferredSlice(File directory) {
        long s = open(directory);
        try {
            long c = control();
            try {
                long[] r = slice(s, c, 0);
                require(r.length == 12 && r[0] == 1 && r[1] == 1 && r[2] == 0, "zero budget defers");
                require(r[3] == 0 && r[4] == 1 && r[6] == 0 && r[7] == 0 && r[8] == 0 && r[9] == 0, "empty coverage");
                require((r[10] & 4) == 0 && r[11] == 0, "deferred is not complete or failed");
            } finally { nativeFreeSyncControl(c); }
        } finally { nativeFree(s); }
    }
    private static void errorReset(File directory) {
        long s = open(directory);
        try {
            long c = control();
            try {
                long[] failed = slice(s, c, 1);
                require(failed.length == 12 && failed[0] == 0 && failed[2] == 4 && failed[11] == 11,
                        "missing member fails with invalid-parameters category");
                require(nativeSyncControlErrorCode(c) == 11, "control captures categorized error");
                long[] next = slice(s, c, 0);
                require(next[0] == 1 && next[11] == 0 && nativeSyncControlErrorCode(c) == 0,
                        "next call clears stale error");
            } finally { nativeFreeSyncControl(c); }
        } finally { nativeFree(s); }
    }
    private static void cancelledSlice(File directory) {
        long s = open(directory);
        try {
            long c = control();
            try {
                nativeCancelSync(c);
                long[] r = slice(s, c, 1000);
                require(r.length == 12 && r[0] == 1 && r[2] == 3 && r[5] == 5, "pre-cancelled slice");
                require((r[10] & 8) != 0 && (r[10] & 4) == 0 && r[11] == 0, "cancel coverage");
            } finally { nativeFreeSyncControl(c); }
        } finally { nativeFree(s); }
    }
    private static void callbackCancellation(File directory) {
        long s = open(directory);
        try {
            long c = control();
            try {
                List<long[]> events = new ArrayList<>();
                fullCancelled(nativeSyncPlayerV1(s, "Fixture#1234", c, event -> {
                    snapshot(event);
                    events.add(event);
                    if (event[1] == 0) nativeCancelSync(c);
                }));
                require(events.size() == 2, "starting and cancelled callbacks");
                require(events.get(0)[1] == 0 && events.get(1)[1] == 5, "callback order and retained snapshot values");
                require(events.get(0) != events.get(1), "each callback owns its snapshot");
            } finally { nativeFreeSyncControl(c); }
        } finally { nativeFree(s); }
    }
    private static void concurrentCancellation(File directory) throws Exception {
        long s = open(directory);
        try {
            long c = control();
            CountDownLatch starting = new CountDownLatch(1);
            CountDownLatch cancelled = new CountDownLatch(1);
            AtomicReference<Throwable> failure = new AtomicReference<>();
            Thread canceller = new Thread(() -> {
                try {
                    require(starting.await(5, TimeUnit.SECONDS), "starting callback arrives");
                    nativeCancelSync(c);
                    require(nativeSyncControlIsCancelled(c), "cross-thread cancellation visible");
                } catch (Throwable t) { failure.set(t); }
                finally { cancelled.countDown(); }
            });
            try {
                canceller.start();
                fullCancelled(nativeSyncPlayerV1(s, "Fixture#1234", c, event -> {
                    if (event[1] == 0) {
                        starting.countDown();
                        try { require(cancelled.await(5, TimeUnit.SECONDS), "cancel must not wait for store lock"); }
                        catch (InterruptedException e) { throw new RuntimeException(e); }
                    }
                }));
                canceller.join(5000);
                require(!canceller.isAlive() && failure.get() == null, "cancellation thread finishes safely");
            } finally {
                starting.countDown();
                canceller.join(6000);
                if (!canceller.isAlive()) nativeFreeSyncControl(c);
            }
        } finally { nativeFree(s); }
    }
    private static void listenerFailure(File directory) {
        long s = open(directory);
        try {
            long c = control();
            try {
                IllegalStateException expected = new IllegalStateException("fixture listener failure");
                int[] calls = {0};
                try {
                    nativeSyncPlayerV1(s, "Fixture#1234", c, event -> { calls[0]++; throw expected; });
                    throw new AssertionError("listener failure must propagate");
                } catch (IllegalStateException actual) {
                    require(actual == expected, "original listener exception retained");
                }
                require(calls[0] == 1 && nativeSyncControlIsCancelled(c), "listener failure cancels and stops callbacks");
            } finally { nativeFreeSyncControl(c); }
        } finally { nativeFree(s); }
    }
    private static void backfill(File directory) {
        long s = open(directory);
        try {
            require(nativeBackfillScoreboardValues(s), "empty store backfill succeeds");
            require(nativeBackfillScoreboardValues(s), "backfill is idempotent");
        } finally { nativeFree(s); }
    }
    private static void closeAndReopen(File directory) {
        for (int n = 0; n < 8; n++) {
            long s = open(directory);
            boolean consumed = false;
            try {
                require(nativeClose(s), "close joins both database workers");
                consumed = true;
                try (SQLiteDatabase db = SQLiteDatabase.openDatabase(new File(directory, "dcli.sqlite3").getPath(), null, 0)) {
                    db.execSQL("CREATE TABLE IF NOT EXISTS preservation_marker(value INTEGER)");
                    if (n == 0) db.execSQL("INSERT INTO preservation_marker VALUES(42)");
                    try (android.database.Cursor cursor = db.rawQuery("SELECT value FROM preservation_marker", null)) {
                        require(cursor.moveToFirst() && cursor.getInt(0) == 42 && !cursor.moveToNext(), "reopening preserves committed data");
                    }
                }
            } finally { if (!consumed) nativeFree(s); }
        }
    }
    public static void main(String[] args) {
        System.load(args[0]);
        String[] names = {"legacy", "control", "deferred_slice", "error_reset", "cancelled_slice", "callbacks",
                          "concurrent_cancel", "listener_failure", "backfill", "close_reopen"};
        Check[] checks = {DcliStore::legacy, DcliStore::controlLifecycle, DcliStore::deferredSlice, DcliStore::errorReset,
                          DcliStore::cancelledSlice, DcliStore::callbackCancellation, DcliStore::concurrentCancellation,
                          DcliStore::listenerFailure, DcliStore::backfill, DcliStore::closeAndReopen};
        int failures = 0;
        for (int i = 0; i < names.length; i++) {
            try {
                checks[i].run(new File(args[1], names[i]));
                System.out.println("JNI_CASE " + names[i] + " PASS");
            } catch (Throwable t) {
                failures++;
                System.out.println("JNI_CASE " + names[i] + " FAIL " + t);
                t.printStackTrace(System.out);
            }
        }
        System.out.println("JNI_RESULT cases=" + names.length + " failures=" + failures);
        System.exit(failures == 0 ? 0 : 1);
    }
}
