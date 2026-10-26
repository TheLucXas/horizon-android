/* SPDX-License-Identifier: MIT
 * Original Android OS bridge for the isolated preparation development fixture.
 * It never signs in, downloads files, starts Minecraft, or accesses another app.
 */
package dev.horizon.javaprobe;

import android.app.Notification;
import android.app.NotificationChannel;
import android.app.NotificationManager;
import android.app.Service;
import android.content.Intent;
import android.os.Handler;
import android.os.IBinder;
import android.os.Looper;
import java.io.BufferedReader;
import java.io.File;
import java.io.FileReader;
import java.io.FileWriter;
import java.util.concurrent.atomic.AtomicBoolean;

public final class PreparationService extends Service {
    private static final String CHANNEL = "horizon-preparation-fixture";
    private static final int NOTIFICATION = 1701;
    private final AtomicBoolean running = new AtomicBoolean(false);
    private final Handler handler = new Handler(Looper.getMainLooper());
    private File external;
    private final Object lifecycle = new Object();
    private boolean nativeLoaded;
    private boolean destroyed;

    private static native String nativePrepare(String external, String internal, String nativeDir);
    private static native void nativeCancel();
    private static native boolean nativeReset();

    private Notification notification(String text) {
        return new Notification.Builder(this, CHANNEL)
                .setSmallIcon(android.R.drawable.stat_sys_download)
                .setContentTitle("Horizon: prueba de preparación")
                .setContentText(text)
                .setCategory(Notification.CATEGORY_PROGRESS)
                .setProgress(0, 0, true)
                .setOngoing(true)
                .setShowWhen(false)
                .build();
    }

    private void write(String name, String text) {
        if (external == null) return;
        try (FileWriter writer = new FileWriter(new File(external, name))) {
            writer.write(text);
        } catch (Exception error) {
            android.util.Log.e("HorizonPrepareProbe", "Cannot write fixture status", error);
        }
    }

    private final Runnable progress = new Runnable() {
        @Override public void run() {
            String last = "Preparando recursos de la prueba local";
            if (external != null) {
                try (BufferedReader reader = new BufferedReader(new FileReader(new File(external, "probe.log")))) {
                    String line;
                    while ((line = reader.readLine()) != null) {
                        if (!line.trim().isEmpty()) last = line;
                    }
                } catch (Exception ignored) { }
            }
            if (last.length() > 120) last = last.substring(0, 120);
            getSystemService(NotificationManager.class).notify(NOTIFICATION, notification(last));
            if (running.get()) handler.postDelayed(this, 2000);
        }
    };

    @Override public void onCreate() {
        super.onCreate();
        external = getExternalFilesDir(null);
        if (external != null) external.mkdirs();
        NotificationChannel channel = new NotificationChannel(
                CHANNEL, "Prueba de preparación local", NotificationManager.IMPORTANCE_LOW);
        getSystemService(NotificationManager.class).createNotificationChannel(channel);
        // dataSync is declared in the test manifest. No notification permission
        // or other protected permission is requested or granted by this fixture.
        startForeground(NOTIFICATION, notification("Iniciando prueba local"));
    }

    @Override public int onStartCommand(Intent intent, int flags, int startId) {
        if (!running.compareAndSet(false, true)) return START_NOT_STICKY;
        write("service-status.txt", "RUNNING\n");
        handler.post(progress);
        new Thread(new Runnable() {
            @Override public void run() {
                try {
                    if (external == null) throw new IllegalStateException("No fixture external directory");
                    System.loadLibrary("horizon_java_fixture");
                    synchronized (lifecycle) {
                        nativeLoaded = true;
                        if (destroyed) throw new IllegalStateException("Fixture service stopped");
                        if (!nativeReset()) throw new IllegalStateException("Another fixture job is active");
                    }
                    String error = nativePrepare(external.getAbsolutePath(),
                            getFilesDir().getAbsolutePath(), getApplicationInfo().nativeLibraryDir);
                    if (error != null) {
                        write("service-error.txt", error);
                        write("service-status.txt", "FAILED\n" + error + "\n");
                    } else {
                        new File(external, "service-error.txt").delete();
                        write("service-status.txt", "COMPLETED\n");
                    }
                } catch (Throwable error) {
                    write("service-error.txt", error.toString());
                    write("service-status.txt", "FAILED\n" + error + "\n");
                } finally {
                    running.set(false);
                    handler.removeCallbacks(progress);
                    stopForeground(STOP_FOREGROUND_REMOVE);
                    stopSelf();
                }
            }
        }, "Horizon preparation fixture").start();
        return START_NOT_STICKY;
    }

    @Override public void onTimeout(int startId, int fgsType) {
        synchronized (lifecycle) {
            destroyed = true;
            if (nativeLoaded) nativeCancel();
        }
        write("service-status.txt", "CANCELLED: Android foreground-service timeout\n");
        stopSelf();
    }

    @Override public void onDestroy() {
        synchronized (lifecycle) {
            destroyed = true;
            if (nativeLoaded) nativeCancel();
        }
        handler.removeCallbacks(progress);
        super.onDestroy();
    }

    @Override public IBinder onBind(Intent intent) { return null; }
}
