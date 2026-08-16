use std::io::{self, Read as _, Write as _};

use flate2::{
    Compression, Decompress, FlushDecompress, GzBuilder, Status, read::MultiGzDecoder,
    write::ZlibEncoder,
};

use crate::error::TransformError;

struct LimitedWriter {
    bytes: Vec<u8>,
    limit: usize,
}

impl LimitedWriter {
    fn new(limit: usize) -> Self {
        Self {
            bytes: Vec::with_capacity(limit.min(1024)),
            limit,
        }
    }
}

impl io::Write for LimitedWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let Some(new_len) = self.bytes.len().checked_add(bytes.len()) else {
            return Err(io::Error::new(io::ErrorKind::WriteZero, "output limit"));
        };
        if new_len > self.limit {
            return Err(io::Error::new(io::ErrorKind::WriteZero, "output limit"));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

pub(super) fn compress(input: &[u8], output_limit: usize) -> Result<Vec<u8>, TransformError> {
    let writer = LimitedWriter::new(output_limit);
    let mut encoder = GzBuilder::new()
        .mtime(0)
        .operating_system(255)
        .write(writer, Compression::new(6));
    encoder
        .write_all(input)
        .map_err(|_| TransformError::OutputTooLarge {
            limit: output_limit,
        })?;
    encoder
        .finish()
        .map(|writer| writer.bytes)
        .map_err(|_| TransformError::OutputTooLarge {
            limit: output_limit,
        })
}

pub(super) fn decompress(input: &[u8], output_limit: usize) -> Result<Vec<u8>, TransformError> {
    let mut decoder = MultiGzDecoder::new(input);
    let mut output = Vec::with_capacity(output_limit.min(8192));
    let mut buffer = [0; 8192];

    loop {
        let remaining = output_limit - output.len();
        let read_limit = remaining.saturating_add(1).min(buffer.len());
        let read = decoder
            .read(&mut buffer[..read_limit])
            .map_err(|_| TransformError::InvalidGzip)?;
        if read == 0 {
            return Ok(output);
        }
        if read > remaining {
            return Err(TransformError::OutputTooLarge {
                limit: output_limit,
            });
        }
        output.extend_from_slice(&buffer[..read]);
    }
}

pub(super) fn zlib_compress(input: &[u8], output_limit: usize) -> Result<Vec<u8>, TransformError> {
    let writer = LimitedWriter::new(output_limit);
    let mut encoder = ZlibEncoder::new(writer, Compression::new(6));
    encoder
        .write_all(input)
        .map_err(|_| TransformError::OutputTooLarge {
            limit: output_limit,
        })?;
    encoder
        .finish()
        .map(|writer| writer.bytes)
        .map_err(|_| TransformError::OutputTooLarge {
            limit: output_limit,
        })
}

pub(super) fn zlib_decompress(
    input: &[u8],
    output_limit: usize,
) -> Result<Vec<u8>, TransformError> {
    let mut decoder = Decompress::new(true);
    let mut output = Vec::with_capacity(output_limit.min(8192));
    let mut buffer = [0; 8192];

    loop {
        let remaining = output_limit - output.len();
        let read_limit = remaining.saturating_add(1).min(buffer.len());
        let input_offset = decoder.total_in() as usize;
        let before_output = decoder.total_out();
        let flush = if input_offset == input.len() {
            FlushDecompress::Finish
        } else {
            FlushDecompress::None
        };
        let status = decoder
            .decompress(&input[input_offset..], &mut buffer[..read_limit], flush)
            .map_err(|_| TransformError::InvalidZlib)?;
        let consumed = decoder.total_in() as usize - input_offset;
        let produced = (decoder.total_out() - before_output) as usize;

        if produced > remaining {
            return Err(TransformError::OutputTooLarge {
                limit: output_limit,
            });
        }
        output.extend_from_slice(&buffer[..produced]);

        if status == Status::StreamEnd {
            return if decoder.total_in() as usize == input.len() {
                Ok(output)
            } else {
                Err(TransformError::InvalidZlib)
            };
        }
        if consumed == 0 && produced == 0 {
            return Err(TransformError::InvalidZlib);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io::{BufRead as _, Read as _};

    use flate2::bufread::GzDecoder;

    use super::*;
    use crate::error::TransformError;

    const ZLIB_EMPTY: &[u8] = &[0x78, 0x9c, 0x03, 0x00, 0x00, 0x00, 0x00, 0x01];
    const ZLIB_HELLO: &[u8] = &[
        0x78, 0x9c, 0xcb, 0x48, 0xcd, 0xc9, 0xc9, 0x07, 0x00, 0x06, 0x2c, 0x02, 0x15,
    ];
    const ZLIB_FDICT: &[u8] = &[
        0x78, 0xbb, 0x06, 0x2c, 0x02, 0x15, 0xcb, 0x00, 0x11, 0x3a, 0x0a, 0x60, 0x4a, 0x11, 0x00,
        0x21, 0x70, 0x04, 0x96,
    ];

    fn member(input: &[u8]) -> Vec<u8> {
        compress(input, usize::MAX).unwrap()
    }

    #[test]
    fn compress_is_deterministic_with_the_exact_fixed_header_and_one_member() {
        let first = member(b"deterministic payload");
        let second = member(b"deterministic payload");
        assert_eq!(first, second);
        assert_eq!(&first[..10], &[0x1f, 0x8b, 0x08, 0x00, 0, 0, 0, 0, 0, 0xff]);

        let mut decoder = GzDecoder::new(std::io::BufReader::new(first.as_slice()));
        let mut decoded = Vec::new();
        decoder.read_to_end(&mut decoded).unwrap();
        assert_eq!(decoded, b"deterministic payload");
        assert!(decoder.into_inner().fill_buf().unwrap().is_empty());
    }

    #[test]
    fn compress_uses_the_fixed_level_six_member_bytes() {
        assert_eq!(
            member(b"hello"),
            [
                0x1f, 0x8b, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xff, 0xcb, 0x48, 0xcd, 0xc9,
                0xc9, 0x07, 0x00, 0x86, 0xa6, 0x10, 0x36, 0x05, 0x00, 0x00, 0x00,
            ]
        );
    }

    #[test]
    fn compress_enforces_the_output_limit_while_writing() {
        let full = member(b"bounded gzip output");
        assert_eq!(compress(b"bounded gzip output", full.len()).unwrap(), full);
        assert_eq!(
            compress(b"bounded gzip output", full.len() - 1).unwrap_err(),
            TransformError::OutputTooLarge {
                limit: full.len() - 1
            }
        );
        assert_eq!(
            compress(b"", 0).unwrap_err(),
            TransformError::OutputTooLarge { limit: 0 }
        );
    }

    #[test]
    fn decompress_consumes_single_and_all_concatenated_members() {
        let first = member(b"first");
        assert_eq!(decompress(&first, 5).unwrap(), b"first");

        let joined = [first, member(b"-second")].concat();
        assert_eq!(decompress(&joined, 12).unwrap(), b"first-second");
    }

    #[test]
    fn decompress_rejects_invalid_headers_crc_truncation_and_trailing_garbage() {
        let valid = member(b"payload");

        let mut invalid_header = valid.clone();
        invalid_header[2] = 0;
        assert_eq!(
            decompress(&invalid_header, 1024).unwrap_err(),
            TransformError::InvalidGzip
        );

        let mut invalid_crc = valid.clone();
        let crc = invalid_crc.len() - 8;
        invalid_crc[crc] ^= 0x01;
        assert_eq!(
            decompress(&invalid_crc, 1024).unwrap_err(),
            TransformError::InvalidGzip
        );

        let mut invalid_second_crc = member(b"second");
        let crc = invalid_second_crc.len() - 8;
        invalid_second_crc[crc] ^= 0x01;
        let invalid_second = [member(b"first"), invalid_second_crc].concat();
        assert_eq!(
            decompress(&invalid_second, 1024).unwrap_err(),
            TransformError::InvalidGzip
        );

        assert_eq!(
            decompress(&valid[..valid.len() - 1], 1024).unwrap_err(),
            TransformError::InvalidGzip
        );

        let trailing = [valid.as_slice(), b"junk"].concat();
        assert_eq!(
            decompress(&trailing, 1024).unwrap_err(),
            TransformError::InvalidGzip
        );
        assert_eq!(
            decompress(b"", 1024).unwrap_err(),
            TransformError::InvalidGzip
        );
    }

    #[test]
    fn decompress_enforces_the_aggregate_output_limit_without_a_partial_result() {
        let joined = [member(b"abc"), member(b"def")].concat();
        assert_eq!(decompress(&joined, 6).unwrap(), b"abcdef");
        assert_eq!(
            decompress(&joined, 5).unwrap_err(),
            TransformError::OutputTooLarge { limit: 5 }
        );
        assert_eq!(
            decompress(&member(b"x"), 0).unwrap_err(),
            TransformError::OutputTooLarge { limit: 0 }
        );
    }

    #[test]
    fn zlib_compress_uses_fixed_level_six_vectors_and_limits() {
        assert_eq!(zlib_compress(b"", usize::MAX).unwrap(), ZLIB_EMPTY);
        assert_eq!(zlib_compress(b"hello", usize::MAX).unwrap(), ZLIB_HELLO);
        assert_eq!(
            zlib_compress(b"hello", usize::MAX).unwrap(),
            zlib_compress(b"hello", usize::MAX).unwrap()
        );

        let full = zlib_compress(b"bounded zlib output", usize::MAX).unwrap();
        assert_eq!(
            zlib_compress(b"bounded zlib output", full.len()).unwrap(),
            full
        );
        assert_eq!(
            zlib_compress(b"bounded zlib output", full.len() - 1).unwrap_err(),
            TransformError::OutputTooLarge {
                limit: full.len() - 1,
            }
        );
    }

    #[test]
    fn zlib_decompress_accepts_one_complete_stream() {
        assert_eq!(zlib_decompress(ZLIB_EMPTY, 0).unwrap(), b"");
        assert_eq!(zlib_decompress(ZLIB_HELLO, 5).unwrap(), b"hello");
    }

    #[test]
    fn zlib_decompress_rejects_header_fdict_checksum_truncation_and_trailing_data() {
        let mut invalid_header = ZLIB_HELLO.to_vec();
        invalid_header[1] ^= 1;
        let mut invalid_checksum = ZLIB_HELLO.to_vec();
        *invalid_checksum.last_mut().unwrap() ^= 1;

        for input in [
            invalid_header,
            ZLIB_FDICT.to_vec(),
            invalid_checksum,
            ZLIB_HELLO[..ZLIB_HELLO.len() - 1].to_vec(),
            Vec::new(),
            [ZLIB_HELLO, b"junk"].concat(),
            [ZLIB_HELLO, ZLIB_EMPTY].concat(),
        ] {
            assert_eq!(
                zlib_decompress(&input, 1024).unwrap_err(),
                TransformError::InvalidZlib
            );
        }
    }

    #[test]
    fn zlib_decompress_checks_one_extra_byte_across_the_buffer_boundary() {
        let payload = vec![b'x'; 8193];
        let compressed = zlib_compress(&payload, usize::MAX).unwrap();
        assert_eq!(zlib_decompress(&compressed, 8193).unwrap(), payload);
        assert_eq!(
            zlib_decompress(&compressed, 8192).unwrap_err(),
            TransformError::OutputTooLarge { limit: 8192 }
        );
    }
}
