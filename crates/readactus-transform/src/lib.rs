//! Deterministic, irreversible replacement of sensitive values.
//!
//! The core primitive is keyed HMAC-SHA256 tokenization: the same input
//! value ALWAYS produces the same replacement within a run (so every
//! occurrence of a customer's email transforms identically across all
//! tables — joins and FK relationships keep working), and without the
//! run key the original is unrecoverable. There is deliberately no
//! reversible mapping stored anywhere: nothing to steal.
//!
//! The run key lives in a `Zeroizing` wrapper so it is wiped from memory
//! when dropped — an attacker snapshotting process memory after a run
//! finds no key to brute-force tokens with.

use chrono::{Datelike, NaiveDate};
use ddbcore::Value;
use hmac::{Hmac, Mac};
use rand::RngCore;
use readactus_detect::PiiKind;
use sha2::Sha256;
use zeroize::Zeroizing;

type HmacSha256 = Hmac<Sha256>;

/// A per-run tokenization key. Generate fresh per run (default), or
/// supply a stored key when two separate runs must produce consistent
/// tokens (e.g. re-copying a source into an existing target).
pub struct RunKey(Zeroizing<[u8; 32]>);

impl RunKey {
    /// Fresh random key — tokens from this run are consistent with each
    /// other and with nothing else.
    pub fn generate() -> Self {
        let mut key = Zeroizing::new([0u8; 32]);
        rand::thread_rng().fill_bytes(key.as_mut());
        Self(key)
    }

    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(Zeroizing::new(bytes))
    }
}

/// Deterministic tokenizer: `transform` maps a (kind, value) pair to a
/// realistic synthetic replacement. Same input → same output, always,
/// within one `RunKey`.
pub struct Tokenizer {
    key: RunKey,
}

impl Tokenizer {
    pub fn new(key: RunKey) -> Self {
        Self { key }
    }

    /// 32 pseudorandom bytes derived from (kind, input). All synthesis
    /// below draws exclusively from this — the original value influences
    /// the output ONLY through the keyed hash, so outputs are
    /// deterministic but unlinkable to inputs without the key.
    fn token(&self, kind: &PiiKind, input: &[u8]) -> [u8; 32] {
        let mut mac = HmacSha256::new_from_slice(self.key.0.as_ref()).expect("HMAC accepts any key length");
        // Domain-separate by kind so the same string detected as two
        // different kinds yields unrelated tokens.
        mac.update(readactus_detect::kind_label(kind).as_bytes());
        mac.update(&[0x1f]);
        mac.update(input);
        mac.finalize().into_bytes().into()
    }

    /// Transforms one cell. NULLs pass through (nullness itself is
    /// structure, not sensitive content). Non-text values for inherently
    /// textual kinds pass through numeric/date synthesis where defined,
    /// otherwise fall back to a numeric token of similar magnitude.
    pub fn transform(&self, kind: &PiiKind, value: &Value) -> Value {
        match value {
            Value::Null => Value::Null,
            Value::Text(s) => Value::Text(self.synthesize_text(kind, s)),
            Value::Date(d) => match kind {
                PiiKind::DateOfBirth => Value::Date(self.synthesize_dob(d)),
                _ => Value::Date(*d),
            },
            Value::SmallInt(n) => Value::SmallInt(self.synthesize_int(kind, &n.to_string(), 4) as i16),
            Value::Integer(n) => Value::Integer(self.synthesize_int(kind, &n.to_string(), 9) as i32),
            Value::BigInt(n) => Value::BigInt(self.synthesize_int(kind, &n.to_string(), 12)),
            other => other.clone(),
        }
    }

    fn synthesize_text(&self, kind: &PiiKind, input: &str) -> String {
        let t = self.token(kind, input.as_bytes());
        match kind {
            PiiKind::Email => {
                let first = pick(FIRST_NAMES, &t, 0).to_lowercase();
                let last = pick(LAST_NAMES, &t, 2).to_lowercase();
                format!("{first}.{last}{}@example.net", num(&t, 4, 100, 999))
            }
            PiiKind::Phone => {
                // 555-01xx is the reserved fictional US exchange; the rest
                // of the digits come from the token.
                format!("+1-{}-555-01{:02}", num(&t, 0, 200, 989), num(&t, 3, 0, 99))
            }
            PiiKind::PersonName => {
                format!("{} {}", pick(FIRST_NAMES, &t, 0), pick(LAST_NAMES, &t, 2))
            }
            PiiKind::Address => {
                format!(
                    "{} {} {}",
                    num(&t, 0, 1, 9999),
                    pick(STREET_NAMES, &t, 2),
                    pick(STREET_TYPES, &t, 4)
                )
            }
            PiiKind::GovernmentId => {
                // 9 digits from the token; 900-999 area prefix is outside
                // real SSN allocation.
                format!("9{:02}-{:02}-{:04}", num(&t, 0, 0, 99), num(&t, 2, 0, 99), num(&t, 4, 0, 9999))
            }
            PiiKind::CreditCard => synthesize_card(&t),
            PiiKind::Credential => {
                // Credentials get no realism — an obviously-synthetic
                // token that can never authenticate anywhere.
                format!("REDACTED-{}", hex(&t, 12))
            }
            PiiKind::DateOfBirth => {
                // Text-typed DOB column: synthesize a plausible ISO date.
                let year = 1950 + (num(&t, 0, 0, 54) as i32);
                let month = 1 + num(&t, 2, 0, 11);
                let day = 1 + num(&t, 4, 0, 27);
                format!("{year:04}-{month:02}-{day:02}")
            }
            PiiKind::IpAddress => {
                // 198.51.100.0/24 is TEST-NET-2, reserved for documentation.
                format!("198.51.100.{}", num(&t, 0, 1, 254))
            }
            PiiKind::Custom(_) => hex(&t, 16),
        }
    }

    fn synthesize_dob(&self, input: &NaiveDate) -> NaiveDate {
        let t = self.token(&PiiKind::DateOfBirth, input.to_string().as_bytes());
        // Preserve the birth YEAR (age cohort stays statistically
        // representative) but scramble month and day.
        let month = 1 + num(&t, 0, 0, 11) as u32;
        let day = 1 + num(&t, 2, 0, 27) as u32;
        NaiveDate::from_ymd_opt(input.year(), month, day).unwrap_or(*input)
    }

    fn synthesize_int(&self, kind: &PiiKind, input: &str, digits: u32) -> i64 {
        let t = self.token(kind, input.as_bytes());
        let cap = 10i64.pow(digits);
        (i64::from_le_bytes(t[..8].try_into().unwrap()).unsigned_abs() % cap as u64) as i64
    }
}

/// Deterministic pick from a word list using two token bytes.
fn pick<'a>(list: &'a [&'a str], token: &[u8; 32], offset: usize) -> &'a str {
    let idx = u16::from_le_bytes([token[offset], token[offset + 1]]) as usize % list.len();
    list[idx]
}

/// Deterministic number in `[lo, hi]` from two token bytes.
fn num(token: &[u8; 32], offset: usize, lo: u32, hi: u32) -> u32 {
    let span = hi - lo + 1;
    lo + (u16::from_le_bytes([token[offset], token[offset + 1]]) as u32 % span)
}

fn hex(token: &[u8; 32], bytes: usize) -> String {
    token[..bytes].iter().map(|b| format!("{b:02x}")).collect()
}

/// 16 digits in the 9999xx test range with a valid Luhn check digit, so
/// format validators accept it but no real card can ever match.
fn synthesize_card(token: &[u8; 32]) -> String {
    let mut digits: Vec<u8> = vec![9, 9, 9, 9];
    for i in 0..11 {
        digits.push(token[i] % 10);
    }
    let check = luhn_check_digit(&digits);
    digits.push(check);
    digits.iter().map(|d| char::from(b'0' + d)).collect()
}

fn luhn_check_digit(digits: &[u8]) -> u8 {
    let mut sum: u32 = 0;
    // Rightmost digit of the FINAL number is the check digit, so payload
    // digits alternate starting with double on the last payload digit.
    for (i, &d) in digits.iter().rev().enumerate() {
        let mut v = d as u32;
        if i % 2 == 0 {
            v *= 2;
            if v > 9 {
                v -= 9;
            }
        }
        sum += v;
    }
    ((10 - (sum % 10)) % 10) as u8
}

const FIRST_NAMES: &[&str] = &[
    "Alex", "Bailey", "Cameron", "Dana", "Eli", "Frankie", "Gray", "Harper", "Indra", "Jules",
    "Kai", "Lennon", "Morgan", "Noor", "Oakley", "Parker", "Quinn", "Riley", "Sasha", "Tatum",
    "Uma", "Vale", "Wren", "Xen", "Yael", "Zion",
];

const LAST_NAMES: &[&str] = &[
    "Ashford", "Barlow", "Calloway", "Dunmore", "Ellery", "Fairbank", "Granger", "Holloway",
    "Ives", "Jensen", "Kirkwood", "Lockhart", "Merton", "Northway", "Oakden", "Pemberton",
    "Quill", "Rosewood", "Sterling", "Thackeray", "Underhill", "Vance", "Whitfield", "Yardley",
];

const STREET_NAMES: &[&str] = &[
    "Maple", "Cedar", "Elm", "Birch", "Willow", "Aspen", "Juniper", "Rowan", "Hazel", "Alder",
    "Linden", "Sycamore", "Chestnut", "Magnolia", "Poplar", "Laurel",
];

const STREET_TYPES: &[&str] = &["Street", "Avenue", "Lane", "Road", "Drive", "Court", "Way", "Terrace"];

#[cfg(test)]
mod tests {
    use super::*;

    fn tokenizer() -> Tokenizer {
        Tokenizer::new(RunKey::from_bytes([7u8; 32]))
    }

    #[test]
    fn same_input_same_output() {
        let t = tokenizer();
        let a = t.transform(&PiiKind::Email, &Value::Text("alice@corp.com".into()));
        let b = t.transform(&PiiKind::Email, &Value::Text("alice@corp.com".into()));
        assert_eq!(a, b, "determinism is the referential-integrity guarantee");
    }

    #[test]
    fn different_inputs_different_outputs() {
        let t = tokenizer();
        let a = t.transform(&PiiKind::Email, &Value::Text("alice@corp.com".into()));
        let b = t.transform(&PiiKind::Email, &Value::Text("bob@corp.com".into()));
        assert_ne!(a, b);
    }

    #[test]
    fn different_keys_different_outputs() {
        let t1 = Tokenizer::new(RunKey::from_bytes([1u8; 32]));
        let t2 = Tokenizer::new(RunKey::from_bytes([2u8; 32]));
        let v = Value::Text("alice@corp.com".into());
        assert_ne!(t1.transform(&PiiKind::Email, &v), t2.transform(&PiiKind::Email, &v));
    }

    #[test]
    fn same_value_different_kinds_unrelated() {
        let t = tokenizer();
        let v = Value::Text("4111111111111111".into());
        let as_card = t.transform(&PiiKind::CreditCard, &v);
        let as_gov = t.transform(&PiiKind::GovernmentId, &v);
        assert_ne!(as_card, as_gov, "kind domain separation");
    }

    #[test]
    fn output_never_contains_input() {
        let t = tokenizer();
        for kind in [PiiKind::Email, PiiKind::PersonName, PiiKind::Address, PiiKind::Credential] {
            let out = t.transform(&kind, &Value::Text("Wolfeschlegelstein".into()));
            if let Value::Text(s) = out {
                assert!(!s.contains("Wolfeschlegelstein"), "{kind:?} leaked input");
            } else {
                panic!("expected text output");
            }
        }
    }

    #[test]
    fn null_passes_through() {
        assert_eq!(tokenizer().transform(&PiiKind::Email, &Value::Null), Value::Null);
    }

    #[test]
    fn email_looks_like_an_email() {
        let t = tokenizer();
        if let Value::Text(s) = t.transform(&PiiKind::Email, &Value::Text("alice@corp.com".into())) {
            assert!(s.contains('@') && s.ends_with("example.net"), "got {s}");
        } else {
            panic!("expected text");
        }
    }

    #[test]
    fn credit_card_passes_luhn() {
        let t = tokenizer();
        if let Value::Text(s) = t.transform(&PiiKind::CreditCard, &Value::Text("4111111111111111".into())) {
            assert_eq!(s.len(), 16);
            assert!(s.starts_with("9999"), "test-range prefix, got {s}");
            let digits: Vec<u32> = s.chars().map(|c| c.to_digit(10).unwrap()).collect();
            let mut sum = 0u32;
            for (i, d) in digits.iter().rev().enumerate() {
                let mut v = *d;
                if i % 2 == 1 {
                    v *= 2;
                    if v > 9 {
                        v -= 9;
                    }
                }
                sum += v;
            }
            assert_eq!(sum % 10, 0, "Luhn check failed for {s}");
        } else {
            panic!("expected text");
        }
    }

    #[test]
    fn dob_preserves_year() {
        let t = tokenizer();
        let input = NaiveDate::from_ymd_opt(1987, 6, 15).unwrap();
        if let Value::Date(d) = t.transform(&PiiKind::DateOfBirth, &Value::Date(input)) {
            assert_eq!(d.year(), 1987, "age cohort must be preserved");
        } else {
            panic!("expected date");
        }
    }
}
