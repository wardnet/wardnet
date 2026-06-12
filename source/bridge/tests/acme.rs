//! Pebble-backed integration tests for the ACME issuance path.
//!
//! Requires a running pebble mock ACME server:
//!
//! ```sh
//! docker compose up -d   # from source/bridge/
//! ```
//!
//! Also requires:
//! - `BRIDGE_TEST_PEBBLE_CA` — path to the pebble WFE CA PEM
//!   (`docker exec <pebble-container> cat /test/certs/pebble.minica.pem`)
//! - `BRIDGE_TEST_PEBBLE_URL` — ACME directory URL (default `https://localhost:14000/dir`)

mod common;

use std::collections::HashMap;
use std::sync::Mutex;

use async_trait::async_trait;
use chrono::Utc;

use wardnet_bridge::acme::{Http01Solver, issue};

use common::pebble_directory_url;

// ── In-memory HTTP-01 solver ─────────────────────────────────────────────────
//
// With `PEBBLE_VA_ALWAYS_VALID=1` pebble never fetches the token, so this
// solver only needs to not error — the in-memory store is there for correctness
// but is never actually queried by the ACME server.

struct InMemorySolver {
    tokens: Mutex<HashMap<String, String>>,
}

impl InMemorySolver {
    fn new() -> Self {
        Self {
            tokens: Mutex::new(HashMap::new()),
        }
    }
}

#[async_trait]
impl Http01Solver for InMemorySolver {
    async fn present(&self, token: &str, key_authorization: &str) -> anyhow::Result<()> {
        self.tokens
            .lock()
            .unwrap()
            .insert(token.to_owned(), key_authorization.to_owned());
        Ok(())
    }

    async fn cleanup(&self, token: &str) -> anyhow::Result<()> {
        self.tokens.lock().unwrap().remove(token);
        Ok(())
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[tokio::test]
#[ignore = "requires pebble (docker compose up -d)"]
async fn issue_creates_cert_from_scratch() {
    let solver = InMemorySolver::new();
    let result = issue(
        &pebble_directory_url(),
        "bridge.test.wardnet.network",
        None,
        &solver,
    )
    .await
    .expect("issue must succeed against pebble");

    assert!(
        !result.chain_pem.is_empty(),
        "issued chain PEM must be non-empty"
    );
    assert!(
        !result.key_pem.is_empty(),
        "issued key PEM must be non-empty"
    );
    assert!(
        !result.account_credentials.is_empty(),
        "account credentials must be returned"
    );
    assert!(
        result.not_after > Utc::now(),
        "issued cert must expire in the future"
    );
}

#[tokio::test]
#[ignore = "requires pebble (docker compose up -d)"]
async fn issue_reuses_account_credentials() {
    let dir = pebble_directory_url();

    let first = issue(
        &dir,
        "bridge.test.wardnet.network",
        None,
        &InMemorySolver::new(),
    )
    .await
    .expect("first issuance must succeed");

    let second = issue(
        &dir,
        "bridge.test.wardnet.network",
        Some(&first.account_credentials),
        &InMemorySolver::new(),
    )
    .await
    .expect("second issuance reusing credentials must succeed");

    assert!(
        !second.chain_pem.is_empty(),
        "renewed chain PEM must be non-empty"
    );
    assert!(
        second.not_after > Utc::now(),
        "renewed cert must expire in the future"
    );
}
