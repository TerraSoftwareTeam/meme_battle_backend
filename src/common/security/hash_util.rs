use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2, Params,
};
use hmac::digest::InvalidLength;
use hmac::{Hmac, Mac};
use sha2::Sha256;

use crate::common::security::jwt::KEYS;

pub fn hash_password(password: &str) -> Result<String, argon2::Error> {
    let salt = SaltString::generate(&mut OsRng);

    let params = Params::default();

    let argon2 = Argon2::new_with_secret(
        &KEYS.argon_secret,
        argon2::Algorithm::Argon2id,
        argon2::Version::V0x13,
        params,
    )?;

    // Hash password to PHC string ($argon2id$v=19$...)
    let hash_password = argon2
        .hash_password(password.as_bytes(), &salt)
        .map_err(|e| {
            tracing::error!("Error hashing password: {}", e);
            argon2::Error::AlgorithmInvalid
        })?;

    Ok(hash_password.to_string())
}

/// Verify that a password matches the provided hash using a secret key.
pub fn verify_password(password_hash: &str, password: &str) -> bool {
    let parsed_hash = match PasswordHash::new(password_hash) {
        Ok(hash) => hash,
        Err(e) => {
            tracing::error!("Error parsing password hash: {}", e);
            return false;
        }
    };

    let params = Params::default();

    let argon2 = match Argon2::new_with_secret(
        &KEYS.argon_secret,
        argon2::Algorithm::Argon2id,
        argon2::Version::V0x13,
        params,
    ) {
        Ok(a) => a,
        Err(e) => {
            tracing::error!("Error initializing Argon2 for verification: {}", e);
            return false;
        }
    };

    argon2
        .verify_password(password.as_bytes(), &parsed_hash)
        .is_ok()
}

type HmacSha256 = Hmac<Sha256>;

pub fn hash_refresh_token(token: &str) -> Result<String, InvalidLength> {
    let mut mac = HmacSha256::new_from_slice(&KEYS.hmac_secret)?;

    mac.update(token.as_bytes());

    let result = mac.finalize();
    Ok(hex::encode(result.into_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_password_hash_and_verify() {
        let password = "super_secret_password";

        let hash = hash_password(password).expect("Failed to hash password");

        assert!(verify_password(&hash, password));

        assert!(!verify_password(&hash, "wrong_password"));
    }

    #[test]
    fn test_refresh_token_hash() {
        let token = "some_random_refresh_token_123";

        let hash1 = hash_refresh_token(token).expect("Failed to hash refresh token");
        let hash2 = hash_refresh_token(token).expect("Failed to hash refresh token");

        assert_eq!(hash1, hash2);
    }
}
