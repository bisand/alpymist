# The one place Alpymist is built. Pinned digest, no network surprises.
FROM alpine:3.22

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
