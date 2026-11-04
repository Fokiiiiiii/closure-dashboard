use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use hmac::{Hmac, KeyInit, Mac};
use sha2::Sha256;
use std::time::{SystemTime, UNIX_EPOCH};
use subtle::ConstantTimeEq;

pub const AGE: u64 = 7 * 24 * 60 * 60;
#[must_use]
pub fn snowflake(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())
}
#[must_use]
pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}
#[must_use]
pub fn sign(id: &str, secret: &str, time: u64) -> String {
    let data = format!("{id}.{}", time.saturating_add(AGE));
    let mut mac = <Hmac<Sha256> as KeyInit>::new_from_slice(secret.as_bytes())
        .unwrap_or_else(|_| unreachable!());
    mac.update(data.as_bytes());
    format!(
        "{data}.{}",
        URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes())
    )
}
#[must_use]
pub fn verify(cookie: &str, secret: &str, time: u64) -> Option<String> {
    let (data, signature) = cookie.rsplit_once('.')?;
    let (id, expiry) = data.split_once('.')?;
    if !snowflake(id) || expiry.parse::<u64>().ok()? <= time {
        return None;
    }
    let bytes = URL_SAFE_NO_PAD.decode(signature).ok()?;
    let mut mac = <Hmac<Sha256> as KeyInit>::new_from_slice(secret.as_bytes()).ok()?;
    mac.update(data.as_bytes());
    let expected = mac.finalize().into_bytes();
    if bytes.len() != expected.len() || !bool::from(bytes.as_slice().ct_eq(expected.as_slice())) {
        return None;
    }
    Some(id.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sessions_reject_tampering_and_expiry() {
        let c = sign("123", "secret", 100);
        assert_eq!(verify(&c, "secret", 100).as_deref(), Some("123"));
        assert!(verify(&(c.clone() + "x"), "secret", 100).is_none());
        assert!(verify(&c, "other", 100).is_none());
        assert!(verify(&c, "secret", 100 + AGE).is_none());
    }
}
