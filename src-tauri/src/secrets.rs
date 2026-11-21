//! Secrets kept in the system credential store (never in settings.json): the
//! Keychain on macOS, Credential Manager on Windows. Every call blocks, so
//! commands run them on the blocking pool.

use crate::error::{Error, Result};

const SERVICE: &str = "com.grandie.chess";

/// A kind of secret: its credential-store entry and how to check a value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Secret {
    LichessToken,
    OpenRouterKey,
}

impl Secret {
    fn entry_name(self) -> &'static str {
        match self {
            Secret::LichessToken => "lichess-api-token",
            Secret::OpenRouterKey => "openrouter-api-key",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Secret::LichessToken => "Lichess token",
            Secret::OpenRouterKey => "OpenRouter key",
        }
    }

    /// Reject values that can't be right, with a message for the user.
    pub fn validate(self, value: &str) -> Result<()> {
        if value.contains(char::is_whitespace) {
            return Err(Error::other(format!("The {} can't contain spaces.", self.label())));
        }
        match self {
            Secret::LichessToken if !value.starts_with("lip_") || value.len() < 12 => {
                Err(Error::other("That doesn't look like a Lichess API token (they start with lip_)."))
            }
            Secret::OpenRouterKey if !value.starts_with("sk-or-") || value.len() < 20 => {
                Err(Error::other("That doesn't look like an OpenRouter key (they start with sk-or-)."))
            }
            _ => Ok(()),
        }
    }
}

fn entry(secret: Secret) -> Result<keyring::Entry> {
    keyring::Entry::new(SERVICE, secret.entry_name()).map_err(|e| Error::other(format!("the credential store is unavailable: {e}")))
}

pub fn get(secret: Secret) -> Result<Option<String>> {
    match entry(secret)?.get_password() {
        Ok(t) => Ok(Some(t)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(Error::other(format!("could not read the {}: {e}", secret.label()))),
    }
}

/// Store the value, or remove it when `value` is empty.
pub fn set(secret: Secret, value: &str) -> Result<()> {
    let e = entry(secret)?;
    if value.is_empty() {
        return match e.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(err) => Err(Error::other(format!("could not remove the {}: {err}", secret.label()))),
        };
    }
    secret.validate(value)?;
    e.set_password(value).map_err(|err| Error::other(format!("could not save the {}: {err}", secret.label())))
}

pub fn lichess_token() -> Result<Option<String>> {
    get(Secret::LichessToken)
}

/// Store the token, or remove it when `token` is empty.
pub fn set_lichess_token(token: &str) -> Result<()> {
    set(Secret::LichessToken, token)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_values_without_touching_the_store() {
        assert!(Secret::LichessToken.validate("lip_abcdefghijkl").is_ok());
        assert!(Secret::LichessToken.validate("abc").is_err());
        assert!(Secret::OpenRouterKey.validate("sk-or-v1-0123456789abcdef").is_ok());
        assert!(Secret::OpenRouterKey.validate("sk-ant-0123456789abcdefgh").is_err());
        assert!(Secret::OpenRouterKey.validate("sk-or-v1-0123 456789abcdef").is_err());
    }
}
