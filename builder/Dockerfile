# The one place Alpymist is built.
#
# Alpine 3.24 ships Rust 1.96 in main, which clears DeniseUI's 1.95 minimum, so
# the whole toolchain comes from apk with nothing fetched from elsewhere. That
# matters for ADR 0002: a build container assembled entirely from signed Alpine
# packages is one less thing to trust.
FROM alpine:3.24

# abuild: Alpine package building. mkinitfs/xorriso/squashfs: ISO assembly.
RUN apk add --no-cache \
      alpine-sdk build-base apk-tools alpine-conf busybox fakeroot \
      xorriso squashfs-tools mtools dosfstools grub-efi \
      rust cargo \
      git bash doas \
 && adduser -D -G abuild builder \
 && echo 'permit nopass :abuild' > /etc/doas.d/abuild.conf

USER builder
WORKDIR /src
