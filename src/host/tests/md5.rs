//! MD5, against RFC 1321's own test suite, and a ROM's length.

#[cfg(test)]
mod tests {
    use crate::host::md5::md5;

    #[test]
    fn the_rfcs_test_suite() {
        for (text, digest) in [
            ("", "d41d8cd98f00b204e9800998ecf8427e"),
            ("a", "0cc175b9c0f1b6a831c399e269772661"),
            ("abc", "900150983cd24fb0d6963f7d28e17f72"),
            ("message digest", "f96b697d7cb7938d525a2f31aaf161d0"),
            (
                "abcdefghijklmnopqrstuvwxyz",
                "c3fcd3d76192e4007dfb496cca67e13b",
            ),
            (
                "12345678901234567890123456789012345678901234567890123456789012345678901234567890",
                "57edf4a22be3c955ac49da2e2107b67a",
            ),
        ] {
            assert_eq!(md5(text.as_bytes()), digest, "{text}");
        }
    }

    #[test]
    fn a_4k_of_zeros() {
        assert_eq!(md5(&[0; 4096]), "620f0b67a91f7f74151bc5be745b7110");
    }
}
