//! Secrets kept in the macOS Keychain (never in settings.json).

use crate::error::{Error, Result};

const SERVICE: &str = "com.grandie.chess";
const LICHESS_TOKEN: &str = "lichess-api-token";

fn entry(name: &str) -> Result<keyring::Entry> {
    keyring::Entry::new(SERVICE, name).map_err(|e| Error::other(format!("Keychain unavailable: {e}")))
}

pub fn lichess_token() -> Result<Option<String>> {
    match entry(LICHESS_TOKEN)?.get_password() {
        Ok(t) => Ok(Some(t)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(Error::other(format!("could not read the Lichess token: {e}"))),
    }
}

/// Store the token, or remove it when `token` is empty.
pub fn set_lichess_token(token: &str) -> Result<()> {
    let e = entry(LICHESS_TOKEN)?;
    if token.is_empty() {
        return match e.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(err) => Err(Error::other(format!("could not remove the Lichess token: {err}"))),
        };
    }
    if !token.starts_with("lip_") || token.len() < 12 || token.contains(char::is_whitespace) {
        return Err(Error::other("That doesn't look like a Lichess API token (they start with lip_)."));
    }
    e.set_password(token).map_err(|err| Error::other(format!("could not save the Lichess token: {err}")))
}
