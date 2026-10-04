//! SHA-1, for the names apk and Flatpak give what they cache.
//!
//! apk keeps a repository's index in a file named after the SHA-1 of its
//! address, and the screenshots kept here are named the same way. That is all
//! this is for: a name that is the same every time, for a file that is found
//! again by it. Nothing here is a secret or a signature, which SHA-1 has not
//! been fit for in a long while.
//!
//! The algorithm is FIPS 180-4's, and the tests are its published examples.

use std::fmt::Write as _;

/// The SHA-1 of `data`, as forty hexadecimal digits.
#[must_use]
#[allow(clippy::many_single_char_names)] // the standard's own: a to e, f, k, w
pub fn hex(data: &[u8]) -> String {
    let mut state: [u32; 5] = [
        0x6745_2301,
        0xefcd_ab89,
        0x98ba_dcfe,
        0x1032_5476,
        0xc3d2_e1f0,
    ];
    // The message, a one bit, zeroes to eight short of a block, and the
    // message's length in bits.
    let mut message = data.to_vec();
    message.push(0x80);
    while message.len() % 64 != 56 {
        message.push(0);
    }
    message.extend_from_slice(&(data.len() as u64).wrapping_mul(8).to_be_bytes());

    for block in message.chunks_exact(64) {
        let mut w = [0u32; 80];
        for (word, bytes) in w.iter_mut().zip(block.chunks_exact(4)) {
            *word = u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
        }
        for i in 16..80 {
            w[i] = (w[i - 3] ^ w[i - 8] ^ w[i - 14] ^ w[i - 16]).rotate_left(1);
        }
        let [mut a, mut b, mut c, mut d, mut e] = state;
        for (i, word) in w.iter().enumerate() {
            let (f, k) = match i {
                0..20 => ((b & c) | (!b & d), 0x5a82_7999),
                20..40 => (b ^ c ^ d, 0x6ed9_eba1),
                40..60 => ((b & c) | (b & d) | (c & d), 0x8f1b_bcdc),
                _ => (b ^ c ^ d, 0xca62_c1d6),
            };
            let next = a
                .rotate_left(5)
                .wrapping_add(f)
                .wrapping_add(e)
                .wrapping_add(k)
                .wrapping_add(*word);
            (e, d, c, b, a) = (d, c, b.rotate_left(30), a, next);
        }
        for (sum, add) in state.iter_mut().zip([a, b, c, d, e]) {
            *sum = sum.wrapping_add(add);
        }
    }

    let mut out = String::with_capacity(40);
    for word in state {
        let _ = write!(out, "{word:08x}");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::hex;

    #[test]
    fn the_published_examples_come_out_as_published() {
        assert_eq!(hex(b""), "da39a3ee5e6b4b0d3255bfef95601890afd80709");
        assert_eq!(hex(b"abc"), "a9993e364706816aba3e25717850c26c9cd0d89d");
        assert_eq!(
            hex(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"),
            "84983e441c3bd26ebaae4aa1f95129e5e54670f1"
        );
        assert_eq!(
            hex(&vec![b'a'; 1_000_000]),
            "34aa973cd4c4daa4f61eeb2bdbad27316534016f"
        );
    }

    /// Where the padding does and does not fit in the message's last block.
    #[test]
    fn a_message_that_ends_at_the_edge_of_a_block_is_padded_right() {
        assert_eq!(hex(&[b'a'; 55]), "c1c8bbdc22796e28c0e15163d20899b65621d65a");
        assert_eq!(hex(&[b'a'; 56]), "c2db330f6083854c99d4b5bfb6e8f29f201be699");
        assert_eq!(hex(&[b'a'; 64]), "0098ba824b5c16427bd7a1122a5a442a25ec644d");
    }

    #[test]
    fn a_repository_is_named_as_apk_names_it() {
        assert_eq!(
            hex(b"https://dl-cdn.alpinelinux.org/alpine/v3.24/main"),
            "1b7a38f8f4a5a6a35ed7f7ce59e7e67a9991939e"
        );
    }
}
