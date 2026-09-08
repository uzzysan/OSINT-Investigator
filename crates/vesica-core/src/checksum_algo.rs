//! Hand-rolled check-digit algorithms.
//!
//! IBAN mod-97, VIN, the three Polish schemes (NIP, PESEL, REGON) and the
//! bech32 polymod are each a few lines of well-specified arithmetic. The spec
//! fixes them as hand-rolled rather than as dependencies; see the report on
//! ticket 0001-06 for why `iban_validate` was not pulled in.
//!
//! Every function here takes an already-compacted value — separators stripped,
//! letters uppercased — and answers a single yes/no. None of them ever panic
//! on arbitrary input; an unexpected shape is simply `false`.

/// Verify the ISO-13616 mod-97 check on a compacted IBAN.
///
/// Expects the full IBAN with no spaces: two letters, two check digits, then
/// the country's basic bank account number. Returns `false` for anything that
/// does not have that shape, so callers do not need to pre-screen.
pub fn iban_mod97_is_valid(compact: &str) -> bool {
    let bytes = compact.as_bytes();
    if bytes.len() < 5 || bytes.len() > 34 {
        return false;
    }
    if !bytes[0].is_ascii_uppercase() || !bytes[1].is_ascii_uppercase() {
        return false;
    }
    if !bytes[2].is_ascii_digit() || !bytes[3].is_ascii_digit() {
        return false;
    }
    if !bytes.iter().all(|b| b.is_ascii_alphanumeric()) {
        return false;
    }

    // Move the first four characters to the end, map A..Z to 10..35, and take
    // the running remainder mod 97 digit by digit so nothing ever overflows.
    let rotated = bytes[4..].iter().chain(bytes[..4].iter());
    let mut remainder: u32 = 0;
    for &b in rotated {
        if b.is_ascii_digit() {
            remainder = remainder * 10 + u32::from(b - b'0');
        } else if b.is_ascii_uppercase() {
            let v = u32::from(b - b'A') + 10;
            remainder = remainder * 100 + v;
        } else {
            return false;
        }
        remainder %= 97;
    }
    remainder == 1
}

/// The ISO-3779 transliteration value of a VIN character, or `None` for a
/// character that may not appear in a VIN.
fn vin_value(c: u8) -> Option<u32> {
    match c {
        b'0'..=b'9' => Some(u32::from(c - b'0')),
        b'A' | b'J' => Some(1),
        b'B' | b'K' | b'S' => Some(2),
        b'C' | b'L' | b'T' => Some(3),
        b'D' | b'M' | b'U' => Some(4),
        b'E' | b'N' | b'V' => Some(5),
        b'F' | b'W' => Some(6),
        b'G' | b'P' | b'X' => Some(7),
        b'H' | b'Y' => Some(8),
        b'R' | b'Z' => Some(9),
        // I, O and Q are excluded from the VIN alphabet precisely because they
        // are confusable with 1 and 0.
        _ => None,
    }
}

/// Verify the ninth-position check digit of a 17-character VIN.
///
/// North-American practice; European manufacturers frequently ship VINs whose
/// check digit does not verify, which is one more reason a failure here is a
/// warning rather than a rejection.
pub fn vin_is_valid(vin: &str) -> bool {
    const WEIGHTS: [u32; 17] = [8, 7, 6, 5, 4, 3, 2, 10, 0, 9, 8, 7, 6, 5, 4, 3, 2];
    let bytes = vin.as_bytes();
    if bytes.len() != 17 {
        return false;
    }
    let mut sum = 0u32;
    for (i, &b) in bytes.iter().enumerate() {
        let Some(v) = vin_value(b) else {
            return false;
        };
        sum += v * WEIGHTS[i];
    }
    let expected = sum % 11;
    let actual = bytes[8];
    if expected == 10 {
        actual == b'X'
    } else {
        actual == b'0' + u8::try_from(expected).unwrap_or(u8::MAX)
    }
}

/// Turn an all-ASCII-digit string into its digit values, or `None`.
fn digits(s: &str, len: usize) -> Option<Vec<u32>> {
    let bytes = s.as_bytes();
    if bytes.len() != len {
        return None;
    }
    bytes
        .iter()
        .map(|b| {
            if b.is_ascii_digit() {
                Some(u32::from(b - b'0'))
            } else {
                None
            }
        })
        .collect()
}

/// Verify the check digit of a Polish NIP (tax identification number).
///
/// Ten digits; weights 6 5 7 2 3 4 5 6 7 over the first nine, sum mod 11
/// compared with the tenth. A remainder of 10 can never be a valid check
/// digit, so such a number is rejected by the scheme itself.
pub fn nip_is_valid(nip: &str) -> bool {
    const WEIGHTS: [u32; 9] = [6, 5, 7, 2, 3, 4, 5, 6, 7];
    let Some(d) = digits(nip, 10) else {
        return false;
    };
    let sum: u32 = WEIGHTS.iter().zip(&d).map(|(w, v)| w * v).sum();
    let check = sum % 11;
    check != 10 && check == d[9]
}

/// Verify the check digit of a Polish PESEL (national identification number).
///
/// Eleven digits; weights 1 3 7 9 repeating over the first ten, and the check
/// digit is the complement mod 10.
pub fn pesel_is_valid(pesel: &str) -> bool {
    const WEIGHTS: [u32; 10] = [1, 3, 7, 9, 1, 3, 7, 9, 1, 3];
    let Some(d) = digits(pesel, 11) else {
        return false;
    };
    let sum: u32 = WEIGHTS.iter().zip(&d).map(|(w, v)| w * v).sum();
    let check = (10 - (sum % 10)) % 10;
    check == d[10]
}

/// Verify the check digit of a Polish REGON, in either the 9-digit or the
/// 14-digit form.
///
/// The 14-digit form is a 9-digit REGON plus a five-character local-unit
/// suffix with its own check digit, so both are verified.
pub fn regon_is_valid(regon: &str) -> bool {
    const W9: [u32; 8] = [8, 9, 2, 3, 4, 5, 6, 7];
    const W14: [u32; 13] = [2, 4, 8, 5, 0, 9, 7, 3, 6, 1, 2, 4, 8];

    fn check_with(weights: &[u32], d: &[u32]) -> bool {
        let sum: u32 = weights.iter().zip(d).map(|(w, v)| w * v).sum();
        let mut check = sum % 11;
        if check == 10 {
            check = 0;
        }
        check == d[weights.len()]
    }

    if let Some(d) = digits(regon, 9) {
        return check_with(&W9, &d);
    }
    if let Some(d) = digits(regon, 14) {
        // The embedded 9-digit REGON must hold as well as the 14-digit one.
        return check_with(&W9, &d[..9]) && check_with(&W14, &d);
    }
    false
}

/// The bech32 character set, in value order.
const BECH32_CHARSET: &[u8; 32] = b"qpzry9x8gf2tvdw0s3jn54khce6mua7l";

/// BIP-173 polymod over a sequence of 5-bit values.
fn bech32_polymod(values: &[u8]) -> u32 {
    const GEN: [u32; 5] = [
        0x3b6a_57b2,
        0x2650_8e6d,
        0x1ea1_19fa,
        0x3d42_33dd,
        0x2a14_62b3,
    ];
    let mut chk: u32 = 1;
    for &v in values {
        let top = chk >> 25;
        chk = ((chk & 0x01ff_ffff) << 5) ^ u32::from(v);
        for (i, g) in GEN.iter().enumerate() {
            if (top >> i) & 1 == 1 {
                chk ^= g;
            }
        }
    }
    chk
}

/// Verify a bech32 or bech32m address checksum (BIP-173 / BIP-350).
///
/// Case-insensitive as the standard requires, but mixed case is invalid and
/// is rejected. This is pure arithmetic — no hashing — which is why it is
/// worth doing here while base58check (which needs SHA-256) is not.
pub fn bech32_is_valid(address: &str) -> bool {
    if !address.is_ascii() || address.len() < 8 || address.len() > 90 {
        return false;
    }
    let has_lower = address.bytes().any(|b| b.is_ascii_lowercase());
    let has_upper = address.bytes().any(|b| b.is_ascii_uppercase());
    if has_lower && has_upper {
        return false;
    }
    let lowered = address.to_ascii_lowercase();
    let Some(sep) = lowered.rfind('1') else {
        return false;
    };
    if sep == 0 || sep + 7 > lowered.len() {
        return false;
    }
    let (hrp, data) = lowered.split_at(sep);
    let data = &data[1..];

    let mut values: Vec<u8> = Vec::with_capacity(hrp.len() * 2 + 1 + data.len());
    for b in hrp.bytes() {
        if !(33..=126).contains(&b) {
            return false;
        }
        values.push(b >> 5);
    }
    values.push(0);
    for b in hrp.bytes() {
        values.push(b & 31);
    }
    for b in data.bytes() {
        let Some(v) = BECH32_CHARSET.iter().position(|&c| c == b) else {
            return false;
        };
        values.push(u8::try_from(v).unwrap_or(u8::MAX));
    }
    let chk = bech32_polymod(&values);
    chk == 1 || chk == 0x2bc8_30a3
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iban_known_good_and_transposed() {
        assert!(iban_mod97_is_valid("PL61109010140000071219812874"));
        assert!(iban_mod97_is_valid("GB82WEST12345698765432"));
        assert!(iban_mod97_is_valid("DE89370400440532013000"));
        // Two adjacent digits swapped in the account part.
        assert!(!iban_mod97_is_valid("PL61109010140000071219812847"));
        assert!(!iban_mod97_is_valid("GB82WEST12345698765431"));
        assert!(!iban_mod97_is_valid(""));
        assert!(!iban_mod97_is_valid("NOTANIBAN"));
    }

    #[test]
    fn vin_known_good_and_corrupted() {
        assert!(vin_is_valid("1M8GDM9AXKP042788"));
        assert!(vin_is_valid("11111111111111111"));
        assert!(!vin_is_valid("1M8GDM9A1KP042788"));
        // I, O and Q are not in the VIN alphabet.
        assert!(!vin_is_valid("1M8GDM9AXKP04278O"));
        assert!(!vin_is_valid("1M8GDM9AXKP04278"));
    }

    #[test]
    fn polish_schemes() {
        assert!(nip_is_valid("5252248481"));
        assert!(!nip_is_valid("5252248482"));
        assert!(!nip_is_valid("525224848"));

        assert!(pesel_is_valid("44051401359"));
        assert!(!pesel_is_valid("44051401358"));

        assert!(regon_is_valid("123456785"));
        assert!(!regon_is_valid("123456789"));
        assert!(regon_is_valid("12345678512347"));
        assert!(!regon_is_valid("12345678512348"));
    }

    #[test]
    fn bech32_known_good_and_corrupted() {
        assert!(bech32_is_valid(
            "bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t4"
        ));
        assert!(bech32_is_valid(
            "bc1p0xlxvlhemja6c4dqv22uapctqupfhlxm9h8z3k2e72q4k9hcz7vqzk5jj0"
        ));
        assert!(!bech32_is_valid(
            "bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t5"
        ));
        // Mixed case is invalid per BIP-173.
        assert!(!bech32_is_valid(
            "bc1QW508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t4"
        ));
    }

    #[test]
    fn nothing_panics_on_odd_input() {
        for s in ["", " ", "\u{1F600}", "----", "0"] {
            let _ = iban_mod97_is_valid(s);
            let _ = vin_is_valid(s);
            let _ = nip_is_valid(s);
            let _ = pesel_is_valid(s);
            let _ = regon_is_valid(s);
            let _ = bech32_is_valid(s);
        }
    }
}
