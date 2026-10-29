package dev.horizon.android;

import android.app.Notification;
import android.app.NotificationChannel;
import android.app.NotificationManager;
import android.app.PendingIntent;
import android.app.Service;
import android.content.Intent;
import android.content.pm.ServiceInfo;
import android.os.Build;
import android.os.IBinder;
import android.os.PowerManager;
import java.util.Locale;

/** Android lifecycle bridge only; downloads, verification and preparation remain in Rust. */
public final class InstallationService extends Service {
    private PowerManager.WakeLock wake;
    private static native void cancelInstallation();

    @Override public void onCreate() {
        super.onCreate();
        System.loadLibrary("horizon_android");
        String lang = Locale.getDefault().getLanguage();
        String title = "Horizon · Installation";
        String text = "Downloading and preparing game files";
        if ("es".equals(lang)) {
            title = "Horizon · Instalación";
            text = "Descargando y preparando archivos del juego";
        } else if ("pt".equals(lang)) {
            title = "Horizon · Instalação";
            text = "Baixando e preparando arquivos do jogo";
        }
        NotificationManager manager = getSystemService(NotificationManager.class);
        manager.createNotificationChannel(new NotificationChannel(
            "installation", title, NotificationManager.IMPORTANCE_LOW));
        Intent launch = getPackageManager().getLaunchIntentForPackage(getPackageName());
        PendingIntent open = PendingIntent.getActivity(this, 0, launch,
            PendingIntent.FLAG_UPDATE_CURRENT | PendingIntent.FLAG_IMMUTABLE);
        Notification notification = new Notification.Builder(this, "installation")
            .setSmallIcon(android.R.drawable.stat_sys_download)
            .setContentTitle(title).setContentText(text).setContentIntent(open)
            .setCategory(Notification.CATEGORY_PROGRESS).setOngoing(true)
            .setProgress(0, 0, true).build();
        if (Build.VERSION.SDK_INT >= 29) {
            startForeground(54, notification, ServiceInfo.FOREGROUND_SERVICE_TYPE_DATA_SYNC);
        } else {
            startForeground(54, notification);
        }
        wake = getSystemService(PowerManager.class).newWakeLock(
            PowerManager.PARTIAL_WAKE_LOCK, "Horizon:installation");
        wake.acquire(6L * 60 * 60 * 1000);
    }
    @Override public int onStartCommand(Intent intent, int flags, int id) {
        // Never restart without a live, authenticated native installer.
        return START_NOT_STICKY;
    }
    @Override public IBinder onBind(Intent intent) { return null; }
    @Override public void onTimeout(int id, int type) {
        cancelInstallation();
        stopSelf();
    }
    @Override public void onDestroy() {
        if (wake != null && wake.isHeld()) wake.release();
        stopForeground(STOP_FOREGROUND_REMOVE);
        super.onDestroy();
    }
}
