//! Bridge provider tests against a `wiremock` bridge. These cryptographically
//! verify the Ed25519 signature and the `PoW` solution — the daemon's signing /
//! `PoW` code is a fresh reimplementation in a different workspace from the
//! bridge's verifier, so a format mismatch must fail *here*, not at deploy time.

use base64::Engine as _;
use ed25519_dalek::{Signature, SigningKey, Verifier};
use sha2::{Digest, Sha256};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::ddns::bridge::{BridgeProvider, check_name_available, register_install, solve_pow};
use crate::ddns::provider::DnsProvider;

/// Count leading zero bits, byte by byte (mirrors the bridge verifier).
fn leading_zero_bits(hash: &[u8]) -> u32 {
    let mut bits = 0u32;
    for byte in hash {
        let z = byte.leading_zeros();
        bits += z;
        if z < 8 {
            break;
        }
    }
    bits
}

#[test]
fn solve_pow_rejects_excessive_difficulty() {
    // A hostile/misconfigured bridge demanding an unsolvable difficulty must be
    // rejected rather than spinning a core forever.
    assert!(solve_pow("nonce", "name", "pubkey", 64).is_err());
}

#[test]
fn solve_pow_finds_valid_proof_for_small_difficulty() {
    let proof = solve_pow("nonce", "name", "pubkey", 6).unwrap();
    let payload = format!("nonce\nname\npubkey\n{proof}");
    assert!(leading_zero_bits(&Sha256::digest(payload.as_bytes())) >= 6);
}

#[tokio::test]
async fn upsert_a_signs_request_verifiably() {
    let seed = [7u8; 32];
    let verifying_key = SigningKey::from_bytes(&seed).verifying_key();

    let server = MockServer::start().await;
    let install_id = "install-123";
    Mock::given(method("PUT"))
        .and(path(format!("/v1/installs/{install_id}/ip")))
        .respond_with(ResponseTemplate::new(204))
        .mount(&server)
        .await;

    let provider = BridgeProvider::new(
        reqwest::Client::new(),
        server.uri(),
        install_id.to_owned(),
        "bearer-xyz".to_owned(),
        seed,
    );
    provider.upsert_a("1.2.3.4".parse().unwrap()).await.unwrap();

    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 1);
    let request = &requests[0];

    assert_eq!(
        request.headers.get("authorization").unwrap(),
        "Bearer bearer-xyz"
    );
    let timestamp = request
        .headers
        .get("x-wardnet-timestamp")
        .unwrap()
        .to_str()
        .unwrap();
    let signature_b64 = request
        .headers
        .get("x-wardnet-signature")
        .unwrap()
        .to_str()
        .unwrap();

    // Reproduce the canonical payload and verify the signature.
    let body_hash = hex::encode(Sha256::digest(&request.body));
    let payload = format!("PUT\n/v1/installs/{install_id}/ip\n{timestamp}\n{body_hash}");
    let signature_bytes: [u8; 64] = base64::engine::general_purpose::STANDARD
        .decode(signature_b64)
        .unwrap()
        .try_into()
        .unwrap();
    verifying_key
        .verify(payload.as_bytes(), &Signature::from_bytes(&signature_bytes))
        .expect("daemon signature must verify against its public key");

    let body: serde_json::Value = serde_json::from_slice(&request.body).unwrap();
    assert_eq!(body["ip"], "1.2.3.4");
}

#[tokio::test]
async fn delete_txt_signs_empty_body() {
    let seed = [9u8; 32];
    let verifying_key = SigningKey::from_bytes(&seed).verifying_key();

    let server = MockServer::start().await;
    let install_id = "install-77";
    Mock::given(method("DELETE"))
        .and(path(format!("/v1/installs/{install_id}/acme-challenge")))
        .respond_with(ResponseTemplate::new(204))
        .mount(&server)
        .await;

    let provider = BridgeProvider::new(
        reqwest::Client::new(),
        server.uri(),
        install_id.to_owned(),
        "tok".to_owned(),
        seed,
    );
    provider.delete_txt().await.unwrap();

    let requests = server.received_requests().await.unwrap();
    let request = &requests[0];
    assert!(request.body.is_empty());
    let timestamp = request
        .headers
        .get("x-wardnet-timestamp")
        .unwrap()
        .to_str()
        .unwrap();
    let signature_b64 = request
        .headers
        .get("x-wardnet-signature")
        .unwrap()
        .to_str()
        .unwrap();
    // Empty body still hashes — SHA-256 of zero bytes.
    let body_hash = hex::encode(Sha256::digest(b""));
    let payload =
        format!("DELETE\n/v1/installs/{install_id}/acme-challenge\n{timestamp}\n{body_hash}");
    let signature_bytes: [u8; 64] = base64::engine::general_purpose::STANDARD
        .decode(signature_b64)
        .unwrap()
        .try_into()
        .unwrap();
    verifying_key
        .verify(payload.as_bytes(), &Signature::from_bytes(&signature_bytes))
        .unwrap();
}

#[tokio::test]
async fn register_install_solves_pow_and_returns_identity() {
    let server = MockServer::start().await;
    let difficulty = 8u32;
    let nonce = "abc123nonce";

    Mock::given(method("GET"))
        .and(path("/v1/register/challenge"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "challenge_id": "chal-1",
            "nonce": nonce,
            "difficulty": difficulty,
            "expires_at": "2030-01-01T00:00:00Z",
        })))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/register"))
        .respond_with(ResponseTemplate::new(201).set_body_json(serde_json::json!({
            "id": "install-9",
            "bearer_token": "tok-9",
            "subdomain": "happy-einstein.my.us.wardnet.network",
            "region": "us",
        })))
        .mount(&server)
        .await;

    let identity = register_install(&reqwest::Client::new(), &server.uri(), "happy-einstein")
        .await
        .unwrap();
    assert_eq!(identity.install_id, "install-9");
    assert_eq!(identity.bearer_token, "tok-9");
    assert_eq!(identity.subdomain, "happy-einstein.my.us.wardnet.network");
    assert_eq!(identity.region, "us");

    // The register request must carry a PoW solution that satisfies difficulty.
    let requests = server.received_requests().await.unwrap();
    let register = requests
        .iter()
        .find(|r| r.url.path() == "/v1/register")
        .unwrap();
    let body: serde_json::Value = serde_json::from_slice(&register.body).unwrap();
    let public_key = body["public_key"].as_str().unwrap();
    let proof = body["proof"].as_u64().unwrap();
    let payload = format!("{nonce}\nhappy-einstein\n{public_key}\n{proof}");
    assert!(leading_zero_bits(&Sha256::digest(payload.as_bytes())) >= difficulty);
}

#[tokio::test]
async fn check_name_available_reports_true_and_false() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/names/free-name/available"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(serde_json::json!({"available": true})),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/v1/names/taken-name/available"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(serde_json::json!({"available": false})),
        )
        .mount(&server)
        .await;

    let client = reqwest::Client::new();
    assert!(
        check_name_available(&client, &server.uri(), "free-name")
            .await
            .unwrap()
    );
    assert!(
        !check_name_available(&client, &server.uri(), "taken-name")
            .await
            .unwrap()
    );
}
