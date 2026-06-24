#[allow(dead_code)]
pub fn encode(bytes: &[u8]) -> String {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0] as usize;
        let b1 = chunk.get(1).copied().unwrap_or(0) as usize;
        let b2 = chunk.get(2).copied().unwrap_or(0) as usize;
        out.push(CHARS[b0 >> 2] as char);
        out.push(CHARS[((b0 & 3) << 4) | (b1 >> 4)] as char);
        if chunk.len() > 1 {
            out.push(CHARS[((b1 & 0xf) << 2) | (b2 >> 6)] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(CHARS[b2 & 0x3f] as char);
        } else {
            out.push('=');
        }
    }
    out
}

#[allow(dead_code)]
pub fn decode(input: &str) -> anyhow::Result<Vec<u8>> {
    let input = input.trim_end_matches('=');
    let mut out = Vec::with_capacity(input.len() * 3 / 4);
    let mut bits = 0u32;
    let mut n = 0u8;
    for c in input.chars() {
        let v: u32 = match c {
            'A'..='Z' => c as u32 - 'A' as u32,
            'a'..='z' => c as u32 - 'a' as u32 + 26,
            '0'..='9' => c as u32 - '0' as u32 + 52,
            '+' | '-' => 62,
            '/' | '_' => 63,
            _ => anyhow::bail!("Invalid base64 character: {}", c),
        };
        bits = (bits << 6) | v;
        n += 1;
        if n == 4 {
            out.push((bits >> 16) as u8);
            out.push((bits >> 8) as u8);
            out.push(bits as u8);
            bits = 0;
            n = 0;
        }
    }
    match n {
        2 => out.push((bits >> 4) as u8),
        3 => {
            out.push((bits >> 10) as u8);
            out.push((bits >> 2) as u8);
        }
        _ => {}
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_decode_roundtrip() {
        let data = b"Hello, World! This is a test.";
        let encoded = encode(data);
        let decoded = decode(&encoded).unwrap();
        assert_eq!(decoded, data);
    }

    #[test]
    fn decode_standard_base64() {
        // "Man" → "TWFu"
        assert_eq!(decode("TWFu").unwrap(), b"Man");
        // "Ma" → "TWE="
        assert_eq!(decode("TWE=").unwrap(), b"Ma");
        // "M" → "TQ=="
        assert_eq!(decode("TQ==").unwrap(), b"M");
    }

    #[test]
    fn decode_accepts_url_safe() {
        // URL-safe variant uses - and _ instead of + and /
        let url_safe = "SGVsbG8-V29ybGQ_";
        let standard = "SGVsbG8+V29ybGQ/";
        assert_eq!(decode(url_safe).unwrap(), decode(standard).unwrap());
    }
}
