//! Unit tests for the API layer (JWT, claims, state).

#[cfg(test)]
mod tests {
    use crate::*;
    use rstest::rstest;

    // ── JWT token tests ──

    #[rstest]
    fn test_create_and_verify_token() {
        let secret = "test-secret-key-123";
        let user_id = 42;
        let github_id = 123456;
        let login = "testuser";

        let token = create_token(secret, user_id, github_id, login)
            .expect("Token creation should succeed");
        assert!(!token.is_empty());

        let claims = verify_token(secret, &token)
            .expect("Token verification should succeed");
        assert_eq!(claims.sub, user_id);
        assert_eq!(claims.github_id, github_id);
        assert_eq!(claims.github_login, login);
    }

    #[rstest]
    fn test_verify_invalid_token() {
        let result = verify_token("secret", "invalid.token.here");
        assert!(result.is_err());
    }

    #[rstest]
    fn test_verify_wrong_secret() {
        let secret = "correct-secret";
        let token = create_token(secret, 1, 1, "user").unwrap();
        let result = verify_token("wrong-secret", &token);
        assert!(result.is_err());
    }

    #[rstest]
    fn test_verify_empty_token() {
        let result = verify_token("secret", "");
        assert!(result.is_err());
    }

    #[rstest]
    fn test_verify_malformed_token() {
        let result = verify_token("secret", "not.a.jwt");
        assert!(result.is_err());
    }

    #[rstest]
    fn test_token_is_unique() {
        let secret = "secret";
        let token1 = create_token(secret, 1, 100, "a").unwrap();
        let token2 = create_token(secret, 2, 200, "b").unwrap();
        assert_ne!(token1, token2);
    }

    #[rstest]
    fn test_token_contains_expected_claims() {
        let secret = "my-secret";
        let token = create_token(secret, 7, 999, "example").unwrap();
        let claims = verify_token(secret, &token).unwrap();
        assert_eq!(claims.sub, 7);
        assert_eq!(claims.github_id, 999);
        assert_eq!(claims.github_login, "example");
    }

    #[rstest]
    fn test_token_expiry_is_future() {
        let secret = "secret";
        let token = create_token(secret, 1, 1, "user").unwrap();
        let claims = verify_token(secret, &token).unwrap();
        let now = chrono::Utc::now().timestamp() as usize;
        assert!(claims.exp > now, "Token expiry should be in the future");
    }

    #[rstest]
    fn test_token_with_special_characters_in_login() {
        let secret = "secret";
        let login = "user-name_test.123";
        let token = create_token(secret, 1, 1, login).unwrap();
        let claims = verify_token(secret, &token).unwrap();
        assert_eq!(claims.github_login, login);
    }

    // ── Claims serialization tests ──

    #[rstest]
    fn test_claims_serialization_roundtrip() {
        let claims = Claims {
            sub: 10,
            github_id: 555,
            github_login: "serializer".to_string(),
            exp: 9999999999,
            iat: 1111111111,
        };

        let json = serde_json::to_string(&claims).expect("Serialize");
        let parsed: Claims = serde_json::from_str(&json).expect("Deserialize");

        assert_eq!(parsed.sub, claims.sub);
        assert_eq!(parsed.github_id, claims.github_id);
        assert_eq!(parsed.github_login, claims.github_login);
    }

    #[rstest]
    fn test_claims_json_has_expected_fields() {
        let claims = Claims {
            sub: 1,
            github_id: 2,
            github_login: "test".into(),
            exp: 100,
            iat: 50,
        };
        let json = serde_json::to_string(&claims).unwrap();
        assert!(json.contains("\"sub\":1"));
        assert!(json.contains("\"github_id\":2"));
        assert!(json.contains("\"github_login\":\"test\""));
    }
}
