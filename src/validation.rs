//! Input validation. Everything that comes from a request body is checked here,
//! before it reaches the database. Functions return the cleaned value.

use crate::error::AppError;

const MAX_EMAIL_LEN: usize = 254;
const MIN_PASSWORD_LEN: usize = 8;
const MAX_PASSWORD_LEN: usize = 128;
pub const MAX_TITLE_LEN: usize = 200;
pub const MAX_BODY_LEN: usize = 10_000;

fn invalid<T>(message: &str) -> Result<T, AppError> {
    Err(AppError::Validation(message.to_string()))
}

/// Trim, lower-case and sanity-check an email. This is a format check, not proof
/// that the address exists.
pub fn email(raw: &str) -> Result<String, AppError> {
    let email = raw.trim().to_lowercase();
    let valid = email.len() <= MAX_EMAIL_LEN
        && email.chars().all(|c| !c.is_whitespace() && !c.is_control())
        && email.split_once('@').is_some_and(|(local, domain)| {
            !local.is_empty()
                && !domain.contains('@')
                && domain.contains('.')
                && !domain.starts_with('.')
                && !domain.ends_with('.')
        });
    if valid {
        Ok(email)
    } else {
        invalid("email is not a valid address")
    }
}

pub fn password(raw: &str) -> Result<&str, AppError> {
    let len = raw.chars().count();
    if (MIN_PASSWORD_LEN..=MAX_PASSWORD_LEN).contains(&len) {
        Ok(raw)
    } else {
        invalid("password must be 8 to 128 characters")
    }
}

pub fn title(raw: &str) -> Result<String, AppError> {
    let title = raw.trim();
    if title.is_empty() || title.chars().count() > MAX_TITLE_LEN {
        return invalid("title must be 1 to 200 characters");
    }
    Ok(title.to_string())
}

pub fn body(raw: &str) -> Result<String, AppError> {
    if raw.chars().count() > MAX_BODY_LEN {
        return invalid("body must be at most 10000 characters");
    }
    Ok(raw.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn email_is_trimmed_and_lower_cased() {
        assert_eq!(email("  Ann@Example.COM ").unwrap(), "ann@example.com");
    }

    #[test]
    fn bad_emails_are_rejected() {
        for bad in [
            "",
            "plain",
            "@x.com",
            "a@b",
            "a@@b.com",
            "a b@c.com",
            "a@.com",
            "a@b.",
            "a@b@c.com",
        ] {
            assert!(email(bad).is_err(), "{bad:?} should be rejected");
        }
        assert!(email(&format!("{}@x.com", "a".repeat(250))).is_err());
    }

    #[test]
    fn password_length_is_counted_in_characters() {
        assert!(password("1234567").is_err());
        assert!(password("12345678").is_ok());
        assert!(password("пароль12").is_ok());
        assert!(password(&"x".repeat(129)).is_err());
    }

    #[test]
    fn title_is_trimmed_and_bounded() {
        assert_eq!(title("  hi  ").unwrap(), "hi");
        assert!(title("   ").is_err());
        assert!(title(&"x".repeat(201)).is_err());
        assert!(title(&"x".repeat(200)).is_ok());
    }

    #[test]
    fn body_is_bounded() {
        assert!(body("").is_ok());
        assert!(body(&"x".repeat(10_001)).is_err());
    }
}
