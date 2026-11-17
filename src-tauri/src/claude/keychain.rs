//! Stores the Anthropic API key in the macOS login Keychain. The key is read
//! only on the Rust side and is never returned to the webview.

use security_framework::passwords::{
    delete_generic_password, get_generic_password, set_generic_password,
};

const SERVICE: &str = "com.local.macorganizer";
const ACCOUNT: &str = "anthropic-api-key";
/// `errSecItemNotFound`
const NOT_FOUND: i32 = -25300;

/// Trim and sanity-check a pasted key before storing it.
pub fn normalize_api_key(input: &str) -> Result<String, String> {
    let key = input.trim();
    if !key.starts_with("sk-ant-") {
        return Err(
            "That doesn't look like an Anthropic API key (it should start with \"sk-ant-\")."
                .into(),
        );
    }
    let is_valid_charset = key
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    if !is_valid_charset || !(20..=300).contains(&key.len()) {
        return Err("That API key has unexpected characters or length.".into());
    }
    Ok(key.to_string())
}

pub fn save_api_key(key: &str) -> Result<(), String> {
    set_generic_password(SERVICE, ACCOUNT, key.as_bytes())
        .map_err(|e| format!("Couldn't save the key to Keychain: {e}"))
}

pub fn load_api_key() -> Result<Option<String>, String> {
    match get_generic_password(SERVICE, ACCOUNT) {
        Ok(bytes) => String::from_utf8(bytes)
            .map(Some)
            .map_err(|_| "The stored API key is corrupted; please enter it again.".into()),
        Err(e) if e.code() == NOT_FOUND => Ok(None),
        Err(e) => Err(format!("Couldn't read the key from Keychain: {e}")),
    }
}

pub fn delete_api_key() -> Result<(), String> {
    match delete_generic_password(SERVICE, ACCOUNT) {
        Err(e) if e.code() != NOT_FOUND => {
            Err(format!("Couldn't remove the key from Keychain: {e}"))
        }
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_and_trims_a_plausible_key() {
        let key = format!("  sk-ant-api03-{}\n", "a".repeat(40));
        assert_eq!(normalize_api_key(&key), Ok(key.trim().to_string()));
    }

    #[test]
    fn rejects_wrong_prefix_charset_or_length() {
        assert!(normalize_api_key("sk-proj-abc").is_err());
        assert!(normalize_api_key("sk-ant-short").is_err());
        assert!(normalize_api_key(&format!("sk-ant-{} x", "a".repeat(30))).is_err());
    }
}
