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
	apks="$apks alpymistctl mesa-egl mesa-gles mesa-dri-gallium"
	apkovl="genapkovl-alpymist.sh"
}
