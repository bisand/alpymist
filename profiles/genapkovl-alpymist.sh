#!/bin/sh -e
# Generates the Alpymist apkovl: the overlay unpacked over the running system.

HOSTNAME="$1"
if [ -z "$HOSTNAME" ]; then
	echo "usage: $0 hostname"
	exit 1
fi

cleanup() {
	rm -rf "$tmp"
}

makefile() {
	OWNER="$1"
	PERMS="$2"
	FILENAME="$3"
	cat > "$FILENAME"
	chown "$OWNER" "$FILENAME"
	chmod "$PERMS" "$FILENAME"
}

rc_add() {
	mkdir -p "$tmp"/etc/runlevels/"$2"
	ln -sf /etc/init.d/"$1" "$tmp"/etc/runlevels/"$2"/"$1"
}

tmp="$(mktemp -d)"
trap cleanup EXIT

mkdir -p "$tmp"/etc
makefile root:root 0644 "$tmp"/etc/hostname <<EOF
$HOSTNAME
EOF

mkdir -p "$tmp"/etc/network
makefile root:root 0644 "$tmp"/etc/network/interfaces <<EOF
auto lo
iface lo inet loopback

auto eth0
iface eth0 inet dhcp
	hostname $HOSTNAME
EOF

mkdir -p "$tmp"/etc/apk
# This file *replaces* the world the image was built with, so anything the
# running system needs must be listed here — not only in the mkimage profile.
# Mesa is what gives the EGL probe something to ask; without it every machine
# reports "libEGL.so.1 could not be loaded" and lands on the Potato tier.
makefile root:root 0644 "$tmp"/etc/apk/world <<EOF
alpine-base
doas
alpymist-shell
eudev
udev-init-scripts
udev-init-scripts-openrc
alpymistctl
alpymist-install
alpymist-install-openrc
mesa-egl
mesa-gles
mesa-dri-gallium
font-fira-ttf
dbus
dbus-openrc
iwd
iwd-openrc
iw
openresolv
wireless-regdb
EOF

# iwd configures the address itself once it has joined a network, and hands
# name servers to openresolv. Without this it associates and then sits there
# with no address, because nothing else runs DHCP on a wireless interface.
# The installer writes the same file into the installed system.
mkdir -p "$tmp"/etc/iwd
makefile root:root 0644 "$tmp"/etc/iwd/main.conf <<EOF
[General]
EnableNetworkConfiguration=true

[Network]
NameResolvingService=resolvconf
EOF

# evdev exposes keyboards, mice and touchpads as /dev/input/event*, which is
# the only way the installer can read them. It is a module, not built in, and
# nothing else asks for it on a system with no display server.
makefile root:root 0644 "$tmp"/etc/modules <<EOF
evdev
EOF

# Hardware probing on first boot. This is a real product service, not test
# scaffolding: the installer and the desktop metapackages both read the tier
# it records. The automated boot test just happens to assert on the same
# console output.
mkdir -p "$tmp"/etc/init.d
makefile root:root 0755 "$tmp"/etc/init.d/alpymist-probe <<'EOF'
#!/sbin/openrc-run

description="Probe the hardware and record which Alpymist desktop tier it supports"

depend() {
	need localmount
	after modules hwdrivers
}

start() {
	ebegin "Probing hardware capability"
	mkdir -p /var/lib/alpymist
	alpymistctl probe --format json > /var/lib/alpymist/tier.json
	# Markers bracket the output so an automated boot test can find it in a
	# serial log without depending on where in the boot it appeared.
	echo "=== ALPYMIST-PROBE-BEGIN ==="
	alpymistctl probe
	echo "=== ALPYMIST-PROBE-END ==="
	eend 0
}
EOF

rc_add devfs sysinit
rc_add dmesg sysinit
rc_add mdev sysinit
rc_add hwdrivers sysinit
# modloop mounts /lib/modules from the image. This overlay replaces the whole
# runlevel set, so leaving it out means only drivers built into the initramfs
# ever load — which is how the installer booted with a keyboard and a mouse
# the kernel could see and no /dev/input to read them from.
rc_add modloop sysinit

rc_add modules boot
rc_add sysctl boot
rc_add hostname boot
rc_add bootmisc boot
rc_add syslog boot
rc_add networking boot
rc_add alpymist-probe boot
# iwd talks to its clients, the installer's iwctl included, over D-Bus.
rc_add dbus default
rc_add iwd default
rc_add alpymist-install default

rc_add mount-ro shutdown
rc_add killprocs shutdown
rc_add savecache shutdown

tar -c -C "$tmp" etc | gzip -9n > "$HOSTNAME".apkovl.tar.gz
