//! Where the relay token lives. Never a config file, a log or a crash report.
//!
//! [`KeyringTokenStore`] (cargo feature `keyring`) uses the OS credential store; tests and builds
//! without the feature use [`MemoryTokenStore`].

use std::fmt;
use std::sync::Mutex;

/// The keyring service name.
pub const KEYRING_SERVICE: &str = "io.longlines.relay";
/// The keyring user name. Fixed so the token can be found before the account is known.
pub const KEYRING_USER: &str = "token";

/// A relay token. Its `Debug` and `Display` never show the secret.
#[derive(Clone, PartialEq, Eq)]
pub struct Token(String);

impl Token {
    pub fn new(secret: impl Into<String>) -> Self {
        Self(secret.into())
    }

    /// The secret, for the `Authorization` header and the credential store only.
    pub fn expose(&self) -> &str {
        &self.0
    }

    /// True for `ll_relay_` and 52 lower-case base32 characters (contract section 3).
    pub fn is_well_formed(&self) -> bool {
        self.0.strip_prefix("ll_relay_").is_some_and(|rest| {
            rest.len() == 52
                && rest
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || (b'2'..=b'7').contains(&b))
        })
    }
}

impl fmt::Debug for Token {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Token(<redacted>)")
    }
}

impl fmt::Display for Token {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("<redacted>")
    }
}

/// Why the credential store failed. Never carries the token.
#[derive(Debug, thiserror::Error)]
#[error("credential store: {0}")]
pub struct TokenStoreError(pub String);

/// Keeps one token.
pub trait TokenStore: Send + Sync + fmt::Debug {
    /// `Ok(None)` when no token is stored.
    fn load(&self) -> Result<Option<Token>, TokenStoreError>;
    fn save(&self, token: &Token) -> Result<(), TokenStoreError>;
    /// Deleting when nothing is stored is fine.
    fn delete(&self) -> Result<(), TokenStoreError>;
}

/// An in-memory store for tests and for builds without the credential store.
#[derive(Debug, Default)]
pub struct MemoryTokenStore {
    token: Mutex<Option<Token>>,
}

impl MemoryTokenStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_token(token: Token) -> Self {
        Self {
            token: Mutex::new(Some(token)),
        }
    }
}

impl TokenStore for MemoryTokenStore {
    fn load(&self) -> Result<Option<Token>, TokenStoreError> {
        Ok(self.token.lock().unwrap_or_else(|e| e.into_inner()).clone())
    }

    fn save(&self, token: &Token) -> Result<(), TokenStoreError> {
        *self.token.lock().unwrap_or_else(|e| e.into_inner()) = Some(token.clone());
        Ok(())
    }

    fn delete(&self) -> Result<(), TokenStoreError> {
        *self.token.lock().unwrap_or_else(|e| e.into_inner()) = None;
        Ok(())
    }
}

/// Keeps a copy of another store's token in memory, so the sender's frequent reads never reach
/// the OS credential store (one read at launch, then only on sign-in and sign-out).
#[derive(Debug)]
pub struct CachingTokenStore<S> {
    inner: S,
    /// `None` until the first successful load.
    cached: Mutex<Option<Option<Token>>>,
}

impl<S: TokenStore> CachingTokenStore<S> {
    pub fn new(inner: S) -> Self {
        Self {
            inner,
            cached: Mutex::new(None),
        }
    }

    fn cache(&self) -> std::sync::MutexGuard<'_, Option<Option<Token>>> {
        self.cached.lock().unwrap_or_else(|e| e.into_inner())
    }
}

impl<S: TokenStore> TokenStore for CachingTokenStore<S> {
    fn load(&self) -> Result<Option<Token>, TokenStoreError> {
        let mut cached = self.cache();
        if let Some(token) = cached.as_ref() {
            return Ok(token.clone());
        }
        let token = self.inner.load()?;
        *cached = Some(token.clone());
        Ok(token)
    }

    fn save(&self, token: &Token) -> Result<(), TokenStoreError> {
        let mut cached = self.cache();
        self.inner.save(token)?;
        *cached = Some(Some(token.clone()));
        Ok(())
    }

    /// Forgets the token in memory even when the credential store fails, so a revoked token is
    /// never sent again from this run.
    fn delete(&self) -> Result<(), TokenStoreError> {
        let mut cached = self.cache();
        *cached = Some(None);
        self.inner.delete()
    }
}

/// The OS credential store: Keychain on macOS, Credential Manager on Windows, the Secret
/// Service on Linux.
#[cfg(feature = "keyring")]
#[derive(Debug, Default)]
pub struct KeyringTokenStore;

#[cfg(feature = "keyring")]
impl KeyringTokenStore {
    pub fn new() -> Self {
        Self
    }

    fn entry() -> Result<keyring::Entry, TokenStoreError> {
        keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER)
            .map_err(|e| TokenStoreError(e.to_string()))
    }
}

#[cfg(feature = "keyring")]
impl TokenStore for KeyringTokenStore {
    fn load(&self) -> Result<Option<Token>, TokenStoreError> {
        match Self::entry()?.get_password() {
            Ok(secret) => Ok(Some(Token::new(secret))),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(TokenStoreError(e.to_string())),
        }
    }

    fn save(&self, token: &Token) -> Result<(), TokenStoreError> {
        Self::entry()?
            .set_password(token.expose())
            .map_err(|e| TokenStoreError(e.to_string()))
    }

    fn delete(&self) -> Result<(), TokenStoreError> {
        match Self::entry()?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(TokenStoreError(e.to_string())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECRET: &str = "ll_relay_abcdefghijklmnopqrstuvwxyz234567abcdefghijklmnopqrst";

    #[test]
    fn token_never_prints_its_secret() {
        let token = Token::new(SECRET);
        assert!(!format!("{token:?}").contains("ll_relay_"));
        assert!(!format!("{token}").contains("ll_relay_"));
        assert_eq!(token.expose(), SECRET);
    }

    #[test]
    fn token_shape() {
        assert!(Token::new(SECRET).is_well_formed());
        assert!(!Token::new("ll_relay_short").is_well_formed());
        assert!(!Token::new(SECRET.to_uppercase()).is_well_formed());
        assert!(!Token::new(SECRET.replace('a', "1")).is_well_formed());
        assert!(!Token::new(format!("{SECRET}a")).is_well_formed());
    }

    /// Counts reads and can be made to fail.
    #[derive(Debug, Default)]
    struct Counting {
        store: MemoryTokenStore,
        loads: std::sync::atomic::AtomicUsize,
        fail_delete: bool,
    }

    impl TokenStore for Counting {
        fn load(&self) -> Result<Option<Token>, TokenStoreError> {
            self.loads.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            self.store.load()
        }
        fn save(&self, token: &Token) -> Result<(), TokenStoreError> {
            self.store.save(token)
        }
        fn delete(&self) -> Result<(), TokenStoreError> {
            if self.fail_delete {
                return Err(TokenStoreError("locked".into()));
            }
            self.store.delete()
        }
    }

    #[test]
    fn caching_store_reads_the_inner_store_once() {
        let store = CachingTokenStore::new(Counting::default());
        assert_eq!(store.load().unwrap(), None);
        assert_eq!(store.load().unwrap(), None);
        store.save(&Token::new(SECRET)).unwrap();
        assert_eq!(store.load().unwrap(), Some(Token::new(SECRET)));
        assert_eq!(
            store.inner.loads.load(std::sync::atomic::Ordering::SeqCst),
            1
        );
        assert_eq!(store.inner.store.load().unwrap(), Some(Token::new(SECRET)));
        store.delete().unwrap();
        assert_eq!(store.load().unwrap(), None);
        assert_eq!(store.inner.store.load().unwrap(), None);
    }

    #[test]
    fn caching_store_forgets_the_token_when_delete_fails() {
        let store = CachingTokenStore::new(Counting {
            fail_delete: true,
            ..Counting::default()
        });
        store.save(&Token::new(SECRET)).unwrap();
        assert!(store.delete().is_err());
        assert_eq!(store.load().unwrap(), None);
    }

    #[test]
    fn memory_store_round_trip() {
        let store = MemoryTokenStore::new();
        assert_eq!(store.load().unwrap(), None);
        store.save(&Token::new(SECRET)).unwrap();
        assert_eq!(store.load().unwrap(), Some(Token::new(SECRET)));
        store.delete().unwrap();
        store.delete().unwrap();
        assert_eq!(store.load().unwrap(), None);
    }
}
