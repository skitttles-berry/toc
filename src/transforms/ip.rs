use crate::error::TransformError;

pub(super) fn normalize(input: &[u8], output_limit: usize) -> Result<Vec<u8>, TransformError> {
    let text = std::str::from_utf8(input).map_err(|_| TransformError::InvalidUtf8Input)?;
    let output = text
        .parse::<std::net::IpAddr>()
        .map_err(|_| TransformError::InvalidIpAddress)?
        .to_string();
    if output.len() > output_limit {
        return Err(TransformError::OutputTooLarge {
            limit: output_limit,
        });
    }
    Ok(output.into_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_ipv4_and_rfc5952_ipv6() {
        assert_eq!(normalize(b"192.0.2.1", 16).unwrap(), b"192.0.2.1");
        assert_eq!(
            normalize(b"2001:0DB8:0000:0000:0000:ff00:0042:8329", 64).unwrap(),
            b"2001:db8::ff00:42:8329"
        );
        assert_eq!(
            normalize(b"2001:0:0:1:0:0:1:1", 64).unwrap(),
            b"2001::1:0:0:1:1"
        );
        assert_eq!(
            normalize(b"2001:db8:0:1:1:1:1:1", 64).unwrap(),
            b"2001:db8:0:1:1:1:1:1"
        );
        assert_eq!(
            normalize(b"::ffff:192.0.2.128", 64).unwrap(),
            b"::ffff:192.0.2.128"
        );
    }

    #[test]
    fn rejects_every_non_address_wrapper() {
        for input in [
            b" 127.0.0.1".as_slice(),
            b"127.0.0.1 ",
            b"127.0.0.1/24",
            b"127.0.0.1:80",
            b"[::1]",
            b"fe80::1%en0",
            b"01.2.3.4",
        ] {
            assert_eq!(
                normalize(input, 64).unwrap_err(),
                TransformError::InvalidIpAddress
            );
        }
    }

    #[test]
    fn normalize_ip_maps_utf8_and_output_errors() {
        assert_eq!(
            normalize(&[0xff], 8).unwrap_err(),
            TransformError::InvalidUtf8Input
        );
        assert_eq!(
            normalize(b"::1", 2).unwrap_err(),
            TransformError::OutputTooLarge { limit: 2 }
        );
        assert_eq!(normalize(b"::1", 3).unwrap(), b"::1");
    }
}
