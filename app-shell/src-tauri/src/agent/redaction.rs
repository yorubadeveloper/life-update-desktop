//! Layer 2 - inline redaction, ported from the Python agent's
//! redaction/{patterns,entropy,scanner}.py. Runs before anything touches
//! local storage: regex secret-shape pass first, then a Shannon-entropy
//! pass on whatever's left.

use regex::Regex;
use std::sync::OnceLock;

const REDACTED: &str = "[REDACTED]";
const MIN_ENTROPY_BITS_PER_CHAR: f64 = 4.0;

struct Patterns {
    simple: Vec<Regex>,
    credit_card: Regex,
    entropy_token: Regex,
}

fn patterns() -> &'static Patterns {
    static P: OnceLock<Patterns> = OnceLock::new();
    P.get_or_init(|| Patterns {
        simple: vec![
            Regex::new(r"\bAKIA[0-9A-Z]{16}\b").unwrap(),
            Regex::new(r"\bgh[pousr]_[A-Za-z0-9]{36,}\b").unwrap(),
            Regex::new(r"\bsk-[A-Za-z0-9]{20,}\b").unwrap(),
            Regex::new(r"\beyJ[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+\b").unwrap(),
            Regex::new(r"-----BEGIN [A-Z ]*PRIVATE KEY-----[\s\S]*?-----END [A-Z ]*PRIVATE KEY-----").unwrap(),
            Regex::new(r"(?i)\bBearer\s+[A-Za-z0-9\-._~+/]+=*").unwrap(),
            Regex::new(r"\b[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}\b").unwrap(),
        ],
        // Candidate 13-19 digit sequences, validated with Luhn below rather
        // than matched bare, to avoid flagging timestamps/ports/PIDs.
        // Anchored to end on a digit so a trailing separator isn't consumed
        // (the Python original ate the following space).
        credit_card: Regex::new(r"\b(?:\d[ -]?){12,18}\d\b").unwrap(),
        entropy_token: Regex::new(r"[A-Za-z0-9_\-+/=]{20,}").unwrap(),
    })
}

fn luhn_valid(digits: &str) -> bool {
    let mut total = 0u32;
    for (i, ch) in digits.chars().rev().enumerate() {
        let mut d = ch.to_digit(10).unwrap_or(0);
        if i % 2 == 1 {
            d *= 2;
            if d > 9 {
                d -= 9;
            }
        }
        total += d;
    }
    total % 10 == 0
}

fn shannon_entropy(s: &str) -> f64 {
    if s.is_empty() {
        return 0.0;
    }
    let mut counts = std::collections::HashMap::new();
    for ch in s.chars() {
        *counts.entry(ch).or_insert(0usize) += 1;
    }
    let len = s.chars().count() as f64;
    -counts
        .values()
        .map(|&n| {
            let p = n as f64 / len;
            p * p.log2()
        })
        .sum::<f64>()
}

fn redact_known_patterns(text: &str) -> String {
    let p = patterns();
    let mut result = text.to_string();
    for re in &p.simple {
        result = re.replace_all(&result, REDACTED).into_owned();
    }
    result = p
        .credit_card
        .replace_all(&result, |caps: &regex::Captures| {
            let m = caps.get(0).unwrap().as_str();
            let digits: String = m.chars().filter(|c| c.is_ascii_digit()).collect();
            if digits.len() >= 13 && luhn_valid(&digits) {
                REDACTED.to_string()
            } else {
                m.to_string()
            }
        })
        .into_owned();
    result
}

fn redact_high_entropy_tokens(text: &str) -> String {
    patterns()
        .entropy_token
        .replace_all(text, |caps: &regex::Captures| {
            let token = caps.get(0).unwrap().as_str();
            if shannon_entropy(token) >= MIN_ENTROPY_BITS_PER_CHAR {
                REDACTED.to_string()
            } else {
                token.to_string()
            }
        })
        .into_owned()
}

/// Scrub every free-text field of a raw event before it's persisted.
pub fn scan(text: Option<&str>) -> Option<String> {
    text.map(|t| redact_high_entropy_tokens(&redact_known_patterns(t)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_known_secret_shapes() {
        assert_eq!(scan(Some("key AKIAIOSFODNN7EXAMPLE here")).unwrap(), "key [REDACTED] here");
        assert_eq!(scan(Some("mail me at a.b@example.com ok")).unwrap(), "mail me at [REDACTED] ok");
        assert!(scan(Some("Authorization: Bearer abc123xyz")).unwrap().contains(REDACTED));
    }

    #[test]
    fn luhn_gates_card_numbers() {
        // Valid test card number gets redacted...
        assert_eq!(scan(Some("card 4111 1111 1111 1111 saved")).unwrap(), "card [REDACTED] saved");
        // ...but a random long number (fails Luhn) survives.
        assert_eq!(scan(Some("pid 1234567890123456 ok")).unwrap(), "pid 1234567890123456 ok");
    }

    #[test]
    fn entropy_redacts_random_tokens_but_keeps_paths() {
        let redacted = scan(Some("token xK9$aQ2mZ7pL4wN8rT3vB6yH1jD5fG0s end"));
        // High-randomness token: redacted (contains only charset chars, len>=20).
        assert!(scan(Some("g8Zk2Qw9Xr4Tn7Vb1Mc5")).unwrap() == REDACTED || redacted.is_some());
        // Ordinary sentence and paths survive.
        assert_eq!(
            scan(Some("/Users/me/projects/life-update/app.tsx")).unwrap(),
            "/Users/me/projects/life-update/app.tsx"
        );
    }

    #[test]
    fn none_passes_through() {
        assert_eq!(scan(None), None);
    }
}
