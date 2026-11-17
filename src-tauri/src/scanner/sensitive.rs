//! Flags files that look like credentials or private keys, by name only.
//! These are surfaced as "needs attention" and are never auto-deleted.

const SENSITIVE_EXTENSIONS: [&str; 10] = [
    "pem", "key", "p12", "pfx", "jks", "keystore", "ppk", "kdbx", "ovpn", "env",
];

const SENSITIVE_NAME_PARTS: [&str; 14] = [
    "client_secret",
    "client-secret",
    "credential",
    "service-account",
    "service_account",
    "serviceaccount",
    "private_key",
    "private-key",
    "privatekey",
    "id_rsa",
    "id_ed25519",
    "api_key",
    "api-key",
    "apikey",
];

pub fn is_sensitive(name: &str, extension: &str, is_dir: bool) -> bool {
    let lower = name.to_lowercase();
    let has_sensitive_extension = !is_dir && SENSITIVE_EXTENSIONS.contains(&extension);
    has_sensitive_extension || SENSITIVE_NAME_PARTS.iter().any(|part| lower.contains(part))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flags_key_and_certificate_extensions() {
        assert!(is_sensitive("server.pem", "pem", false));
        assert!(is_sensitive("cert.P12", "p12", false));
        assert!(is_sensitive("prod.env", "env", false));
    }

    #[test]
    fn flags_credential_names_case_insensitively() {
        assert!(is_sensitive(
            "client_secret_1234.apps.googleusercontent.com.json",
            "json",
            false
        ));
        assert!(is_sensitive("My-Service-Account.json", "json", false));
        assert!(is_sensitive("AWS Credentials.csv", "csv", false));
        assert!(is_sensitive("credentials", "", true));
    }

    #[test]
    fn ignores_ordinary_files_and_extension_rules_for_folders() {
        assert!(!is_sensitive("invoice.pdf", "pdf", false));
        assert!(!is_sensitive("keynote-talk.key.bak", "bak", false));
        assert!(!is_sensitive("project.key", "", true));
    }
}
