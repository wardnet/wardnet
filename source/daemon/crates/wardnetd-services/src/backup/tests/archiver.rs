//! Round-trip and failure-mode tests for [`AgeArchiver`].

use wardnet_common::backup::BundleManifest;
use wardnetd_data::secret_store::SecretEntry;

use crate::backup::archiver::{AgeArchiver, BackupArchiver, BundleContents};

fn sample_contents() -> BundleContents {
    BundleContents {
        manifest: BundleManifest::new("0.2.0-test", 7, "test-host", 2),
        database_bytes: b"SQLite format 3\x00fake-db-bytes".to_vec(),
        config_bytes: b"[database]\npath = \"wardnet.db\"\n".to_vec(),
        secrets: vec![
            SecretEntry {
                path: "wireguard/aaa.key".to_owned(),
                value: b"priv-key-aaa".to_vec(),
            },
            SecretEntry {
                path: "wireguard/bbb.key".to_owned(),
                value: b"priv-key-bbb".to_vec(),
            },
        ],
    }
}

#[tokio::test]
async fn pack_unpack_round_trip() {
    let archiver = AgeArchiver::new();
    let contents = sample_contents();

    let encrypted = archiver
        .pack("correct-horse-battery-staple", contents.clone())
        .await
        .unwrap();

    let decoded = archiver
        .unpack("correct-horse-battery-staple", &encrypted)
        .await
        .unwrap();

    assert_eq!(decoded.manifest, contents.manifest);
    assert_eq!(decoded.database_bytes, contents.database_bytes);
    assert_eq!(decoded.config_bytes, contents.config_bytes);

    let mut got: Vec<_> = decoded.secrets.into_iter().collect();
    got.sort_by(|a, b| a.path.cmp(&b.path));
    let mut want: Vec<_> = contents.secrets.into_iter().collect();
    want.sort_by(|a, b| a.path.cmp(&b.path));
    assert_eq!(got.len(), want.len());
    for (g, w) in got.iter().zip(want.iter()) {
        assert_eq!(g.path, w.path);
        assert_eq!(g.value, w.value);
    }
}

#[tokio::test]
async fn unpack_fails_with_wrong_passphrase() {
    let archiver = AgeArchiver::new();
    let encrypted = archiver
        .pack("real-passphrase-1234", sample_contents())
        .await
        .unwrap();

    let err = archiver
        .unpack("wrong-passphrase-5678", &encrypted)
        .await
        .unwrap_err();
    assert!(
        format!("{err:#}").to_lowercase().contains("decryption"),
        "expected decryption error, got: {err}"
    );
}

#[tokio::test]
async fn unpack_rejects_garbage_bytes() {
    let archiver = AgeArchiver::new();
    let err = archiver
        .unpack("whatever", b"not-an-age-stream")
        .await
        .unwrap_err();
    assert!(
        format!("{err:#}").to_lowercase().contains("age"),
        "expected age-layer error, got: {err}"
    );
}

#[tokio::test]
async fn pack_is_non_empty_and_not_plaintext() {
    let archiver = AgeArchiver::new();
    let contents = sample_contents();
    let plaintext_marker = &contents.secrets[0].value.clone();

    let encrypted = archiver
        .pack("a-reasonable-passphrase", contents)
        .await
        .unwrap();

    assert!(!encrypted.is_empty());
    // The plaintext marker must not appear in the ciphertext.
    assert!(
        !encrypted
            .windows(plaintext_marker.len())
            .any(|w| w == plaintext_marker.as_slice()),
        "encrypted output should not contain plaintext secret bytes"
    );
}
