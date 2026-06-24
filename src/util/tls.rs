/// Normalized TLS behaviour derived from the `ssl_mode` / `sslmode` config string.
///
/// | Config value                                    | Variant           |
/// |-------------------------------------------------|-------------------|
/// | `"disable"`                                     | `Disabled`        |
/// | `"require"`                                     | `RequireNoVerify` |
/// | `"prefer"`, `"allow"`, `"verify-ca"`, `"verify-full"`, anything else | `VerifyFull` |
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TlsMode {
    /// Plain-text connection — no TLS negotiation.
    Disabled,
    /// TLS wire encryption; certificate and hostname are NOT verified.
    /// Matches libpq `sslmode=require` semantics.
    RequireNoVerify,
    /// TLS with full certificate and hostname verification (native-tls default).
    VerifyFull,
}

/// Parse a libpq / MySQL `ssl_mode` string into a [`TlsMode`].
pub fn parse_ssl_mode(mode: &str) -> TlsMode {
    match mode {
        "disable" => TlsMode::Disabled,
        "require" => TlsMode::RequireNoVerify,
        _ => TlsMode::VerifyFull,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disable_maps_to_disabled() {
        assert_eq!(parse_ssl_mode("disable"), TlsMode::Disabled);
    }

    #[test]
    fn require_maps_to_no_verify() {
        assert_eq!(parse_ssl_mode("require"), TlsMode::RequireNoVerify);
    }

    #[test]
    fn everything_else_maps_to_verify_full() {
        for mode in &["prefer", "allow", "verify-ca", "verify-full", ""] {
            assert_eq!(parse_ssl_mode(mode), TlsMode::VerifyFull, "mode={mode}");
        }
    }
}
