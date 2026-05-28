package com.observalog;

import java.io.OutputStream;
import java.util.concurrent.ArrayBlockingQueue;
import java.util.concurrent.BlockingQueue;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.atomic.AtomicLong;

// Non-blocking log queue drained by a background daemon thread.
// Mirrors observalog-go/async.go.
public final class AsyncDrain {

    private static volatile BlockingQueue<byte[]> queue;
    private static volatile Thread drainThread;
    private static final AtomicLong dropCount = new AtomicLong(0);
    private static volatile boolean initialised = false;

    private AsyncDrain() {}

    public static synchronized void start(OutputStream out, int bufferSize) {
        if (initialised) return;
        queue = new ArrayBlockingQueue<>(bufferSize);
        drainThread = new Thread(() -> {
            while (!Thread.currentThread().isInterrupted()) {
                try {
                    byte[] line = queue.poll(100, TimeUnit.MILLISECONDS);
                    if (line != null) {
                        out.write(line);
                        out.flush();
                    }
                } catch (InterruptedException e) {
                    Thread.currentThread().interrupt();
                } catch (Exception ignored) {}
            }
            // Drain remaining on shutdown
            byte[] line;
            while ((line = queue.poll()) != null) {
                try { out.write(line); out.flush(); } catch (Exception ignored) {}
            }
        }, "observalog-drain");
        drainThread.setDaemon(true);
        drainThread.start();
        initialised = true;
    }

    // Non-blocking send. Drops and counts if queue full.
    public static void send(byte[] line) {
        if (!queue.offer(line)) {
            dropCount.incrementAndGet();
        }
    }

    public static void shutdown() {
        if (drainThread != null) {
            drainThread.interrupt();
            try { drainThread.join(5000); } catch (InterruptedException ignored) {}
        }
    }

    public static long droppedCount() {
        return dropCount.get();
    }
}
