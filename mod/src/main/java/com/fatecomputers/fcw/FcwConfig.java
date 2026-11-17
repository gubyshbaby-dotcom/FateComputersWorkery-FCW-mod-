package com.fatecomputers.fcw;

import net.neoforged.neoforge.common.ModConfigSpec;

import java.util.Arrays;
import java.util.List;
import java.util.regex.Pattern;

/**
 * Server config (per world, {@code serverconfig/fcw-server.toml}) and client config
 * ({@code config/fcw-client.toml}).
 */
public final class FcwConfig {
    private FcwConfig() {
    }

    public static final class Server {
        public final ModConfigSpec.BooleanValue daemonEnabled;
        public final ModConfigSpec.ConfigValue<String> mode;
        public final ModConfigSpec.ConfigValue<String> remoteAddress;
        public final ModConfigSpec.ConfigValue<String> remoteToken;
        public final ModConfigSpec.ConfigValue<String> daemonPath;
        public final ModConfigSpec.ConfigValue<String> qemuPath;
        public final ModConfigSpec.IntValue quicPort;
        public final ModConfigSpec.ConfigValue<String> publicHost;
        public final ModConfigSpec.IntValue publicPort;
        public final ModConfigSpec.IntValue maxVms;
        public final ModConfigSpec.ConfigValue<String> accelerator;
        public final ModConfigSpec.IntValue startupTimeoutSeconds;
        public final ModConfigSpec.IntValue shutdownTimeoutSeconds;
        public final ModConfigSpec.IntValue messagesPerTick;
        public final ModConfigSpec.BooleanValue qemuAutoDownload;
        public final ModConfigSpec.ConfigValue<String> qemuUrlTemplate;
        public final ModConfigSpec.ConfigValue<List<? extends String>> qemuPlatforms;

        public final ModConfigSpec.ConfigValue<String> imageUrlTemplate;

        public final ModConfigSpec.ConfigValue<String> guestLang;
        public final ModConfigSpec.ConfigValue<String> guestKeyboardLayouts;
        public final ModConfigSpec.BooleanValue guestNetwork;
        public final ModConfigSpec.BooleanValue internet;
        public final ModConfigSpec.IntValue rateLimitKiBps;
        public final ModConfigSpec.ConfigValue<List<? extends String>> netAllow;
        public final ModConfigSpec.ConfigValue<List<? extends String>> netDeny;
        public final ModConfigSpec.IntValue maxMemoryMb;
        public final ModConfigSpec.IntValue displayWidth;
        public final ModConfigSpec.IntValue displayHeight;
        public final ModConfigSpec.BooleanValue restartRunningOnLoad;
        public final ModConfigSpec.BooleanValue updateGuestAgent;

        public final ModConfigSpec.IntValue linkRadius;
        public final ModConfigSpec.IntValue viewDistance;
        public final ModConfigSpec.BooleanValue ownerOnlyControl;
        public final ModConfigSpec.IntValue tokenTtlSeconds;

        public final ModConfigSpec.BooleanValue productionRequirePower;
        public final ModConfigSpec.DoubleValue productionSpeedMultiplier;
        public final ModConfigSpec.IntValue routerRadius;
        public final ModConfigSpec.IntValue machineRequestsPerSecond;

        public final ModConfigSpec.ConfigValue<String> onPowerLoss;

        Server(ModConfigSpec.Builder b) {
            b.comment("VM daemon (native fcw-daemon process started by the server)").push("daemon");
            daemonEnabled = b.comment("Use the VM daemon (start it, or connect to it in remote mode).")
                    .define("enabled", true);
            mode = b.comment("local  = the server starts its own fcw-daemon (needs QEMU and, for speed, /dev/kvm in this container/host).",
                            "remote = connect to a daemon running as a system service on the host (Pelican/Pterodactyl, where",
                            "         containers get no /dev/kvm). Install it on the node with the script from native/packaging,",
                            "         then set remoteAddress and remoteToken (the token file written by that script).")
                    .defineInList("mode", "local", Arrays.asList("local", "remote"));
            remoteAddress = b.comment("remote mode: host:port of the daemon's control channel (--control-tcp). On Pelican/Pterodactyl",
                            "the Docker host is usually reachable from the container as 172.18.0.1.")
                    .define("remoteAddress", "172.18.0.1:25700");
            remoteToken = b.comment("remote mode: the shared secret from the daemon's --token-file.")
                    .define("remoteToken", "");
            daemonPath = b.comment("Path to an fcw-daemon executable. Empty = the one bundled in the mod jar for this platform.")
                    .define("path", "");
            qemuPath = b.comment("Path to qemu-system-x86_64. Empty = $FCW_QEMU, then the portable QEMU in <gameDir>/fcw/qemu,",
                            "then qemu-system-x86_64 from PATH.")
                    .define("qemuPath", "");
            quicPort = b.comment("UDP port the daemon's QUIC server listens on (screens, input and sound go directly to it).")
                    .defineInRange("quicPort", 25600, 1, 65535);
            publicHost = b.comment("Host name or IP that clients use to reach the QUIC port.",
                            "Empty = the address the client used to join this Minecraft server (127.0.0.1 in single player).",
                            "Behind NAT / frp / a proxy: the public address of the machine that forwards UDP to this server.")
                    .define("publicHost", "");
            publicPort = b.comment("UDP port that clients connect to, if it differs from quicPort",
                            "(port forwarding or frp remotePort). 0 = same as quicPort.")
                    .defineInRange("publicPort", 0, 0, 65535);
            maxVms = b.comment("Maximum number of virtual machines running at the same time.")
                    .defineInRange("maxVms", 4, 1, 256);
            accelerator = b.comment("Virtualization accelerator: auto, kvm, whpx, hvf or tcg (software).")
                    .defineInList("accelerator", "auto", Arrays.asList("auto", "kvm", "whpx", "hvf", "tcg")); // List.of rejects contains(null)
            startupTimeoutSeconds = b.comment("How long to wait for the daemon to report FCW-READY.")
                    .defineInRange("startupTimeoutSeconds", 30, 5, 600);
            shutdownTimeoutSeconds = b.comment("How long to wait for the daemon to stop its VMs when the server stops",
                            "(remote mode: the soft shutdown time given to this world's machines; the daemon keeps running).")
                    .defineInRange("shutdownTimeoutSeconds", 15, 1, 300);
            messagesPerTick = b.comment("Maximum number of daemon replies/events applied per server tick.")
                    .defineInRange("messagesPerTick", 64, 1, 10000);
            qemuAutoDownload = b.comment("If the daemon finds no QEMU, download the portable one into <gameDir>/fcw/qemu",
                            "(for hosts where packages cannot be installed, e.g. Pelican/Pterodactyl containers).")
                    .define("qemuAutoDownload", true);
            qemuUrlTemplate = b.comment("Download URL of the portable QEMU; <platform> is replaced by e.g. linux-x86_64.")
                    .define("qemuUrlTemplate", "https://github.com/gubyshbaby-dotcom/FateComputersWorkery-FCW-mod-/releases/download/qemu/fcw-qemu-<platform>.tar.zst");
            qemuPlatforms = b.comment("Platforms for which a portable QEMU archive is published.")
                    .defineListAllowEmpty("qemuPlatforms", List.of("linux-x86_64", "linux-aarch64"), () -> "linux-x86_64",
                            o -> o instanceof String str && !str.isBlank());
            b.pop();

            b.comment("Guest OS images").push("images");
            imageUrlTemplate = b.comment("Download URL for /fcw images fetch <id>; <id> is replaced by the image id.")
                    .define("urlTemplate", "https://github.com/gubyshbaby-dotcom/FateComputersWorkery-FCW-mod-/releases/download/images/fcw-image-<id>.tar.zst");
            b.pop();

            b.comment("Virtual machines").push("vm");
            guestLang = b.comment("Guest locale (fcw.lang= kernel parameter).")
                    .define("lang", "ru_RU.UTF-8");
            guestKeyboardLayouts = b.comment("Guest keyboard layouts (fcw.kbd= kernel parameter).")
                    .define("keyboardLayouts", "us,ru");
            guestNetwork = b.comment("Give guests a network card on the daemon's virtual switch (DHCP, DNS <name>.mc, NAT).",
                            "Also passed to the daemon as --network on|off.")
                    .define("network", true);
            maxMemoryMb = b.comment("Upper limit for the memory of one VM, MB (installed modules above it are ignored).")
                    .defineInRange("maxMemoryMb", 8192, 128, 1048576);
            displayWidth = b.comment("Guest display width for machines with a graphics card.")
                    .defineInRange("displayWidth", 1024, 320, 7680);
            displayHeight = b.comment("Guest display height for machines with a graphics card.")
                    .defineInRange("displayHeight", 768, 200, 4320);
            restartRunningOnLoad = b.comment("Start machines that were running when the world was saved (after a server restart).")
                    .define("restartRunningOnLoad", true);
            updateGuestAgent = b.comment("Replace the fcw agent inside a guest with the one bundled in the mod jar when they differ",
                            "(new commands such as `fcw machine` reach existing machines without rebuilding the OS images).")
                    .define("updateGuestAgent", true);
            b.pop();

            b.comment("Guest network (daemon flags --internet, --net-rate-limit, --net-allow, --net-deny)").push("network");
            internet = b.comment("Let guests reach the Internet through the daemon's NAT. Local networks and the host stay blocked.")
                    .define("internet", true);
            rateLimitKiBps = b.comment("Bandwidth limit per machine, KiB/s. 0 = the daemon's default (8 MiB/s).")
                    .defineInRange("rateLimitKiBps", 8192, 0, 10_000_000);
            netAllow = b.comment("CIDR ranges guests may reach although the firewall would block them, e.g. \"192.168.1.10/32\".")
                    .defineListAllowEmpty("allow", List.of(), () -> "0.0.0.0/32", FcwConfig::isCidr);
            netDeny = b.comment("Additional CIDR ranges guests may not reach.")
                    .defineListAllowEmpty("deny", List.of(), () -> "0.0.0.0/32", FcwConfig::isCidr);
            b.pop();

            b.comment("Peripherals and access").push("access");
            linkRadius = b.comment("Radius in blocks in which monitors, keyboards, mice and speakers link to a computer.")
                    .defineInRange("linkRadius", 8, 1, 64);
            viewDistance = b.comment("Maximum distance in blocks at which a player may receive a screen view token.")
                    .defineInRange("viewDistance", 128, 8, 1024);
            ownerOnlyControl = b.comment("Only the player who placed a computer (and operators) may use its keyboard and mouse.")
                    .define("ownerOnlyControl", false);
            tokenTtlSeconds = b.comment("Lifetime of screen/input tokens handed to clients.")
                    .defineInRange("tokenTtlSeconds", 120, 10, 3600);
            b.pop();

            b.comment("Production machines (arc furnace, crystal puller, wafering cell, EUV scanner, chip line)").push("production");
            productionRequirePower = b.comment("true = machines work only on FE fed into their power port.",
                            "false = a built-in transformer powers them, so the chain can be played without energy mods.")
                    .define("requirePower", false);
            productionSpeedMultiplier = b.comment("Speed of all machine recipes: 2.0 = twice as fast, 0.5 = twice as slow.")
                    .defineInRange("speedMultiplier", 1.0, 0.01, 100.0);
            routerRadius = b.comment("Radius in blocks around a router in which machines join its data network without a cable",
                            "(measured to the machine's control panel).")
                    .defineInRange("routerRadius", 32, 1, 128);
            machineRequestsPerSecond = b.comment("How many `fcw machine` requests per second one computer may send to machines.",
                            "Requests above the limit are refused, so a script in a tight loop cannot stall the server.")
                    .defineInRange("machineRequestsPerSecond", 20, 1, 1000);
            b.pop();

            b.comment("Computers as power consumers (only with production.requirePower = true)").push("computers");
            onPowerLoss = b.comment("What happens to a running computer five seconds after it ran out of FE:",
                            "off   = hard power-off, like pulling the plug (unsaved data in the guest is lost);",
                            "pause = the virtual machine is frozen and continues when the power is back.")
                    .defineInList("onPowerLoss", "off", Arrays.asList("off", "pause"));
            b.pop();
        }
    }

    public static final class Client {
        public final ModConfigSpec.IntValue screenRenderDistance;
        public final ModConfigSpec.IntValue nearFps;
        public final ModConfigSpec.IntValue farFps;
        public final ModConfigSpec.IntValue farDistance;
        public final ModConfigSpec.IntValue controlFps;
        public final ModConfigSpec.ConfigValue<String> nativeLibraryPath;
        public final ModConfigSpec.BooleanValue audioEnabled;
        public final ModConfigSpec.DoubleValue audioVolume;
        public final ModConfigSpec.BooleanValue machineHints;

        Client(ModConfigSpec.Builder b) {
            b.comment("Live monitor screens").push("screens");
            screenRenderDistance = b.comment("Monitors farther than this (blocks) show the static model picture.")
                    .defineInRange("renderDistance", 32, 4, 256);
            nearFps = b.comment("Frame rate requested for nearby screens.")
                    .defineInRange("nearFps", 30, 1, 60);
            farFps = b.comment("Frame rate limit for distant screens.")
                    .defineInRange("farFps", 5, 1, 60);
            farDistance = b.comment("Distance in blocks from which a screen counts as distant.")
                    .defineInRange("farDistance", 12, 1, 256);
            controlFps = b.comment("Frame rate requested while you are using a computer.")
                    .defineInRange("controlFps", 60, 1, 120);
            nativeLibraryPath = b.comment("Path to the fcw_client native library. Empty = the one bundled in the mod jar.")
                    .define("nativeLibraryPath", "");
            b.pop();

            b.comment("Sound from computers through linked speakers").push("audio");
            audioEnabled = b.define("enabled", true);
            audioVolume = b.defineInRange("volume", 1.0, 0.0, 2.0);
            b.pop();

            b.comment("Production machines").push("machines");
            machineHints = b.comment("Show a line above the hotbar when aiming at a loading point of a machine:",
                            "what it is, what lies there and what a right click does.")
                    .define("hints", true);
            b.pop();
        }
    }

    private static final Pattern CIDR = Pattern.compile(
            "^(\\d{1,3}(\\.\\d{1,3}){3}/\\d{1,2}|[0-9a-fA-F:]+/\\d{1,3})$");

    static boolean isCidr(Object o) {
        return o instanceof String s && CIDR.matcher(s.trim()).matches();
    }

    public static final Server SERVER;
    public static final ModConfigSpec SERVER_SPEC;
    public static final Client CLIENT;
    public static final ModConfigSpec CLIENT_SPEC;

    static {
        ModConfigSpec.Builder sb = new ModConfigSpec.Builder();
        SERVER = new Server(sb);
        SERVER_SPEC = sb.build();
        ModConfigSpec.Builder cb = new ModConfigSpec.Builder();
        CLIENT = new Client(cb);
        CLIENT_SPEC = cb.build();
    }
}
