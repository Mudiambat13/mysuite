use argon2::{
    Argon2,
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString, rand_core::OsRng},
};

pub const MIN_PASSWORD_LEN: usize = 12;

pub fn hash(password: &str) -> anyhow::Result<String> {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| anyhow::anyhow!("hachage impossible : {e}"))
}

pub fn verify(password: &str, hash: &str) -> bool {
    match PasswordHash::new(hash) {
        Ok(parsed) => Argon2::default()
            .verify_password(password.as_bytes(), &parsed)
            .is_ok(),
        Err(_) => false,
    }
}

pub fn validate_strength(password: &str) -> Result<(), &'static str> {
    if password.chars().count() < MIN_PASSWORD_LEN {
        return Err("le mot de passe doit contenir au moins 12 caractères");
    }
    let has_alpha = password.chars().any(|c| c.is_alphabetic());
    let has_digit = password.chars().any(|c| c.is_ascii_digit());
    if !(has_alpha && has_digit) {
        return Err("le mot de passe doit contenir des lettres et des chiffres");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_and_verify() {
        let h = hash("Correct-Horse-42").unwrap();
        assert!(verify("Correct-Horse-42", &h));
        assert!(!verify("wrong", &h));
        assert!(!verify("x", "not-a-hash"));
    }

    #[test]
    fn strength_rules() {
        assert!(validate_strength("short1").is_err());
        assert!(validate_strength("onlyletterslongenough").is_err());
        assert!(validate_strength("LongEnough-Pass-123").is_ok());
    }
}
