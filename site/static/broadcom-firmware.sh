#!/bin/sh
# Alpymist: the firmware Broadcom's older Wi-Fi cards need, onto a stick.
#
#     sh broadcom-firmware.sh [DIR]
#
# For a machine whose only network is such a card: every Mac from before
# 2012, and a good many other laptops of those years. Run this on any machine
# that has a network -- macOS, any Linux, a BSD -- and it leaves the firmware
# in DIR/alpymist-firmware/b43. DIR is the stick; left out, it is wherever
# this script is, which on an Alpymist install stick is the stick. Alpymist's
# installer takes the firmware from the stick it installs from, and on a
# machine already installed
#
#     doas alpymist firmware broadcom --from DIR
#
# does the same. On Alpymist itself, `alpymist firmware broadcom --to DIR` is
# this script.
#
# Alpymist does not ship that firmware, and neither does this: Broadcom never
# gave anyone leave to pass it on. What is published is Broadcom's own driver
# for routers, which has the firmware in it. This downloads that driver, to
# the machine it runs on, and cuts the firmware out with b43-fwcutter, as
# every distribution's own installer for it does. Both downloads are held to
# the checksums below; a file that is not the one named is not used.
#
# It needs curl or wget, tar with bzip2, and either b43-fwcutter or a C
# compiler to build it with: on macOS, `xcode-select --install`.
#
# crates/alpymist/src/broadcom.rs holds this to the same file and checksum
# as `alpymist firmware broadcom`. See docs/adr/0021.

set -eu

# Broadcom's driver, as OpenWrt keeps it for building its own images: the
# version the b43 developers name for every kernel since 3.2.
ARCHIVE="broadcom-wl-5.100.138.tar.bz2"
SHA256="f1e7067aac5b62b67b8b6e4c517990277804339ac16065eb13c731ff909ae46f"
MIRRORS="https://sources.openwrt.org https://downloads.openwrt.org/sources https://mirror2.openwrt.org/sources"
OBJECT="broadcom-wl-5.100.138/linux/wl_apsta.o"

# The tool that cuts the firmware out, from its author, for a machine that
# has no package of it. Free software: GPL, two C files.
CUTTER_ARCHIVE="b43-fwcutter-019.tar.bz2"
CUTTER_SHA256="d6ea85310df6ae08e7f7e46d8b975e17fc867145ee249307413cfbe15d7121ce"
CUTTER_URL="https://bues.ch/b43/fwcutter"

die() {
	echo "broadcom-firmware: $*" >&2
	exit 1
}

has() {
	command -v "$1" >/dev/null 2>&1
}

case "${1:-}" in
	-h | --help)
		sed -n '2,/^$/s/^# \{0,1\}//p' "$0"
		exit 0
		;;
esac

if [ $# -ge 1 ]; then
	dir=$1
elif [ -f "$0" ]; then
	dir=$(dirname "$0")
else
	die "say where the stick is: sh -s /path/to/stick"
fi
[ -d "$dir" ] || die "$dir is not a directory"
into="$dir/alpymist-firmware"

# Fetch $1 to the file $2.
download() {
	if has curl; then
		curl -fsSL -o "$2" "$1"
	elif has wget; then
		wget -q -O "$2" "$1"
	else
		die "this needs curl or wget to download with"
	fi
}

# Print the SHA-256 of the file $1.
sha256() {
	if has sha256sum; then
		sha256sum "$1" | cut -d ' ' -f 1
	elif has shasum; then
		shasum -a 256 "$1" | cut -d ' ' -f 1
	elif has openssl; then
		openssl dgst -sha256 "$1" | sed 's/.*= *//'
	else
		die "this needs sha256sum, shasum or openssl to check the download with"
	fi
}

# Fail unless the file $1 has the checksum $2.
check() {
	sum=$(sha256 "$1")
	[ "$sum" = "$2" ] || die "$(basename "$1") is not the file this was written against (sha256 $sum, not $2); nothing was written"
}

scratch=$(mktemp -d "${TMPDIR:-/tmp}/alpymist-firmware.XXXXXX")
trap 'rm -rf "$scratch"' EXIT
trap 'exit 1' INT TERM HUP

if has b43-fwcutter; then
	cutter=b43-fwcutter
else
	has cc || die "this needs b43-fwcutter, or a C compiler to build it with.
On macOS: xcode-select --install. On Linux it is a package named b43-fwcutter.
Or run this on another machine."
	echo "Downloading $CUTTER_URL/$CUTTER_ARCHIVE"
	download "$CUTTER_URL/$CUTTER_ARCHIVE" "$scratch/$CUTTER_ARCHIVE" \
		|| die "could not download b43-fwcutter: is there a network?"
	check "$scratch/$CUTTER_ARCHIVE" "$CUTTER_SHA256"
	tar -xjf "$scratch/$CUTTER_ARCHIVE" -C "$scratch"
	echo "Building b43-fwcutter"
	# What its Makefile does, without needing GNU make.
	(
		cd "$scratch/${CUTTER_ARCHIVE%.tar.bz2}"
		cc -std=c99 -Os -D_BSD_SOURCE -D_DEFAULT_SOURCE -DFWCUTTER_VERSION_=019 \
			-o b43-fwcutter fwcutter.c md5.c
	) || die "could not build b43-fwcutter"
	cutter="$scratch/${CUTTER_ARCHIVE%.tar.bz2}/b43-fwcutter"
fi

fetched=
for mirror in $MIRRORS; do
	echo "Downloading $mirror/$ARCHIVE"
	if download "$mirror/$ARCHIVE" "$scratch/$ARCHIVE"; then
		fetched=yes
		break
	fi
done
[ -n "$fetched" ] || die "could not download Broadcom's driver from any mirror: is there a network?"
check "$scratch/$ARCHIVE" "$SHA256"

tar -xjf "$scratch/$ARCHIVE" -C "$scratch" "$OBJECT" \
	|| die "could not unpack Broadcom's driver"
# Cut where it was unpacked and copied over after, so a stick is left with
# all of it or none.
mkdir "$scratch/cut"
"$cutter" -w "$scratch/cut" "$scratch/$OBJECT" >/dev/null \
	|| die "b43-fwcutter could not cut the firmware out"
set -- "$scratch"/cut/b43/*.fw
[ -f "$1" ] || die "b43-fwcutter cut nothing out"
mkdir -p "$into/b43" || die "could not write to $dir"
cp "$scratch"/cut/b43/*.fw "$into/b43/" || die "could not write to $into"

echo "Broadcom's Wi-Fi firmware is in $into/b43."
echo "Alpymist's installer takes it from the stick it installs from. On a machine"
echo "already installed: doas alpymist firmware broadcom --from $dir"
