use std::sync::Arc;
use std::time::{Duration, Instant};

use jsonwebtoken::{decode, decode_header, jwk::JwkSet, Algorithm, DecodingKey, Validation};
use reqwest::Client;
use tokio::sync::{Mutex, RwLock};

const JWKS_REFRESH_FLOOR: Duration = Duration::from_secs(30);

#[derive(Clone, Debug)]
pub(crate) struct CloudflareAccessConfig {
    pub(crate) issuer: String,
    pub(crate) audience: String,
    pub(crate) jwks_url: String,
}

#[derive(Clone)]
pub(crate) struct CloudflareAccessVerifier {
    issuer: Arc<str>,
    audience: Arc<str>,
    jwks_url: Arc<str>,
    client: Client,
    keys: Arc<RwLock<JwkSet>>,
    last_refresh_attempt: Arc<Mutex<Option<Instant>>>,
}

#[derive(Debug)]
enum VerificationError {
    UnknownKey,
    Invalid,
}

impl CloudflareAccessVerifier {
    pub(crate) fn new(config: CloudflareAccessConfig) -> Result<Self, String> {
        if !config.issuer.starts_with("https://") {
            return Err("Cloudflare Access issuer must use https".to_string());
        }
        if !config.jwks_url.starts_with("https://") {
            return Err("Cloudflare Access JWKS URL must use https".to_string());
        }
        if config.audience.trim().is_empty() {
            return Err("Cloudflare Access audience must not be empty".to_string());
        }
        let client = Client::builder()
            .https_only(true)
            .timeout(Duration::from_secs(8))
            .build()
            .map_err(|error| format!("cannot construct Cloudflare Access JWKS client: {error}"))?;
        Ok(Self {
            issuer: Arc::from(config.issuer),
            audience: Arc::from(config.audience),
            jwks_url: Arc::from(config.jwks_url),
            client,
            keys: Arc::new(RwLock::new(JwkSet { keys: Vec::new() })),
            last_refresh_attempt: Arc::new(Mutex::new(None)),
        })
    }

    #[cfg(test)]
    fn from_jwks(config: CloudflareAccessConfig, keys: JwkSet) -> Self {
        Self {
            issuer: Arc::from(config.issuer),
            audience: Arc::from(config.audience),
            jwks_url: Arc::from(config.jwks_url),
            client: Client::new(),
            keys: Arc::new(RwLock::new(keys)),
            last_refresh_attempt: Arc::new(Mutex::new(Some(Instant::now()))),
        }
    }

    /// Validate a Cloudflare Access application assertion without making the
    /// Runtime's local availability depend on network reachability. JWKS are
    /// loaded lazily on the first Access-authenticated request and refreshed
    /// only when a previously unknown `kid` appears.
    pub(crate) async fn verify(&self, token: &str) -> bool {
        match self.verify_cached(token).await {
            Ok(()) => true,
            Err(VerificationError::Invalid) => false,
            Err(VerificationError::UnknownKey) => {
                if self.refresh_keys_if_due().await.is_err() {
                    return false;
                }
                self.verify_cached(token).await.is_ok()
            }
        }
    }

    async fn verify_cached(&self, token: &str) -> Result<(), VerificationError> {
        let header = decode_header(token).map_err(|_| VerificationError::Invalid)?;
        if header.alg != Algorithm::RS256 {
            return Err(VerificationError::Invalid);
        }
        let kid = header.kid.ok_or(VerificationError::Invalid)?;
        let keys = self.keys.read().await;
        let jwk = keys.find(&kid).ok_or(VerificationError::UnknownKey)?;
        let key = DecodingKey::from_jwk(jwk).map_err(|_| VerificationError::Invalid)?;
        let mut validation = Validation::new(Algorithm::RS256);
        // Keep clock-skew tolerance explicit rather than inheriting a crate default.
        validation.leeway = 30;
        validation.set_audience(&[self.audience.as_ref()]);
        validation.set_issuer(&[self.issuer.as_ref()]);
        validation.set_required_spec_claims(&["exp", "iss", "aud"]);
        decode::<serde_json::Value>(token, &key, &validation)
            .map(|_| ())
            .map_err(|_| VerificationError::Invalid)
    }

    async fn refresh_keys_if_due(&self) -> Result<(), String> {
        let mut last_attempt = self.last_refresh_attempt.lock().await;
        if last_attempt.is_some_and(|instant| instant.elapsed() < JWKS_REFRESH_FLOOR) {
            return Ok(());
        }
        *last_attempt = Some(Instant::now());
        let fresh = fetch_jwks(&self.client, &self.jwks_url).await?;
        if fresh.keys.is_empty() {
            return Err("Cloudflare Access JWKS refresh returned no keys".to_string());
        }
        *self.keys.write().await = fresh;
        Ok(())
    }
}

async fn fetch_jwks(client: &Client, jwks_url: &str) -> Result<JwkSet, String> {
    client
        .get(jwks_url)
        .send()
        .await
        .map_err(|error| format!("cannot fetch Cloudflare Access JWKS: {error}"))?
        .error_for_status()
        .map_err(|error| format!("Cloudflare Access JWKS endpoint rejected request: {error}"))?
        .json::<JwkSet>()
        .await
        .map_err(|error| format!("cannot decode Cloudflare Access JWKS: {error}"))
}

#[cfg(test)]
mod tests {
    use super::{CloudflareAccessConfig, CloudflareAccessVerifier};
    use jsonwebtoken::{encode, jwk::JwkSet, Algorithm, EncodingKey, Header};
    use serde::Serialize;
    use std::time::{SystemTime, UNIX_EPOCH};

    const TEST_PRIVATE_KEY: &str = r#"-----BEGIN PRIVATE KEY-----
MIIEvAIBADANBgkqhkiG9w0BAQEFAASCBKYwggSiAgEAAoIBAQCplxJSMojjP1ur
Os7dS9GyY5s+T3stmFcVpm4oLIGtNlXi+x1l7ujUNNa/kuK0Qinz7spfQaJJVdc+
W7s5Ul3CyYjtWP3KBIJye18pYYR976iv1J3y9/LILTerjERVw8lXvS1iAFQ7Y5hm
wyXphIQrR4SMRFN5EshO3ygeDKhS/K8nbkXp/dsa2s9AcMejRInzrv63H2mZSHeg
EBmILS9UqkS57WDWP41sFp4BEYniOWrwFIg8VBbpiAEHBAV4kJ4MYXAkzYA0IBP+
tDBmbZNZoeHckCG6CjUM5X0UbOZk0FMYOo6YYTW6BA8N23xstzaJ5Xt12zdsAsUy
H5ybHfdNAgMBAAECggEABTeAeMbVLyhjyYClnGkYqkQmImSPhXeKNkBIYzP7STjC
q5jN7rTKtLxrXrlGAAWJBNfzobqDI35ggKqRt9Gw0K0iaSqzo+M/oAXrh3pYeQdG
SSJhOXgnH8FEVSKbd4fuSaSoILuh27HUnlSidex9pFcu3KG9b5wETWjP8xywkNzo
L/pSHo5x4ZhgjnvE+r7SmoJ7RTA4n1rYGSOpGKRQnvoGTfSEXNToSr+bVslJ/NWP
UPAh1PMIJlDQI2SZRZJkkHJcswI9U6u7TK3jGDLyzNJx81HNTMr0L3R+ecHbgyc9
86D9GKxAGIh7Es1rW+hESk2NKWSM287gyooV6iKggQKBgQDb3WRcd4BfwKQuOSaM
XrwObOuh5uDh4Nr+DhJ1YjxvN9BiyE8kCo1LWvWeU7bNk21i5t/hIy1r4E24zB/N
SR0apxLryS9KzMIjiUBzH7BsEmNdP4qwyc/EeFv7o179PO+ayur1o9gijtZnUgTH
G2cJu2RXe38zBvXIGWHvwbKrewKBgQDFdmsrXhne5yIJzt4b9tgOqKg3RKBU+wj5
EH9jmhQlSDKb2eUN5O6PUSRKCjohHhex4KQMCk+r4XyCndhz2KL88J4eFqxjYic2
gHndAuyExIvhV3cj6RW+fx4V+yu3wfDVZdORohQm4PJGzxPk5SuUGXH4bfOassxn
lVP3fOfp1wKBgA8Gt3g2Vpi0ssPR9hd71gBqY0RCYjYtxum8DnjlSNoVB3Ho3LfK
3NM8mTLD5+du3vf2bXCWleEciFNL6BSAnbOXnYxtyISlL9N76uKzVLxeGVpjIFhq
wn9b9nVhOfm2s21x1tMI6pmaB38yNM9iyQz6OKZd81iKbjvJuE7JfyuHAoGAJF6u
WJuJelPqIhJXOKFbpD+OVDewrFZcjbtrK0ZK5Z8Jq0kT9l4vTnhsjbKaiFUJmjq9
HHadvBPZIhm+r3+8bYhIJ1SXxepjPJenWnzaYY3uEcBRcmzRE3hIa1YK9FqlaDjM
IivPOGYAWeh0SpmnUCzroA1obBr4qS+I+rGn6ZsCgYBPeQILSZOH9xtzQ2twVYJJ
EhygLWgSrUVf3XtCHsCeurPqpy78sJQjt03omQ/LNuM/jWoGtRX0SHC+4ift5CYQ
YzSJWr0ZdiEzUnMgYJO4/X+kDxRv95qDd/pCsNDBqvNC3G5/g8ac4SMxxiWg/Crg
OB+Y0ifP2QwnQFxNXvlKKA==
-----END PRIVATE KEY-----"#;

    const TEST_JWKS: &str = r#"{"keys":[{"kty":"RSA","kid":"test-key","use":"sig","alg":"RS256","n":"qZcSUjKI4z9bqzrO3UvRsmObPk97LZhXFaZuKCyBrTZV4vsdZe7o1DTWv5LitEIp8-7KX0GiSVXXPlu7OVJdwsmI7Vj9ygSCcntfKWGEfe-or9Sd8vfyyC03q4xEVcPJV70tYgBUO2OYZsMl6YSEK0eEjERTeRLITt8oHgyoUvyvJ25F6f3bGtrPQHDHo0SJ867-tx9pmUh3oBAZiC0vVKpEue1g1j-NbBaeARGJ4jlq8BSIPFQW6YgBBwQFeJCeDGFwJM2ANCAT_rQwZm2TWaHh3JAhugo1DOV9FGzmZNBTGDqOmGE1ugQPDdt8bLc2ieV7dds3bALFMh-cmx33TQ","e":"AQAB"}]}"#;

    #[derive(Serialize)]
    struct Claims<'a> {
        exp: u64,
        iss: &'a str,
        aud: [&'a str; 1],
        sub: &'a str,
    }

    fn verifier() -> CloudflareAccessVerifier {
        let keys: JwkSet = serde_json::from_str(TEST_JWKS).unwrap();
        CloudflareAccessVerifier::from_jwks(
            CloudflareAccessConfig {
                issuer: "https://access.example.com".to_string(),
                audience: "runtime-audience".to_string(),
                jwks_url: "https://access.example.com/certs".to_string(),
            },
            keys,
        )
    }

    fn token(issuer: &str, audience: &str, exp: u64) -> String {
        let mut header = Header::new(Algorithm::RS256);
        header.kid = Some("test-key".to_string());
        encode(
            &header,
            &Claims {
                exp,
                iss: issuer,
                aud: [audience],
                sub: "test-user",
            },
            &EncodingKey::from_rsa_pem(TEST_PRIVATE_KEY.as_bytes()).unwrap(),
        )
        .unwrap()
    }

    #[tokio::test]
    async fn validates_signature_issuer_audience_and_expiry_contract() {
        let verifier = verifier();
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        assert!(
            verifier
                .verify(&token(
                    "https://access.example.com",
                    "runtime-audience",
                    now + 300,
                ))
                .await
        );
        assert!(
            !verifier
                .verify(&token(
                    "https://other.example.com",
                    "runtime-audience",
                    now + 300,
                ))
                .await
        );
        assert!(
            !verifier
                .verify(&token(
                    "https://access.example.com",
                    "other-audience",
                    now + 300,
                ))
                .await
        );
        assert!(
            !verifier
                .verify(&token(
                    "https://access.example.com",
                    "runtime-audience",
                    now.saturating_sub(120),
                ))
                .await
        );
        assert!(!verifier.verify("not-a-jwt").await);
    }
}
