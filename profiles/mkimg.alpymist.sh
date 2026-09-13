# Alpymist image profile for Alpine's mkimage.
#
# Shell, because mkimage sources these as shell functions — there is no other
# interface. It contains no logic beyond declaring what goes in the image; the
# decisions all live in the Rust in crates/.

profile_alpymist() {
	profile_base
	title="Alpymist"
	desc="Alpymist smoke-test image: probes the machine and reports its desktop tier"
	profile_abbrev="alpymist"
	# Without this mkimage names the file alpine-alpymist-*; the image is
	# Alpymist, and says so.
	image_name="alpymist"
	image_ext="iso"
	# Both of these, not just the extension: mkimage builds the syslinux boot
	# sections only when output_format says iso, and without them an x86 image
	# has no BIOS boot path at all — no use on the old laptops Alpymist is for,
	# and invisible on aarch64, which boots through UEFI either way.
	output_format="iso"
	arch="x86_64 aarch64"
	hostname="alpymist"
	# Mesa is needed for the EGL probe to have anything to answer with. It is
	# pulled in explicitly rather than as a dependency of alpymistctl, because
	# alpymistctl deliberately works without it.
	# font-fira-ttf carries Fira Mono, which the splash and installer draw with.
	# Without it they fall back to Denise's built-in bitmap and still read
	# correctly, including Norwegian — just more crudely.
	apks="$apks alpymistctl alpymist-install alpymist-install-openrc"
	apks="$apks mesa-egl mesa-gles mesa-dri-gallium font-fira-ttf"
	# What the installer runs, on the image rather than fetched, so installing
	# works with no network: partitioning, LUKS, filesystems and the bootloader
	# setup-disk installs into the new system.
	apks="$apks sfdisk dosfstools cryptsetup blkid grub-efi"
	# The kernel is on the image to boot it, but not in its package repository
	# unless named here, and setup-disk installs it from that repository.
	# linux-firmware-none is what setup-disk picks for hardware needing none.
	apks="$apks linux-lts linux-firmware-none"
	# Every desktop tier, so the installer can put any of them on a machine
	# with no network. About 170 MiB more on the image.
	apks="$apks alpymist-desktop-full alpymist-desktop-lite alpymist-desktop-legacy"
	case "$ARCH" in
		x86*) apks="$apks grub-bios";;
	esac
	# Boot quietly into the installer rather than into a login prompt.
	#
	# Deliberately *not* console=tty2. Redirecting the console to an unused VT
	# does hide boot messages, but it also silences the serial console — which
	# is the only way to see what a machine did when it fails to come up, and
	# the only way this image can be tested under QEMU at all. Suppression is
	# left to quiet/loglevel, and the installer blanks the text it is drawing
	# over by putting the VT into graphics mode itself.
	initfs_cmdline="$initfs_cmdline quiet loglevel=3 vt.global_cursor_default=0"
	# x86 has no device tree naming a serial console, so without this the
	# console is the screen and the serial port stays silent: nothing to debug
	# a failed boot with, and nothing for the CI boot test to read. Serial
	# last, so boot messages go there rather than under the installer.
	case "$ARCH" in
		x86*) initfs_cmdline="$initfs_cmdline console=tty0 console=ttyS0,115200";;
	esac
	apkovl="genapkovl-alpymist.sh"
}
