# Alpymist image profile for Alpine's mkimage.
#
# Shell, because mkimage sources these as shell functions — there is no other
# interface. It contains no logic beyond declaring what goes in the image; the
# decisions all live in the Rust in crates/.

# The boot menus' backgrounds, onto the image.
#
# Taken out of alpymist-splash-boot rather than carried here as files: that
# subpackage exists only for this, and it is built by the same code that draws
# the splash the menu hands over to, so the two cannot drift apart. Nothing
# installs it — `apk fetch` takes it straight from the repository.
#
# The GRUB font comes along because gfxterm draws no text without one and
# Alpine's mkimage installs none.
section_alpymist_boot() {
	build_section alpymist_boot \
		$(apk fetch --root "$APKROOT" --simulate alpymist-splash-boot grub \
			| sort | checksum)
}

build_alpymist_boot() {
	mkdir -p "$DESTDIR"/boot/grub/fonts
	apk fetch --root "$APKROOT" --stdout alpymist-splash-boot \
		| tar -C "$DESTDIR" -xz usr/share/alpymist/boot
	apk fetch --root "$APKROOT" --stdout grub \
		| tar -C "$DESTDIR" -xz usr/share/grub/unicode.pf2
	mv "$DESTDIR"/usr/share/alpymist/boot "$DESTDIR"/boot/alpymist
	mv "$DESTDIR"/usr/share/grub/unicode.pf2 "$DESTDIR"/boot/grub/fonts/
	rm -rf "$DESTDIR"/usr
	# Loudly, rather than shipping a menu that points at nothing.
	for _fn in menu-640.png menu-1280.png; do
		[ -f "$DESTDIR"/boot/alpymist/$_fn ] \
			|| die "alpymist-splash-boot has no $_fn"
	done
	[ -f "$DESTDIR"/boot/grub/fonts/unicode.pf2 ] || die "no GRUB font"
}

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
	# pulled in explicitly rather than as a dependency of alpymist, because
	# alpymist deliberately works without it.
	# font-fira-ttf carries Fira Mono, which the splash and installer draw with.
	# Without it they fall back to Denise's built-in bitmap and still read
	# correctly, including Norwegian — just more crudely.
	apks="$apks alpymist alpymist-install alpymist-install-openrc"
	apks="$apks alpymist-splash alpymist-splash-openrc"
	apks="$apks mesa-egl mesa-gles mesa-dri-gallium font-fira-ttf"
	# What the installer runs, on the image rather than fetched, so installing
	# works with no network: partitioning, LUKS, filesystems and the bootloader
	# setup-disk installs into the new system.
	apks="$apks sfdisk dosfstools cryptsetup blkid grub-efi"
	# The kernel is on the image to boot it, but not in its package repository
	# unless named here, and setup-disk installs it from that repository.
	# linux-firmware-none is what setup-disk picks for hardware needing none.
	apks="$apks linux-lts linux-firmware-none"
	# Wi-Fi: iwd, which the installer and the desktop both drive, and the
	# regulatory database every wireless driver asks for. The live system's
	# firmware is the kernel's modloop, which has all of it; these packages are
	# for the installed system, whose firmware is picked from this repository by
	# the drivers loaded. All of linux-firmware would add about 760 MiB, so the
	# image carries the common laptop Wi-Fi and Intel graphics firmware, about
	# 40 MiB, and the installer fetches the rest when it has a network.
	apks="$apks iwd iwd-openrc impala iw openresolv wireless-regdb"
	apks="$apks linux-firmware-rtlwifi linux-firmware-rtw88 linux-firmware-rtw89"
	apks="$apks linux-firmware-brcm linux-firmware-cypress"
	apks="$apks linux-firmware-ath9k_htc linux-firmware-ath10k linux-firmware-i915"
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

	_alpymist_boot_menus
}

# Put the mark on the boot menus, BIOS and UEFI both.
#
# These replace mkimg.base.sh's own generators, and they are defined inside a
# function rather than at the top of this file on purpose: mkimage.sh sources
# every mkimg.*.sh in turn and "alpymist" sorts before "base", so anything
# defined at the top level here is overwritten again a moment later. A
# definition that runs from profile_alpymist happens after all the sourcing,
# and before the sections are built.
#
# Only provable in CI or under QEMU: nothing here can be exercised on a
# development machine.
_alpymist_boot_menus() {
	# GRUB decodes nothing it was not built with, and draws no text without
	# gfxterm and a font.
	grub_mod="$grub_mod png font gfxterm"

	# vesamenu.c32 is what draws a background at all; base unpacks only the
	# text-mode modules. Anything missing from the package is skipped rather
	# than failing the build — a text menu is a far better outcome than no
	# image.
	build_syslinux() {
		local _fn
		mkdir -p "$DESTDIR"/boot/syslinux
		apk fetch --root "$APKROOT" --stdout syslinux \
			| tar -C "$DESTDIR" -xz usr/share/syslinux
		for _fn in isohdpfx.bin isolinux.bin ldlinux.c32 libutil.c32 \
			libcom32.c32 mboot.c32 vesamenu.c32 libmenu.c32 libgpl.c32; do
			[ -e "$DESTDIR"/usr/share/syslinux/$_fn ] || continue
			mv "$DESTDIR"/usr/share/syslinux/$_fn "$DESTDIR"/boot/syslinux/$_fn
		done
		rm -rf "$DESTDIR"/usr
	}

	# mkimage keys the syslinux and grub-efi sections on their package lists,
	# not on what is built from them, so overriding the builders would be
	# stale in a warm workdir. Ours is never warm: `make iso` and CI both run
	# mkimage with --workdir /tmp/work inside a container that is thrown away,
	# so every build starts from nothing.

	syslinux_gen_config() {
		local _f _p _initrd
		[ -z "$syslinux_serial" ] || echo "SERIAL $syslinux_serial"
		cat <<- EOF
		UI vesamenu.c32
		MENU BACKGROUND /boot/alpymist/menu-640.png
		MENU TITLE Alpymist
		MENU MARGIN 10
		MENU VSHIFT 13
		MENU ROWS 4
		MENU TABMSGROW 20
		MENU TIMEOUTROW 22
		MENU COLOR screen      0 #00000000 #00000000 none
		MENU COLOR border      0 #00000000 #00000000 none
		MENU COLOR title       0 #ffeaf0f6 #00000000 none
		MENU COLOR unsel       0 #ff9aabbd #00000000 none
		MENU COLOR sel         0 #ff0b121e #ff7fb8d9 none
		MENU COLOR hotkey      0 #ff7fb8d9 #00000000 none
		MENU COLOR hotsel      0 #ff0b121e #ff7fb8d9 none
		MENU COLOR tabmsg      0 #ff9aabbd #00000000 none
		MENU COLOR timeout_msg 0 #ff9aabbd #00000000 none
		MENU COLOR timeout     0 #ffeaf0f6 #00000000 none
		MENU COLOR help        0 #ff9aabbd #00000000 none
		EOF
		echo "TIMEOUT ${syslinux_timeout:-10}"
		echo "PROMPT ${syslinux_prompt:-1}"
		echo "DEFAULT ${kernel_flavors%% *}"

		for _f in $kernel_flavors; do
			_initrd="/boot/initramfs-$_f"
			for _p in $initrd_ucode; do
				_initrd="$_p,$_initrd"
			done
			cat <<- EOF

			LABEL $_f
				MENU LABEL Alpymist
				KERNEL /boot/vmlinuz-$_f
				INITRD $_initrd
				FDTDIR /boot/dtbs-$_f
				APPEND $initfs_cmdline $kernel_cmdline
			EOF
		done
	}

	# Guarded by `loadfont` exactly as the distributions' own grub.cfgs are: a
	# gfxterm with no font is a black screen, and falling back to the plain
	# console menu is the difference between plain and unusable.
	grub_gen_config() {
		local _f _p _initrd
		cat <<- EOF
		if loadfont \$prefix/fonts/unicode.pf2 ; then
			set gfxmode=auto
			insmod all_video
			insmod gfxterm
			insmod png
			terminal_output gfxterm
			background_image -m stretch /boot/alpymist/menu-1280.png
			set color_normal=light-gray/black
			set color_highlight=black/cyan
		fi
		set timeout=1
		EOF

		for _f in $kernel_flavors; do
			_initrd="/boot/initramfs-$_f"
			for _p in $initrd_ucode; do
				_initrd="$_p $_initrd"
			done
			cat <<- EOF

			menuentry "Alpymist" {
				linux	/boot/vmlinuz-$_f $initfs_cmdline $kernel_cmdline
				initrd	$_initrd
			}
			EOF
		done
	}
}
