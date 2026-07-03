use argon2::{
    Argon2, PasswordHash, PasswordHasher, PasswordVerifier,
    password_hash::{SaltString, rand_core::OsRng},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::Error;
use crate::Result;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AuthClaim {
    pub user_id: u64,
    pub uuid: uuid::Uuid,
    pub exp: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ResetClaim {
    pub user_id: u64,
    pub email: String,
    /// Fingerprint of the password hash the token was issued against. Changing
    /// the password changes the hash, so this stops matching and the link
    /// becomes single-use. See `password_fingerprint`.
    pub pw: String,
    pub exp: usize,
}

/// A short, non-reversible fingerprint of a stored password hash. Embedded in
/// reset tokens so a link stops working once the password is changed (the argon2
/// hash — salt included — differs even for the same password). Not the hash
/// itself, so the token never carries anything usable to attack it offline.
pub fn password_fingerprint(password_hash: &str) -> String {
    let digest = Sha256::digest(password_hash.as_bytes());
    hex::encode(&digest[..8])
}

pub fn hash_password(password: &str) -> Result<String> {
    Ok(Argon2::default()
        .hash_password(password.as_bytes(), &SaltString::generate(&mut OsRng))
        .map_err(|_| Error::Unauthorized)?
        .to_string())
}

pub fn verify_password(password: &str, hash: &str) -> bool {
    let hash = match PasswordHash::new(hash) {
        Ok(hash) => hash,
        Err(_) => return false,
    };

    Argon2::default()
        .verify_password(password.as_bytes(), &hash)
        .is_ok()
}

#[cfg(test)]
mod tests {
    use super::{hash_password, password_fingerprint};

    #[test]
    fn fingerprint_is_stable_and_short() {
        let hash = hash_password("hunter2").unwrap();
        assert_eq!(password_fingerprint(&hash), password_fingerprint(&hash));
        assert_eq!(password_fingerprint(&hash).len(), 16); // 8 bytes hex
    }

    #[test]
    fn fingerprint_changes_with_the_hash() {
        // Same password, different salt -> different hash -> different
        // fingerprint, which is what makes a used reset link single-use.
        let a = hash_password("hunter2").unwrap();
        let b = hash_password("hunter2").unwrap();
        assert_ne!(a, b);
        assert_ne!(password_fingerprint(&a), password_fingerprint(&b));
    }
}
