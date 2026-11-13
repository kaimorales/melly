//! App-owned Sign in with ChatGPT. Never imports another application's session.
use crate::credentials::SecretStore;
use anyhow::{Result, anyhow, bail, ensure};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode, decode_header, jwk::JwkSet};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};
use tokio_util::sync::CancellationToken;
use url::Url;

const ISSUER: &str = "https://auth.openai.com";
const SCOPES: &str =
    "openid profile email offline_access resource.invoke chatgpt.tokens.use.direct";
const RESOURCE: &str = "https://api.openai.com/v1";
const SESSION_KEY: &str = "chatgpt.session.v1";
const REGISTRATION_KEY: &str = "chatgpt.registration.v1";
const HOST_KEY: &str = "chatgpt.host.v1";
const GENERATION_KEY: &str = "chatgpt.generation.v1";
const AUTHORIZE: &str = "https://auth.openai.com/api/accounts/authorize";
const TOKEN: &str = "https://auth.openai.com/api/accounts/oauth/token";
const DISCOVERY: &str = "https://auth.openai.com/.well-known/openid-configuration";
const JWKS: &str = "https://auth.openai.com/.well-known/jwks.json";
const SIGN_IN_SECONDS: u64 = 300;
const MAX_RESPONSE: usize = 256 * 1024;
static COMMIT: Mutex<()> = Mutex::new(());
static REFRESH: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

#[derive(Serialize, Deserialize)]
struct Registration {
    client_id: String,
    identity: Option<Identity>,
}

struct Pending {
    state: String,
    nonce: String,
    verifier: String,
    redirect_uri: Url,
    expected_client_id: Option<String>,
    expires_at: u64,
}
struct Callback {
    code: String,
    client_id: String,
}
#[derive(Clone, Serialize, Deserialize)]
struct Identity {
    issuer: String,
    subject: String,
    client_id: String,
}
#[derive(Clone, Serialize, Deserialize)]
struct Session {
    identity: Identity,
    access_token: Option<String>,
    refresh_token: Option<String>,
    id_token: String,
    scopes: Vec<String>,
    expires_at: u64,
    #[serde(default)]
    refresh_expires_at: u64,
    earliest_refresh_at: u64,
}
#[derive(Default, Deserialize)]
struct TokenResponse {
    access_token: Option<String>,
    refresh_token: Option<String>,
    id_token: Option<String>,
    token_type: Option<String>,
    scope: Option<String>,
    expires_in: Option<u64>,
    earliest_refresh_at: Option<u64>,
}
impl Pending {
    fn new(redirect_uri: Url, expected_client_id: Option<String>) -> Result<Self> {
        ensure!(
            redirect_uri.scheme() == "http"
                && redirect_uri.host_str() == Some("127.0.0.1")
                && redirect_uri.port().is_some()
                && redirect_uri.path() == "/auth/callback"
                && redirect_uri.username().is_empty()
                && redirect_uri.password().is_none()
                && redirect_uri.query().is_none()
                && redirect_uri.fragment().is_none(),
            "Invalid ChatGPT callback address"
        );
        if let Some(client) = &expected_client_id {
            ensure!(valid_client(client), "Invalid ChatGPT registration");
        }
        Ok(Self {
            state: random(),
            nonce: random(),
            verifier: random(),
            redirect_uri,
            expected_client_id,
            expires_at: now() + SIGN_IN_SECONDS,
        })
    }
    fn authorization_url(&self, host_id: &str, consent: bool) -> Result<Url> {
        let mut url = Url::parse(AUTHORIZE)
            .map_err(|_| anyhow!("Invalid ChatGPT authorization configuration"))?;
        let mut query = url.query_pairs_mut();
        query.append_pair(
            "client_id",
            self.expected_client_id
                .as_deref()
                .unwrap_or("dynamic_agent_client"),
        );
        if self.expected_client_id.is_none() {
            query.append_pair("agent_name_hint", "Melly");
        }
        query
            .append_pair("ext_agent_host_id", host_id)
            .append_pair("response_type", "code")
            .append_pair("redirect_uri", self.redirect_uri.as_str())
            .append_pair("scope", SCOPES)
            .append_pair("resource", RESOURCE)
            .append_pair("state", &self.state)
            .append_pair("nonce", &self.nonce)
            .append_pair("code_challenge_method", "S256")
            .append_pair(
                "code_challenge",
                &URL_SAFE_NO_PAD.encode(Sha256::digest(self.verifier.as_bytes())),
            );
        if consent {
            query.append_pair("prompt", "consent");
        }
        drop(query);
        Ok(url)
    }
    fn callback(&self, url: &Url, now: u64) -> Result<Callback> {
        ensure!(
            now < self.expires_at,
            "ChatGPT sign-in expired; please try again"
        );
        ensure!(
            url.scheme() == self.redirect_uri.scheme()
                && url.host_str() == self.redirect_uri.host_str()
                && url.port() == self.redirect_uri.port()
                && url.path() == self.redirect_uri.path()
                && url.username().is_empty()
                && url.password().is_none()
                && url.fragment().is_none(),
            "ChatGPT callback address did not match"
        );
        let mut query = HashMap::new();
        for (key, value) in url.query_pairs() {
            ensure!(
                query.insert(key.into_owned(), value.into_owned()).is_none(),
                "ChatGPT callback contained duplicate parameters"
            );
        }
        ensure!(
            query
                .get("state")
                .is_some_and(|value| same(value, &self.state)),
            "ChatGPT sign-in state did not match"
        );
        if query.contains_key("error") {
            bail!("ChatGPT sign-in was not authorized; please try again");
        }
        let client_id = match (&self.expected_client_id, query.get("client_id")) {
            (Some(expected), Some(actual)) if expected != actual => {
                bail!("ChatGPT registration did not match")
            }
            (Some(expected), _) => expected.clone(),
            (None, Some(actual)) if valid_client(actual) => actual.clone(),
            _ => bail!("ChatGPT registration was incomplete; please try again"),
        };
        let code = query
            .remove("code")
            .filter(|value| valid_secret(value))
            .ok_or_else(|| anyhow!("ChatGPT did not return an authorization code"))?;
        Ok(Callback { code, client_id })
    }
}
impl Session {
    fn has_permission(&self) -> bool {
        self.identity.issuer == ISSUER
            && !self.identity.subject.is_empty()
            && valid_client(&self.identity.client_id)
            && self
                .scopes
                .iter()
                .any(|value| value == "chatgpt.tokens.use.direct")
            && self.scopes.iter().any(|value| value == "resource.invoke")
    }
    fn can_infer(&self, now: u64) -> bool {
        self.has_permission()
            && self.expires_at > now
            && self.access_token.as_deref().is_some_and(valid_secret)
    }
    fn can_renew(&self, now: u64) -> bool {
        self.has_permission()
            && self.refresh_expires_at > now
            && self.refresh_token.as_deref().is_some_and(valid_secret)
            && self.access_token.as_deref().is_some_and(valid_secret)
    }
}

#[derive(Deserialize)]
struct IdClaims {
    iss: String,
    sub: String,
    iat: u64,
    exp: u64,
    #[serde(default)]
    nonce: Option<String>,
    #[serde(default)]
    azp: Option<String>,
    aud: serde_json::Value,
}

fn verify_identity(
    token: &str,
    keys: &JwkSet,
    client_id: &str,
    nonce: Option<&str>,
    subject: Option<&str>,
) -> Result<Identity> {
    let invalid = || anyhow!("ChatGPT identity could not be verified; please sign in again");
    let header = decode_header(token).map_err(|_| invalid())?;
    ensure!(
        matches!(
            header.alg,
            Algorithm::RS256 | Algorithm::ES256 | Algorithm::ES384 | Algorithm::EdDSA
        ),
        "ChatGPT identity used an unsupported signature"
    );
    let kid = header.kid.as_ref().ok_or_else(invalid)?;
    let key = keys.find(kid).ok_or_else(invalid)?;
    ensure!(
        key.common
            .public_key_use
            .as_ref()
            .is_none_or(|usage| *usage == jsonwebtoken::jwk::PublicKeyUse::Signature)
            && key.common.key_operations.as_ref().is_none_or(
                |operations| operations.contains(&jsonwebtoken::jwk::KeyOperations::Verify)
            )
            && key
                .common
                .key_algorithm
                .as_ref()
                .is_none_or(|algorithm| algorithm.to_string() == format!("{:?}", header.alg)),
        "ChatGPT identity key does not allow this signature"
    );
    let decoding_key = DecodingKey::from_jwk(key).map_err(|_| invalid())?;
    let mut validation = Validation::new(header.alg);
    validation.set_audience(&[client_id]);
    validation.set_issuer(&[ISSUER]);
    validation.set_required_spec_claims(&["exp", "iat", "iss", "aud", "sub"]);
    validation.leeway = 5;
    validation.validate_nbf = true;
    let claims = decode::<IdClaims>(token, &decoding_key, &validation)
        .map_err(|_| invalid())?
        .claims;
    ensure!(
        !claims.sub.trim().is_empty()
            && claims.sub.len() <= 2048
            && claims.iat <= now() + 5
            && claims.exp > claims.iat,
        "ChatGPT identity claims were invalid"
    );
    if let Some(nonce) = nonce {
        ensure!(
            claims
                .nonce
                .as_deref()
                .is_some_and(|value| same(value, nonce)),
            "ChatGPT identity nonce did not match"
        );
    }
    if let Some(subject) = subject {
        ensure!(
            claims.sub == subject,
            "ChatGPT account did not match the selected registration"
        );
    }
    if claims.aud.as_array().is_some_and(|values| values.len() > 1) {
        ensure!(
            claims.azp.as_deref() == Some(client_id),
            "ChatGPT identity audience was ambiguous"
        );
    }
    if let Some(azp) = claims.azp {
        ensure!(
            azp == client_id,
            "ChatGPT identity authorized party did not match"
        );
    }
    Ok(Identity {
        issuer: claims.iss,
        subject: claims.sub,
        client_id: client_id.to_owned(),
    })
}

fn session_from_response(
    response: TokenResponse,
    identity: Identity,
    prior: Option<&Session>,
    now: u64,
) -> Result<Session> {
    if response.access_token.is_some() {
        ensure!(
            response
                .token_type
                .as_deref()
                .is_some_and(|kind| kind.eq_ignore_ascii_case("Bearer")),
            "ChatGPT returned an unsupported token type"
        );
        ensure!(
            response.access_token.as_deref().is_some_and(valid_secret),
            "ChatGPT returned an invalid access token"
        );
        ensure!(
            response
                .expires_in
                .is_some_and(|expiry| expiry > 0 && expiry <= 86400),
            "ChatGPT returned an invalid token lifetime"
        );
    }
    if let Some(refresh) = &response.refresh_token {
        ensure!(
            valid_secret(refresh),
            "ChatGPT returned an invalid refresh token"
        );
    }
    if prior.is_some() {
        ensure!(
            response.access_token.is_some() && response.refresh_token.is_some(),
            "ChatGPT did not return replacement credentials; sign in again"
        );
    }
    let scopes = match response.scope {
        Some(scope) => scope.split_ascii_whitespace().map(str::to_owned).collect(),
        None => prior
            .map(|previous| previous.scopes.clone())
            .unwrap_or_default(),
    };
    let id_token = response
        .id_token
        .or_else(|| prior.map(|previous| previous.id_token.clone()))
        .ok_or_else(|| anyhow!("ChatGPT did not return a verifiable identity"))?;
    let refresh_expires_at = if response.refresh_token.is_some() {
        now.saturating_add(30 * 24 * 60 * 60)
    } else {
        0
    };
    Ok(Session {
        identity,
        access_token: response.access_token,
        refresh_token: response.refresh_token,
        id_token,
        scopes,
        expires_at: now.saturating_add(response.expires_in.unwrap_or(0)),
        refresh_expires_at,
        earliest_refresh_at: response.earliest_refresh_at.unwrap_or(0),
    })
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_secs())
        .unwrap_or(0)
}
fn random() -> String {
    let mut bytes = [0; 32];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}
fn same(a: &str, b: &str) -> bool {
    a.len() == b.len()
        && a.bytes()
            .zip(b.bytes())
            .fold(0u8, |difference, (a, b)| difference | (a ^ b))
            == 0
}
fn valid_client(value: &str) -> bool {
    value.starts_with("oaiapp_")
        && value.len() > 7
        && value.len() <= 512
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
}
fn valid_secret(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 65536
        && !value.chars().any(char::is_whitespace)
        && !value.chars().any(char::is_control)
}
fn get(vault: &dyn SecretStore, id: &str) -> Result<Option<String>> {
    vault
        .get(id)
        .map_err(|_| anyhow!("Credential vault could not be read"))
}
fn put(vault: &dyn SecretStore, id: &str, value: &str) -> Result<()> {
    vault
        .set(id, value)
        .map_err(|_| anyhow!("Credential vault could not be updated"))
}
fn remove(vault: &dyn SecretStore, id: &str) -> Result<()> {
    vault
        .delete(id)
        .map_err(|_| anyhow!("Credential vault could not be updated"))
}
fn encode<T: Serialize>(value: &T) -> Result<String> {
    serde_json::to_string(value).map_err(|_| anyhow!("ChatGPT credentials could not be saved"))
}
fn decode_saved<T: serde::de::DeserializeOwned>(value: &str) -> Result<T> {
    serde_json::from_str(value)
        .map_err(|_| anyhow!("Stored ChatGPT credentials are invalid; please sign in again"))
}
fn check_cancel(cancel: &CancellationToken) -> Result<()> {
    if cancel.is_cancelled() {
        bail!("ChatGPT sign-in cancelled");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ring::signature::KeyPair;
    use serde_json::{Value, json};
    use sha2::{Digest, Sha256};
    fn now() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
    }
    fn pending(client: Option<&str>) -> Pending {
        Pending::new(
            Url::parse("http://127.0.0.1:54321/auth/callback").unwrap(),
            client.map(str::to_owned),
        )
        .unwrap()
    }
    fn callback(pending: &Pending, state: &str, client: Option<&str>) -> Url {
        let mut url = pending.redirect_uri.clone();
        url.query_pairs_mut()
            .append_pair("code", "synthetic-code")
            .append_pair("state", state);
        if let Some(client) = client {
            url.query_pairs_mut().append_pair("client_id", client);
        }
        url
    }
    fn identity() -> Identity {
        Identity {
            issuer: ISSUER.into(),
            subject: "synthetic-subject".into(),
            client_id: "oaiapp_test".into(),
        }
    }
    fn response(scope: Option<&str>) -> TokenResponse {
        TokenResponse {
            access_token: Some("synthetic-access".into()),
            refresh_token: Some("synthetic-refresh".into()),
            id_token: Some("synthetic-identity".into()),
            token_type: Some("Bearer".into()),
            scope: scope.map(str::to_owned),
            expires_in: Some(3600),
            earliest_refresh_at: None,
        }
    }
    fn signed(claims: &Value) -> (String, jsonwebtoken::jwk::JwkSet) {
        let random = ring::rand::SystemRandom::new();
        let pkcs8 = ring::signature::EcdsaKeyPair::generate_pkcs8(
            &ring::signature::ECDSA_P256_SHA256_ASN1_SIGNING,
            &random,
        )
        .unwrap();
        let pair = ring::signature::EcdsaKeyPair::from_pkcs8(
            &ring::signature::ECDSA_P256_SHA256_ASN1_SIGNING,
            pkcs8.as_ref(),
            &random,
        )
        .unwrap();
        let public = pair.public_key().as_ref();
        let jwks = serde_json::from_value(json!({"keys":[{"kty":"EC", "crv":"P-256", "kid":"test", "use":"sig", "alg":"ES256", "x":URL_SAFE_NO_PAD.encode(&public[1..33]), "y":URL_SAFE_NO_PAD.encode(&public[33..65])}]})).unwrap();
        let mut header = jsonwebtoken::Header::new(jsonwebtoken::Algorithm::ES256);
        header.kid = Some("test".into());
        let token = jsonwebtoken::encode(
            &header,
            claims,
            &jsonwebtoken::EncodingKey::from_ec_der(pkcs8.as_ref()),
        )
        .unwrap();
        (token, jwks)
    }
    fn claims() -> Value {
        json!({"iss":ISSUER, "aud":"oaiapp_test", "sub":"synthetic-subject", "nonce":"synthetic-nonce", "iat":now(), "exp":now()+3600})
    }

    #[test]
    fn authorization_has_fresh_pkce_state_nonce_and_explicit_plan_scopes() {
        let p = pending(None);
        let second = pending(None);
        assert_ne!(p.state, second.state);
        assert_ne!(p.nonce, second.nonce);
        assert!(p.verifier.len() >= 43);
        let url = p.authorization_url("urn:uuid:synthetic", false).unwrap();
        assert_eq!(
            url.as_str().split('?').next().unwrap(),
            "https://auth.openai.com/api/accounts/authorize"
        );
        let values: std::collections::HashMap<_, _> = url.query_pairs().into_owned().collect();
        assert_eq!(values["client_id"], "dynamic_agent_client");
        assert_eq!(values["agent_name_hint"], "Melly");
        assert_eq!(values["resource"], RESOURCE);
        assert_eq!(values["scope"], SCOPES);
        assert_eq!(values["code_challenge_method"], "S256");
        assert_eq!(
            values["code_challenge"],
            URL_SAFE_NO_PAD.encode(Sha256::digest(p.verifier.as_bytes()))
        );
        assert_eq!(values["redirect_uri"], p.redirect_uri.as_str());
    }

    #[test]
    fn returning_sign_in_reuses_registration_and_only_requests_consent_when_needed() {
        let p = pending(Some("oaiapp_test"));
        let values: std::collections::HashMap<_, _> = p
            .authorization_url("urn:uuid:synthetic", true)
            .unwrap()
            .query_pairs()
            .into_owned()
            .collect();
        assert_eq!(values["client_id"], "oaiapp_test");
        assert!(!values.contains_key("agent_name_hint"));
        assert_eq!(values["prompt"], "consent");
    }

    #[test]
    fn callback_requires_state_exact_loopback_and_issued_client() {
        let p = pending(None);
        let good = callback(&p, &p.state, Some("oaiapp_test"));
        assert_eq!(p.callback(&good, now()).unwrap().client_id, "oaiapp_test");
        assert_eq!(p.callback(&good, now()).unwrap().code, "synthetic-code");
        assert!(
            p.callback(&callback(&p, "wrong", Some("oaiapp_test")), now())
                .is_err()
        );
        assert!(p.callback(&callback(&p, &p.state, None), now()).is_err());
        assert!(
            p.callback(&callback(&p, &p.state, Some("dynamic_agent_client")), now())
                .is_err()
        );
        let mut duplicate = good.clone();
        duplicate.query_pairs_mut().append_pair("state", &p.state);
        assert!(p.callback(&duplicate, now()).is_err());
        for invalid in [
            good.as_str().replace("127.0.0.1", "localhost"),
            good.as_str().replace("54321", "54322"),
            good.as_str().replace("/auth/callback", "/callback"),
            good.as_str().replace("http:", "https:"),
        ] {
            assert!(p.callback(&Url::parse(&invalid).unwrap(), now()).is_err());
        }
        assert!(p.callback(&good, p.expires_at + 1).is_err());
    }

    #[test]
    fn callback_reauthorization_cannot_replace_client_and_denial_is_sanitized() {
        let p = pending(Some("oaiapp_test"));
        assert_eq!(
            p.callback(&callback(&p, &p.state, None), now())
                .unwrap()
                .client_id,
            "oaiapp_test"
        );
        assert!(
            p.callback(&callback(&p, &p.state, Some("oaiapp_other")), now())
                .is_err()
        );
        let mut denied = p.redirect_uri.clone();
        denied
            .query_pairs_mut()
            .append_pair("state", &p.state)
            .append_pair("error", "access_denied")
            .append_pair("error_description", "secret-provider-payload");
        let error = p
            .callback(&denied, now())
            .err()
            .expect("denial must fail")
            .to_string();
        assert!(!error.contains("secret-provider-payload"));
    }

    #[test]
    fn id_token_verifies_signature_issuer_audience_expiry_nonce_and_returning_subject() {
        let (token, keys) = signed(&claims());
        let verified = verify_identity(
            &token,
            &keys,
            "oaiapp_test",
            Some("synthetic-nonce"),
            Some("synthetic-subject"),
        )
        .unwrap();
        assert_eq!(verified.subject, "synthetic-subject");
        assert!(
            verify_identity(&token, &keys, "oaiapp_other", Some("synthetic-nonce"), None).is_err()
        );
        assert!(verify_identity(&token, &keys, "oaiapp_test", Some("wrong-nonce"), None).is_err());
        assert!(
            verify_identity(
                &token,
                &keys,
                "oaiapp_test",
                Some("synthetic-nonce"),
                Some("wrong-subject")
            )
            .is_err()
        );
        let (_, other_keys) = signed(&claims());
        assert!(
            verify_identity(
                &token,
                &other_keys,
                "oaiapp_test",
                Some("synthetic-nonce"),
                None
            )
            .is_err()
        );
        for (key, value) in [
            ("iss", json!("https://example.invalid")),
            ("exp", json!(now() - 60)),
            ("sub", json!("")),
            ("iat", json!(now() + 600)),
        ] {
            let mut bad = claims();
            bad[key] = value;
            let (token, keys) = signed(&bad);
            assert!(
                verify_identity(&token, &keys, "oaiapp_test", Some("synthetic-nonce"), None)
                    .is_err(),
                "claim {key}"
            );
        }
    }

    #[test]
    fn identity_rejects_jwks_keys_not_authorized_for_its_signature() {
        let (token, keys) = signed(&claims());
        let mut encryption = keys.clone();
        encryption.keys[0].common.public_key_use =
            Some(jsonwebtoken::jwk::PublicKeyUse::Encryption);
        assert!(
            verify_identity(
                &token,
                &encryption,
                "oaiapp_test",
                Some("synthetic-nonce"),
                None
            )
            .is_err()
        );
        let mut wrong_algorithm = keys.clone();
        wrong_algorithm.keys[0].common.key_algorithm = Some(jsonwebtoken::jwk::KeyAlgorithm::ES384);
        assert!(
            verify_identity(
                &token,
                &wrong_algorithm,
                "oaiapp_test",
                Some("synthetic-nonce"),
                None
            )
            .is_err()
        );
        let mut wrong_operation = keys;
        wrong_operation.keys[0].common.key_operations =
            Some(vec![jsonwebtoken::jwk::KeyOperations::Encrypt]);
        assert!(
            verify_identity(
                &token,
                &wrong_operation,
                "oaiapp_test",
                Some("synthetic-nonce"),
                None
            )
            .is_err()
        );
    }

    #[test]
    fn identity_only_or_missing_grant_never_enables_plan_usage() {
        for scope in [
            None,
            Some("openid profile email"),
            Some("resource.invoke chatgpt.tokens.use.directly"),
            Some("chatgpt.tokens.use.direct"),
        ] {
            assert!(
                !session_from_response(response(scope), identity(), None, now())
                    .unwrap()
                    .can_infer(now())
            );
        }
        let granted =
            session_from_response(response(Some(SCOPES)), identity(), None, now()).unwrap();
        assert!(granted.can_infer(now()));
        assert!(!granted.can_infer(now() + 3601));
        let mut invalid = response(Some(SCOPES));
        invalid.token_type = Some("Basic".into());
        assert!(session_from_response(invalid, identity(), None, now()).is_err());
    }

    #[test]
    fn refreshed_tokens_replace_together_and_do_not_infer_new_scopes() {
        let old = session_from_response(response(Some(SCOPES)), identity(), None, now()).unwrap();
        let mut new = response(None);
        new.access_token = Some("synthetic-new-access".into());
        new.refresh_token = Some("synthetic-new-refresh".into());
        new.id_token = None;
        let refreshed = session_from_response(new, identity(), Some(&old), now() + 50).unwrap();
        assert_eq!(
            refreshed.access_token.as_deref(),
            Some("synthetic-new-access")
        );
        assert_eq!(
            refreshed.refresh_token.as_deref(),
            Some("synthetic-new-refresh")
        );
        assert_eq!(refreshed.scopes, old.scopes);
        assert!(refreshed.can_infer(now() + 50));
        let restricted =
            session_from_response(response(Some("openid")), identity(), Some(&old), now()).unwrap();
        assert!(!restricted.can_infer(now()));
    }

    struct FakeService {
        reply: Mutex<Option<Result<TokenResponse>>>,
        keys: Mutex<Option<JwkSet>>,
        calls: std::sync::atomic::AtomicUsize,
        vault: Arc<crate::credentials::MemorySecrets>,
        disconnect_on_exchange: bool,
    }
    #[async_trait::async_trait]
    impl AuthService for FakeService {
        async fn exchange(
            &self,
            form: &[(&str, &str)],
            _: &CancellationToken,
        ) -> Result<TokenResponse> {
            self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            let form: HashMap<_, _> = form.iter().copied().collect();
            assert_eq!(form["client_id"], "oaiapp_test");
            assert_eq!(form["resource"], RESOURCE);
            let registration: Registration =
                decode_saved(&get(self.vault.as_ref(), REGISTRATION_KEY)?.unwrap()).unwrap();
            assert_eq!(
                registration.client_id, "oaiapp_test",
                "issued client is saved before exchange"
            );
            if form["grant_type"] == "refresh_token" {
                assert!(!form.contains_key("scope"));
                assert_eq!(form["refresh_token"], "synthetic-refresh");
            } else {
                assert_eq!(form["grant_type"], "authorization_code");
                assert_eq!(form["code"], "synthetic-code");
                assert!(form["code_verifier"].len() >= 43);
                assert!(form["redirect_uri"].starts_with("http://127.0.0.1:"));
            }
            if self.disconnect_on_exchange {
                disconnect(self.vault.as_ref())?;
            }
            self.reply.lock().unwrap().take().expect("one refresh only")
        }
        async fn keys(&self, _: &CancellationToken) -> Result<JwkSet> {
            Ok(self.keys.lock().unwrap().take().unwrap())
        }
    }
    fn fake(
        vault: Arc<crate::credentials::MemorySecrets>,
        reply: Result<TokenResponse>,
    ) -> FakeService {
        FakeService {
            vault,
            reply: Mutex::new(Some(reply)),
            keys: Mutex::new(None),
            calls: std::sync::atomic::AtomicUsize::new(0),
            disconnect_on_exchange: false,
        }
    }
    fn saved_expired() -> Arc<crate::credentials::MemorySecrets> {
        let vault = Arc::new(crate::credentials::MemorySecrets::default());
        let mut session =
            session_from_response(response(Some(SCOPES)), identity(), None, now()).unwrap();
        session.expires_at = now() - 1;
        put(vault.as_ref(), SESSION_KEY, &encode(&session).unwrap()).unwrap();
        put(
            vault.as_ref(),
            REGISTRATION_KEY,
            &encode(&Registration {
                client_id: "oaiapp_test".into(),
                identity: Some(identity()),
            })
            .unwrap(),
        )
        .unwrap();
        vault
    }

    #[test]
    fn expired_access_remains_configured_while_its_verified_grant_can_renew() {
        let vault = saved_expired();
        assert!(is_connected(vault.as_ref()));
        let raw = get(vault.as_ref(), SESSION_KEY).unwrap().unwrap();
        let mut expired: Value = serde_json::from_str(&raw).unwrap();
        expired["refresh_expires_at"] = json!(now() - 1);
        put(vault.as_ref(), SESSION_KEY, &expired.to_string()).unwrap();
        assert!(!is_connected(vault.as_ref()));
        expired["refresh_expires_at"] = json!(now() + 3600);
        expired["scopes"] = json!(["openid", "email", "profile"]);
        put(vault.as_ref(), SESSION_KEY, &expired.to_string()).unwrap();
        assert!(!is_connected(vault.as_ref()));
    }
    fn refresh_response() -> TokenResponse {
        let mut reply = response(None);
        reply.id_token = None;
        reply.access_token = Some("synthetic-new-access".into());
        reply.refresh_token = Some("synthetic-new-refresh".into());
        reply
    }

    #[tokio::test]
    async fn simultaneous_access_serializes_rotating_refresh_and_persists_new_pair() {
        let vault = saved_expired();
        let service = fake(vault.clone(), Ok(refresh_response()));
        let (first, second) = tokio::join!(
            access_token_with(vault.clone(), &service),
            access_token_with(vault.clone(), &service)
        );
        assert_eq!(first.unwrap(), "synthetic-new-access");
        assert_eq!(second.unwrap(), "synthetic-new-access");
        assert_eq!(service.calls.load(std::sync::atomic::Ordering::SeqCst), 1);
        let updated: Session =
            decode_saved(&get(vault.as_ref(), SESSION_KEY).unwrap().unwrap()).unwrap();
        assert_eq!(
            updated.refresh_token.as_deref(),
            Some("synthetic-new-refresh")
        );
        assert!(is_connected(vault.as_ref()));
    }

    #[tokio::test]
    async fn terminal_refresh_error_clears_tokens_but_temporary_error_retains_them() {
        for (code, clear) in [
            ("invalid_grant", true),
            ("refresh_token_reused", true),
            ("request_failed", false),
        ] {
            let vault = saved_expired();
            let service = fake(
                vault.clone(),
                Err(OAuthFailure {
                    status: if clear { 400 } else { 503 },
                    code,
                    request_id: None,
                }
                .into()),
            );
            assert!(access_token_with(vault.clone(), &service).await.is_err());
            assert_eq!(get(vault.as_ref(), SESSION_KEY).unwrap().is_none(), clear);
            assert!(get(vault.as_ref(), REGISTRATION_KEY).unwrap().is_some());
        }
    }

    #[tokio::test]
    async fn disconnect_during_refresh_never_restores_credentials() {
        let vault = saved_expired();
        let mut service = fake(vault.clone(), Ok(refresh_response()));
        service.disconnect_on_exchange = true;
        assert!(access_token_with(vault.clone(), &service).await.is_err());
        assert!(get(vault.as_ref(), SESSION_KEY).unwrap().is_none());
        assert!(!is_connected(vault.as_ref()));
    }

    struct PausedRefresh {
        inner: FakeService,
        rotated: tokio::sync::Notify,
        deliver: tokio::sync::Notify,
    }
    #[async_trait::async_trait]
    impl AuthService for PausedRefresh {
        async fn exchange(
            &self,
            form: &[(&str, &str)],
            cancel: &CancellationToken,
        ) -> Result<TokenResponse> {
            // The provider has consumed the old refresh token, but its response
            // has not yet reached the application when the caller cancels.
            let response = self.inner.exchange(form, cancel).await;
            self.rotated.notify_one();
            self.deliver.notified().await;
            response
        }
        async fn keys(&self, cancel: &CancellationToken) -> Result<JwkSet> {
            self.inner.keys(cancel).await
        }
    }

    #[tokio::test]
    async fn stopping_a_turn_does_not_lose_a_rotated_refresh_credential() {
        let vault = saved_expired();
        let service = Arc::new(PausedRefresh {
            inner: fake(vault.clone(), Ok(refresh_response())),
            rotated: tokio::sync::Notify::new(),
            deliver: tokio::sync::Notify::new(),
        });
        let caller_vault = vault.clone();
        let caller_service = service.clone();
        let caller =
            tokio::spawn(async move { access_token_owned(caller_vault, caller_service).await });
        service.rotated.notified().await;
        caller.abort();
        assert!(caller.await.is_err());
        service.deliver.notify_one();
        let saved = tokio::time::timeout(Duration::from_millis(200), async {
            loop {
                let session: Session =
                    decode_saved(&get(vault.as_ref(), SESSION_KEY).unwrap().unwrap()).unwrap();
                if session.refresh_token.as_deref() == Some("synthetic-new-refresh") {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await;
        assert!(
            saved.is_ok(),
            "the rotated replacement must be saved after the turn stops"
        );
        assert!(is_connected(vault.as_ref()));
    }

    #[tokio::test]
    async fn scope_removed_on_refresh_is_saved_but_cannot_be_used() {
        let vault = saved_expired();
        let mut response = refresh_response();
        response.scope = Some("openid profile email".into());
        let service = fake(vault.clone(), Ok(response));
        assert!(access_token_with(vault.clone(), &service).await.is_err());
        assert!(!is_connected(vault.as_ref()));
        assert!(get(vault.as_ref(), SESSION_KEY).unwrap().is_some());
        assert!(access_token_with(vault.clone(), &service).await.is_err());
        assert_eq!(service.calls.load(std::sync::atomic::Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn sign_in_runs_loopback_and_saves_only_verified_plan_grant() {
        let vault = Arc::new(crate::credentials::MemorySecrets::default());
        let service = Arc::new(fake(vault.clone(), Ok(response(Some(SCOPES)))));
        let callback_service = service.clone();
        let on_url = Arc::new(move |authorization: String| {
            let authorization = Url::parse(&authorization).unwrap();
            let values: HashMap<_, _> = authorization.query_pairs().into_owned().collect();
            let mut payload = claims();
            payload["nonce"] = json!(values["nonce"]);
            let (token, keys) = signed(&payload);
            let mut response = response(Some(SCOPES));
            response.id_token = Some(token);
            *callback_service.reply.lock().unwrap() = Some(Ok(response));
            *callback_service.keys.lock().unwrap() = Some(keys);
            let mut callback = Url::parse(&values["redirect_uri"]).unwrap();
            callback
                .query_pairs_mut()
                .append_pair("code", "synthetic-code")
                .append_pair("state", &values["state"])
                .append_pair("client_id", "oaiapp_test");
            tokio::spawn(async move {
                reqwest::get(callback).await.unwrap();
            });
        });
        sign_in_with(
            vault.clone(),
            on_url,
            CancellationToken::new(),
            service.as_ref(),
        )
        .await
        .unwrap();
        assert!(is_connected(vault.as_ref()));
        assert_eq!(service.calls.load(std::sync::atomic::Ordering::SeqCst), 1);
        assert!(
            get(vault.as_ref(), HOST_KEY)
                .unwrap()
                .unwrap()
                .starts_with("urn:uuid:")
        );
    }

    #[tokio::test]
    async fn cancelled_listener_closes_promptly_without_tokens() {
        let vault = Arc::new(crate::credentials::MemorySecrets::default());
        let service = fake(vault.clone(), Ok(response(Some(SCOPES))));
        let cancel = CancellationToken::new();
        let trigger = cancel.clone();
        let on_url = Arc::new(move |_| {
            trigger.cancel();
        });
        let result = tokio::time::timeout(
            Duration::from_millis(100),
            sign_in_with(vault.clone(), on_url, cancel, &service),
        )
        .await;
        assert!(result.unwrap().is_err());
        assert_eq!(service.calls.load(std::sync::atomic::Ordering::SeqCst), 0);
        assert!(get(vault.as_ref(), SESSION_KEY).unwrap().is_none());
    }

    #[tokio::test]
    async fn http_errors_do_not_echo_provider_bodies_and_redirects_are_rejected() {
        for (status, body, extra) in [
            (
                "400 Bad Request",
                r#"{"error":"invalid_grant","error_description":"synthetic-secret-provider-payload"}"#,
                "",
            ),
            (
                "302 Found",
                "synthetic-secret-provider-payload",
                "Location: https://example.invalid/collect\r\n",
            ),
        ] {
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let url = format!("http://{}/token", listener.local_addr().unwrap());
            tokio::spawn(async move {
                let (mut stream, _) = listener.accept().await.unwrap();
                let mut request = [0u8; 2048];
                let _ = stream.read(&mut request).await;
                let reply = format!(
                    "HTTP/1.1 {status}\r\nContent-Length: {}\r\n{extra}Connection: close\r\n\r\n{body}",
                    body.len()
                );
                stream.write_all(reply.as_bytes()).await.unwrap();
            });
            let client = reqwest::Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .unwrap();
            let result: Result<serde_json::Value> =
                request_json(client.get(url), &CancellationToken::new()).await;
            let message = result.unwrap_err().to_string();
            assert!(!message.contains("synthetic-secret"));
            assert!(message.contains("ChatGPT"));
            if status.starts_with("400") {
                assert!(message.contains("invalid_grant"));
                assert!(message.contains("400"));
            }
        }
    }
}

#[derive(Deserialize)]
struct Discovery {
    issuer: String,
    authorization_endpoint: String,
    token_endpoint: String,
    jwks_uri: String,
}

fn http_client() -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .https_only(true)
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(20))
        .user_agent("Melly/1.0")
        .build()
        .map_err(|_| anyhow!("ChatGPT connection could not be initialized"))
}

#[derive(Debug)]
struct OAuthFailure {
    status: u16,
    code: &'static str,
    request_id: Option<String>,
}
impl std::fmt::Display for OAuthFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "ChatGPT authorization failed (HTTP {}, {})",
            self.status, self.code
        )?;
        if let Some(id) = &self.request_id {
            write!(f, "; request {id}")?;
        }
        Ok(())
    }
}
impl std::error::Error for OAuthFailure {}
impl OAuthFailure {
    fn terminal_refresh(&self) -> bool {
        matches!(
            self.code,
            "invalid_grant"
                | "invalid_refresh_token"
                | "token_expired"
                | "refresh_token_expired"
                | "refresh_token_invalidated"
                | "refresh_token_reused"
        )
    }
}

async fn request_json<T: serde::de::DeserializeOwned>(
    request: reqwest::RequestBuilder,
    cancel: &CancellationToken,
) -> Result<T> {
    tokio::select! {
        biased;
        _ = cancel.cancelled() => bail!("ChatGPT sign-in cancelled"),
        result = async {
            let mut response = request.send().await.map_err(|_| anyhow!("ChatGPT authorization network request failed; please try again"))?;
            let status = response.status();
            let request_id = response.headers().get("x-request-id").or_else(|| response.headers().get("openai-request-id"))
                .and_then(|value| value.to_str().ok()).filter(|value| value.len() <= 128 && value.bytes().all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))).map(str::to_owned);
            ensure!(!status.is_redirection(), "ChatGPT authorization refused an HTTP redirect");
            ensure!(response.content_length().is_none_or(|length| length <= MAX_RESPONSE as u64), "ChatGPT authorization response was too large");
            let mut body = Vec::new();
            while let Some(chunk) = response.chunk().await.map_err(|_| anyhow!("ChatGPT authorization response was interrupted"))? {
                ensure!(body.len() + chunk.len() <= MAX_RESPONSE, "ChatGPT authorization response was too large");
                body.extend_from_slice(&chunk);
            }
            if !status.is_success() {
                let parsed: serde_json::Value = serde_json::from_slice(&body).unwrap_or_default();
                let returned = parsed.get("error").and_then(|value| value.as_str().or_else(|| value.get("code").and_then(serde_json::Value::as_str)));
                let code = match returned {
                    Some("invalid_grant") => "invalid_grant", Some("invalid_refresh_token") => "invalid_refresh_token",
                    Some("token_expired") => "token_expired", Some("refresh_token_expired") => "refresh_token_expired",
                    Some("refresh_token_invalidated") => "refresh_token_invalidated", Some("refresh_token_reused") => "refresh_token_reused",
                    Some("invalid_client") => "invalid_client", Some("access_denied") => "access_denied",
                    _ => "request_failed",
                };
                return Err(OAuthFailure { status: status.as_u16(), code, request_id }.into());
            }
            serde_json::from_slice(&body).map_err(|_| anyhow!("ChatGPT authorization returned an invalid response"))
        } => result,
    }
}

async fn signing_keys(client: &reqwest::Client, cancel: &CancellationToken) -> Result<JwkSet> {
    // Discovery is read only when connecting/renewing, never on application startup.
    let metadata: Discovery = request_json(client.get(DISCOVERY), cancel).await?;
    ensure!(
        metadata.issuer == ISSUER
            && metadata.authorization_endpoint == AUTHORIZE
            && metadata.token_endpoint == TOKEN
            && metadata.jwks_uri == JWKS,
        "ChatGPT authorization configuration changed; connection stopped"
    );
    request_json(client.get(JWKS), cancel).await
}

#[async_trait::async_trait]
trait AuthService: Send + Sync {
    async fn exchange(
        &self,
        form: &[(&str, &str)],
        cancel: &CancellationToken,
    ) -> Result<TokenResponse>;
    async fn keys(&self, cancel: &CancellationToken) -> Result<JwkSet>;
}

struct OpenAiAuth {
    client: reqwest::Client,
}
#[async_trait::async_trait]
impl AuthService for OpenAiAuth {
    async fn exchange(
        &self,
        form: &[(&str, &str)],
        cancel: &CancellationToken,
    ) -> Result<TokenResponse> {
        request_json(self.client.post(TOKEN).form(form), cancel).await
    }
    async fn keys(&self, cancel: &CancellationToken) -> Result<JwkSet> {
        signing_keys(&self.client, cancel).await
    }
}

async fn read_callback(stream: &mut TcpStream, pending: &Pending) -> Result<Option<Url>> {
    let mut bytes = Vec::new();
    loop {
        let mut chunk = [0; 1024];
        let size = stream
            .read(&mut chunk)
            .await
            .map_err(|_| anyhow!("ChatGPT callback could not be read"))?;
        ensure!(
            size > 0 && bytes.len() + size <= 16384,
            "Invalid ChatGPT callback request"
        );
        bytes.extend_from_slice(&chunk[..size]);
        if bytes.windows(4).any(|value| value == b"\r\n\r\n") {
            break;
        }
    }
    let request =
        std::str::from_utf8(&bytes).map_err(|_| anyhow!("Invalid ChatGPT callback request"))?;
    let mut lines = request.split("\r\n");
    let first: Vec<_> = lines
        .next()
        .unwrap_or_default()
        .split_ascii_whitespace()
        .collect();
    ensure!(
        first.len() == 3 && first[0] == "GET" && matches!(first[2], "HTTP/1.0" | "HTTP/1.1"),
        "Invalid ChatGPT callback method"
    );
    let host = format!(
        "127.0.0.1:{}",
        pending.redirect_uri.port().unwrap_or_default()
    );
    let hosts: Vec<_> = lines
        .take_while(|line| !line.is_empty())
        .filter_map(|line| line.split_once(':'))
        .filter(|(name, _)| name.eq_ignore_ascii_case("host"))
        .map(|(_, value)| value.trim())
        .collect();
    ensure!(
        hosts.len() == 1 && hosts[0] == host,
        "ChatGPT callback host did not match"
    );
    if first[1].split('?').next() != Some("/auth/callback") {
        return Ok(None);
    }
    let url = Url::parse(&format!("http://{host}{}", first[1]))
        .map_err(|_| anyhow!("Invalid ChatGPT callback address"))?;
    Ok(Some(url))
}

async fn browser_reply(stream: &mut TcpStream, accepted: bool) {
    let (status, text) = if accepted {
        (
            "200 OK",
            "Authorization received. Return to Melly to finish connecting.",
        )
    } else {
        (
            "400 Bad Request",
            "This sign-in could not be verified. Return to Melly and try again.",
        )
    };
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: text/plain; charset=utf-8\r\nCache-Control: no-store\r\nContent-Security-Policy: default-src 'none'; frame-ancestors 'none'\r\nReferrer-Policy: no-referrer\r\nX-Content-Type-Options: nosniff\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{text}",
        text.len()
    );
    let _ = tokio::time::timeout(
        Duration::from_secs(1),
        stream.write_all(response.as_bytes()),
    )
    .await;
}

async fn wait_callback(
    listener: TcpListener,
    pending: &Pending,
    cancel: &CancellationToken,
) -> Result<Callback> {
    tokio::select! {
        biased;
        _ = cancel.cancelled() => bail!("ChatGPT sign-in cancelled"),
        result = tokio::time::timeout(Duration::from_secs(SIGN_IN_SECONDS), async {
            loop {
                let (mut stream, peer) = listener.accept().await.map_err(|_| anyhow!("ChatGPT callback listener stopped"))?;
                if !peer.ip().is_loopback() { continue; }
                match tokio::time::timeout(Duration::from_secs(3), read_callback(&mut stream, pending)).await {
                    Ok(Ok(Some(url))) => {
                        let result = pending.callback(&url, now());
                        browser_reply(&mut stream, result.is_ok()).await;
                        return result;
                    },
                    _ => browser_reply(&mut stream, false).await,
                }
            }
        }) => result.map_err(|_| anyhow!("ChatGPT sign-in timed out; please try again"))?,
    }
}

fn still_current(vault: &dyn SecretStore, generation: &str) -> Result<()> {
    ensure!(
        get(vault, GENERATION_KEY)?.as_deref() == Some(generation),
        "ChatGPT sign-in was cancelled or replaced"
    );
    Ok(())
}

/// Opens a fresh app-owned OAuth flow in the system browser through `on_url`.
/// The callback must be used internally by Rust, never forwarded to webview JS.
pub async fn sign_in(
    vault: Arc<dyn SecretStore>,
    on_url: Arc<dyn Fn(String) + Send + Sync>,
    cancel: CancellationToken,
) -> Result<()> {
    check_cancel(&cancel)?;
    sign_in_with(
        vault,
        on_url,
        cancel,
        &OpenAiAuth {
            client: http_client()?,
        },
    )
    .await
}

async fn sign_in_with(
    vault: Arc<dyn SecretStore>,
    on_url: Arc<dyn Fn(String) + Send + Sync>,
    cancel: CancellationToken,
    service: &dyn AuthService,
) -> Result<()> {
    check_cancel(&cancel)?;
    let (host, generation, registration, prior) = {
        let _commit = COMMIT
            .lock()
            .map_err(|_| anyhow!("ChatGPT connection is busy"))?;
        let host = get(vault.as_ref(), HOST_KEY)?
            .unwrap_or_else(|| format!("urn:uuid:{}", uuid::Uuid::new_v4()));
        put(vault.as_ref(), HOST_KEY, &host)?;
        let generation = random();
        put(vault.as_ref(), GENERATION_KEY, &generation)?;
        let registration: Option<Registration> = get(vault.as_ref(), REGISTRATION_KEY)?
            .map(|value| decode_saved(&value))
            .transpose()?;
        let prior: Option<Session> = get(vault.as_ref(), SESSION_KEY)?
            .map(|value| decode_saved(&value))
            .transpose()?;
        (host, generation, registration, prior)
    };
    let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
        .await
        .map_err(|_| anyhow!("Melly could not open the local ChatGPT sign-in callback"))?;
    let port = listener
        .local_addr()
        .map_err(|_| anyhow!("ChatGPT callback could not be initialized"))?
        .port();
    let redirect = Url::parse(&format!("http://127.0.0.1:{port}/auth/callback"))
        .map_err(|_| anyhow!("Invalid ChatGPT callback configuration"))?;
    let pending = Pending::new(
        redirect,
        registration.as_ref().map(|value| value.client_id.clone()),
    )?;
    let force_consent = prior.as_ref().is_some_and(|value| !value.has_permission());
    check_cancel(&cancel)?;
    on_url(pending.authorization_url(&host, force_consent)?.to_string());
    let callback = wait_callback(listener, &pending, &cancel).await?;
    {
        let _commit = COMMIT
            .lock()
            .map_err(|_| anyhow!("ChatGPT connection is busy"))?;
        check_cancel(&cancel)?;
        still_current(vault.as_ref(), &generation)?;
        // The issued ID survives a failed code exchange, so retry does not register again.
        let saved = Registration {
            client_id: callback.client_id.clone(),
            identity: registration
                .as_ref()
                .and_then(|value| value.identity.clone()),
        };
        put(vault.as_ref(), REGISTRATION_KEY, &encode(&saved)?)?;
    }
    let tokens = service
        .exchange(
            &[
                ("grant_type", "authorization_code"),
                ("client_id", callback.client_id.as_str()),
                ("code", callback.code.as_str()),
                ("code_verifier", pending.verifier.as_str()),
                ("redirect_uri", pending.redirect_uri.as_str()),
                ("resource", RESOURCE),
            ],
            &cancel,
        )
        .await?;
    let keys = service.keys(&cancel).await?;
    let id_token = tokens
        .id_token
        .as_deref()
        .ok_or_else(|| anyhow!("ChatGPT did not return a verifiable identity"))?;
    let expected_subject = registration
        .as_ref()
        .and_then(|value| value.identity.as_ref())
        .map(|value| value.subject.as_str());
    let identity = verify_identity(
        id_token,
        &keys,
        &callback.client_id,
        Some(&pending.nonce),
        expected_subject,
    )?;
    let session = session_from_response(tokens, identity.clone(), None, now())?;
    {
        let _commit = COMMIT
            .lock()
            .map_err(|_| anyhow!("ChatGPT connection is busy"))?;
        check_cancel(&cancel)?;
        still_current(vault.as_ref(), &generation)?;
        put(
            vault.as_ref(),
            REGISTRATION_KEY,
            &encode(&Registration {
                client_id: callback.client_id,
                identity: Some(identity),
            })?,
        )?;
        put(vault.as_ref(), SESSION_KEY, &encode(&session)?)?;
    }
    ensure!(
        session.can_infer(now()),
        "Signed in with ChatGPT; plan usage is not enabled. Continue with ChatGPT to allow plan usage"
    );
    Ok(())
}

/// Returns only an identity-validated, explicitly consented plan token. A failed
/// renewal never changes endpoint, billing route, account, or provider.
pub async fn access_token(vault: Arc<dyn SecretStore>) -> Result<String> {
    access_token_owned(
        vault,
        Arc::new(OpenAiAuth {
            client: http_client()?,
        }),
    )
    .await
}

async fn access_token_owned(
    vault: Arc<dyn SecretStore>,
    service: Arc<dyn AuthService>,
) -> Result<String> {
    // A caller may stop a chat while the authorization server is rotating the
    // refresh token. Detaching this bounded operation lets its replacement be
    // saved even when that caller drops the returned future. Disconnect still
    // invalidates the generation and prevents the task from restoring a session.
    tokio::spawn(async move { access_token_with(vault, service.as_ref()).await })
        .await
        .map_err(|_| anyhow!("ChatGPT credential renewal was interrupted; please sign in again"))?
}

async fn access_token_with(
    vault: Arc<dyn SecretStore>,
    service: &dyn AuthService,
) -> Result<String> {
    let _refresh = REFRESH.lock().await;
    let (raw, session, generation) = {
        let _commit = COMMIT
            .lock()
            .map_err(|_| anyhow!("ChatGPT connection is busy"))?;
        let raw = get(vault.as_ref(), SESSION_KEY)?
            .ok_or_else(|| anyhow!("Please sign in with ChatGPT"))?;
        let session: Session = decode_saved(&raw)?;
        ensure!(
            session.has_permission(),
            "ChatGPT plan usage is not enabled; please sign in and allow plan usage"
        );
        validate_registration(vault.as_ref(), &session)?;
        if session.can_infer(now() + 60)
            || (session.can_infer(now()) && now() < session.earliest_refresh_at)
        {
            return session
                .access_token
                .ok_or_else(|| anyhow!("Please sign in with ChatGPT"));
        }
        let generation = get(vault.as_ref(), GENERATION_KEY)?.unwrap_or_default();
        (raw, session, generation)
    };
    ensure!(
        now() >= session.earliest_refresh_at,
        "ChatGPT token cannot be renewed yet; please try again later"
    );
    ensure!(
        session.can_renew(now()),
        "ChatGPT session expired; please sign in again"
    );
    let refresh_token = session
        .refresh_token
        .as_deref()
        .filter(|value| valid_secret(value))
        .ok_or_else(|| anyhow!("ChatGPT session expired; please sign in again"))?;
    let cancel = CancellationToken::new();
    let response = service
        .exchange(
            &[
                ("grant_type", "refresh_token"),
                ("client_id", session.identity.client_id.as_str()),
                ("refresh_token", refresh_token),
                ("resource", RESOURCE),
            ],
            &cancel,
        )
        .await;
    let response = match response {
        Ok(response) => response,
        Err(error) => {
            if error
                .downcast_ref::<OAuthFailure>()
                .is_some_and(OAuthFailure::terminal_refresh)
            {
                let _commit = COMMIT
                    .lock()
                    .map_err(|_| anyhow!("ChatGPT connection is busy"))?;
                if get(vault.as_ref(), SESSION_KEY)?.as_deref() == Some(raw.as_str()) {
                    remove(vault.as_ref(), SESSION_KEY)?;
                }
            }
            return Err(error);
        }
    };
    let identity = if let Some(id_token) = &response.id_token {
        let keys = service.keys(&cancel).await?;
        verify_identity(
            id_token,
            &keys,
            &session.identity.client_id,
            None,
            Some(&session.identity.subject),
        )?
    } else {
        session.identity.clone()
    };
    let updated = session_from_response(response, identity, Some(&session), now())?;
    {
        let _commit = COMMIT
            .lock()
            .map_err(|_| anyhow!("ChatGPT connection is busy"))?;
        ensure!(
            get(vault.as_ref(), GENERATION_KEY)?.unwrap_or_default() == generation
                && get(vault.as_ref(), SESSION_KEY)?.as_deref() == Some(raw.as_str()),
            "ChatGPT connection changed while renewing"
        );
        put(vault.as_ref(), SESSION_KEY, &encode(&updated)?)?;
    }
    ensure!(
        updated.can_infer(now()),
        "ChatGPT plan usage is no longer enabled; please sign in again"
    );
    updated
        .access_token
        .ok_or_else(|| anyhow!("Please sign in with ChatGPT"))
}

fn validate_registration(vault: &dyn SecretStore, session: &Session) -> Result<()> {
    let registration: Registration = decode_saved(
        &get(vault, REGISTRATION_KEY)?
            .ok_or_else(|| anyhow!("ChatGPT registration is missing; please sign in again"))?,
    )?;
    ensure!(
        registration.client_id == session.identity.client_id
            && registration
                .identity
                .is_some_and(|identity| identity.issuer == session.identity.issuer
                    && identity.subject == session.identity.subject
                    && identity.client_id == session.identity.client_id),
        "ChatGPT session does not match its verified registration"
    );
    Ok(())
}

/// Local verified grant status, including renewable sessions whose access token
/// has expired. This does not claim live model/API verification.
pub fn is_connected(vault: &dyn SecretStore) -> bool {
    get(vault, SESSION_KEY)
        .ok()
        .flatten()
        .and_then(|value| decode_saved::<Session>(&value).ok())
        .is_some_and(|session| {
            (session.can_infer(now()) || session.can_renew(now()))
                && validate_registration(vault, &session).is_ok()
        })
}

/// Clears local tokens and invalidates in-flight commits. Registration/host ID
/// are retained for reauthorization. Remote app access is managed in ChatGPT.
pub fn disconnect(vault: &dyn SecretStore) -> Result<()> {
    let _commit = COMMIT
        .lock()
        .map_err(|_| anyhow!("ChatGPT connection is busy"))?;
    put(vault, GENERATION_KEY, &random())?;
    remove(vault, SESSION_KEY)
}
