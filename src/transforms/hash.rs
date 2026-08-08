use sha2::{Digest as _, Sha256, Sha512};

use crate::{error::TransformError, transforms::hex};

pub(super) fn sha256(input: &[u8], output_limit: usize) -> Result<Vec<u8>, TransformError> {
    hex::encode(&Sha256::digest(input), output_limit)
}

pub(super) fn sha512(input: &[u8], output_limit: usize) -> Result<Vec<u8>, TransformError> {
    hex::encode(&Sha512::digest(input), output_limit)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::TransformError;

    #[test]
    fn sha256_matches_known_vectors_as_lowercase_hex() {
        assert_eq!(
            sha256(b"", 64).unwrap(),
            b"e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            sha256(b"abc", 64).unwrap(),
            b"ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            sha256(&[0xff, 0x00], 64).unwrap(),
            b"ea5dbf9596d187e9500f23e9a680109475341cf4e81f7e043f7d97152c10772f"
        );
    }

    #[test]
    fn sha512_matches_known_vector_as_lowercase_hex() {
        assert_eq!(
            sha512(b"abc", 128).unwrap(),
            concat!(
                "ddaf35a193617abacc417349ae20413112e6fa4e89a97ea20a9eeee64b55d39a",
                "2192992a274fc1a836ba3c23a3feebbd454d4423643ce80e2a9ac94fa54ca49f"
            )
            .as_bytes()
        );
    }

    #[test]
    fn hashes_reject_limits_below_the_fixed_hex_length() {
        assert_eq!(
            sha256(b"abc", 63).unwrap_err(),
            TransformError::OutputTooLarge { limit: 63 }
        );
        assert_eq!(
            sha512(b"abc", 127).unwrap_err(),
            TransformError::OutputTooLarge { limit: 127 }
        );
    }
}
