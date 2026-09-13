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
	apks="$apks sfdisk dosfstools cryptsetup grub-efi"
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
	apkovl="genapkovl-alpymist.sh"
}
