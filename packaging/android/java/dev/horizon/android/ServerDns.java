package dev.horizon.android;

import android.content.Context;
import android.net.ConnectivityManager;
import android.net.DnsResolver;
import android.net.LinkProperties;
import android.net.Network;
import android.os.Build;
import android.os.CancellationSignal;
import java.io.ByteArrayOutputStream;
import java.io.DataOutputStream;
import java.net.DatagramPacket;
import java.net.DatagramSocket;
import java.net.InetAddress;
import java.util.Arrays;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.atomic.AtomicReference;

/** OS DNS bridge: honor the active Android network and Private DNS, never a public fallback. */
public final class ServerDns {
    public static byte[] query(Context context, String name) {
        try {
            ByteArrayOutputStream bytes = new ByteArrayOutputStream();
            DataOutputStream out = new DataOutputStream(bytes);
            out.writeShort(new java.security.SecureRandom().nextInt(65536));
            out.writeShort(0x0100); out.writeShort(1);
            out.writeShort(0); out.writeShort(0); out.writeShort(0);
            for (String label : name.replaceAll("\\.$", "").split("\\.")) {
                byte[] encoded = label.getBytes(java.nio.charset.StandardCharsets.US_ASCII);
                if (encoded.length == 0 || encoded.length > 63) return null;
                out.writeByte(encoded.length); out.write(encoded);
            }
            out.writeByte(0); out.writeShort(33); out.writeShort(1);
            byte[] packet = bytes.toByteArray();
            if (packet.length > 512) return null;
            if (Build.VERSION.SDK_INT >= 29) return modern(packet);
            // Android 9 has no DnsResolver.rawQuery. Use only this network's DNS servers.
            ConnectivityManager manager = context.getSystemService(ConnectivityManager.class);
            Network network = manager.getActiveNetwork();
            LinkProperties properties = manager.getLinkProperties(network);
            if (network == null || properties == null) return null;
            for (InetAddress server : properties.getDnsServers()) {
                try (DatagramSocket socket = new DatagramSocket()) {
                    network.bindSocket(socket);
                    socket.setSoTimeout(1000); socket.connect(server, 53);
                    socket.send(new DatagramPacket(packet, packet.length));
                    byte[] reply = new byte[4096];
                    DatagramPacket received = new DatagramPacket(reply, reply.length);
                    socket.receive(received);
                    if (reply[0] == packet[0] && reply[1] == packet[1])
                        return Arrays.copyOf(reply, received.getLength());
                } catch (Exception ignored) { }
            }
        } catch (Exception ignored) { }
        return null;
    }
    private static byte[] modern(byte[] packet) throws InterruptedException {
        CountDownLatch ready = new CountDownLatch(1);
        AtomicReference<byte[]> answer = new AtomicReference<>();
        CancellationSignal cancel = new CancellationSignal();
        DnsResolver.getInstance().rawQuery(null, packet, 0, Runnable::run, cancel,
            new DnsResolver.Callback<byte[]>() {
                @Override public void onAnswer(byte[] reply, int code) {
                    if (code == 0 && reply.length <= 65535) answer.set(reply);
                    ready.countDown();
                }
                @Override public void onError(DnsResolver.DnsException error) { ready.countDown(); }
            });
        if (!ready.await(3, TimeUnit.SECONDS)) cancel.cancel();
        return answer.get();
    }
}
