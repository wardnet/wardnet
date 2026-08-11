//! In-memory repository mocks shared by the auth-service tests.
//!
//! One module rather than a private set per test file: `AuthServiceImpl` now
//! takes five repositories, and three test files were each maintaining their
//! own copies. Duplicated mocks drift, and a mock that has drifted from the
//! trait it implements silently stops testing what the file claims to.
//!
//! These deliberately reproduce the **SQL-side invariants** the real
//! repositories enforce, because the service relies on them:
//!
//! - [`MockCredentialRepo::find_for_login`] filters disabled users, and returns
//!   `None` indistinguishably for "no credential" and "user disabled".
//! - [`MockSessionRepo::find_principal_by_token_hash`] filters expired sessions,
//!   the absolute ceiling, **and** disabled users — all four conditions, since
//!   a mock that skipped one would let a service bug through.
//!
//! A mock that is more permissive than production turns a passing test into a
//! false negative, which is worse than no test at all.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use wardnet_common::auth::UserRole;
use wardnetd_data::repository::session::{
    SessionForRefresh, SessionPrincipal, SessionRepository, SessionSummary,
};
use wardnetd_data::repository::user::{UserRepository, UserRow};
use wardnetd_data::repository::user_credential::{
    CredentialAlreadyLinkedError, CredentialKind, CredentialLogin, CredentialRow,
    CredentialSummary, UserCredentialRepository,
};
use wardnetd_data::repository::user_enrolment::{EnrolmentTokenRow, UserEnrolmentRepository};
use wardnetd_data::repository::{ApiKeyRepository, SystemConfigRepository};

/// RFC 3339 timestamp used for rows whose creation time is irrelevant.
pub const T0: &str = "2026-01-01T00:00:00+00:00";

/// Build a household-user row.
#[must_use]
pub fn user_row(id: &str, display_name: &str, role: UserRole, enabled: bool) -> UserRow {
    UserRow {
        id: id.to_owned(),
        display_name: display_name.to_owned(),
        email: None,
        role,
        enabled,
        created_at: T0.to_owned(),
        updated_at: T0.to_owned(),
    }
}

// ---------------------------------------------------------------------------
// Users
// ---------------------------------------------------------------------------

/// In-memory [`UserRepository`].
pub struct MockUserRepo {
    pub rows: Mutex<Vec<UserRow>>,
}

impl MockUserRepo {
    #[must_use]
    pub fn empty() -> Self {
        Self {
            rows: Mutex::new(Vec::new()),
        }
    }

    /// A repo already holding one enabled `admin`, as every upgraded box does.
    #[must_use]
    pub fn with_admin(id: &str, display_name: &str) -> Self {
        Self {
            rows: Mutex::new(vec![user_row(id, display_name, UserRole::Admin, true)]),
        }
    }

    #[must_use]
    pub fn with_rows(rows: Vec<UserRow>) -> Self {
        Self {
            rows: Mutex::new(rows),
        }
    }

    /// Number of rows currently stored — lets a test assert a rollback ran.
    #[must_use]
    pub fn count(&self) -> usize {
        self.rows.lock().unwrap().len()
    }
}

#[async_trait]
impl UserRepository for MockUserRepo {
    async fn create(&self, row: &UserRow) -> anyhow::Result<()> {
        self.rows.lock().unwrap().push(row.clone());
        Ok(())
    }

    async fn find_by_id(&self, id: &str) -> anyhow::Result<Option<UserRow>> {
        Ok(self
            .rows
            .lock()
            .unwrap()
            .iter()
            .find(|r| r.id == id)
            .cloned())
    }

    async fn find_by_email(&self, email: &str) -> anyhow::Result<Option<UserRow>> {
        let needle = email.to_lowercase();
        Ok(self
            .rows
            .lock()
            .unwrap()
            .iter()
            .find(|r| r.email.as_ref().map(|e| e.to_lowercase()) == Some(needle.clone()))
            .cloned())
    }

    async fn find_all(&self) -> anyhow::Result<Vec<UserRow>> {
        Ok(self.rows.lock().unwrap().clone())
    }

    async fn update_profile(
        &self,
        id: &str,
        display_name: &str,
        email: Option<&str>,
        now: &str,
    ) -> anyhow::Result<u64> {
        let mut rows = self.rows.lock().unwrap();
        match rows.iter_mut().find(|r| r.id == id) {
            Some(row) => {
                row.display_name = display_name.to_owned();
                row.email = email.map(str::to_owned);
                row.updated_at = now.to_owned();
                Ok(1)
            }
            None => Ok(0),
        }
    }

    async fn set_enabled(&self, id: &str, enabled: bool, now: &str) -> anyhow::Result<u64> {
        let mut rows = self.rows.lock().unwrap();
        match rows.iter_mut().find(|r| r.id == id) {
            Some(row) => {
                row.enabled = enabled;
                row.updated_at = now.to_owned();
                Ok(1)
            }
            None => Ok(0),
        }
    }

    async fn set_role(&self, id: &str, role: UserRole, now: &str) -> anyhow::Result<u64> {
        let mut rows = self.rows.lock().unwrap();
        match rows.iter_mut().find(|r| r.id == id) {
            Some(row) => {
                row.role = role;
                row.updated_at = now.to_owned();
                Ok(1)
            }
            None => Ok(0),
        }
    }

    async fn delete(&self, id: &str) -> anyhow::Result<u64> {
        let mut rows = self.rows.lock().unwrap();
        let before = rows.len();
        rows.retain(|r| r.id != id);
        Ok((before - rows.len()) as u64)
    }

    async fn count_enabled_admins(&self) -> anyhow::Result<i64> {
        Ok(self
            .rows
            .lock()
            .unwrap()
            .iter()
            .filter(|r| r.enabled && r.role == UserRole::Admin)
            .count()
            .try_into()
            .unwrap_or(i64::MAX))
    }

    async fn exists(&self) -> anyhow::Result<bool> {
        Ok(!self.rows.lock().unwrap().is_empty())
    }

    async fn find_first_enabled_admin(&self) -> anyhow::Result<Option<UserRow>> {
        let rows = self.rows.lock().unwrap();
        let mut admins: Vec<&UserRow> = rows
            .iter()
            .filter(|r| r.enabled && r.role == UserRole::Admin)
            .collect();
        // Mirror the SQL `ORDER BY created_at, id` so the answer is stable.
        admins.sort_by(|a, b| (&a.created_at, &a.id).cmp(&(&b.created_at, &b.id)));
        Ok(admins.first().map(|r| (*r).clone()))
    }
}

// ---------------------------------------------------------------------------
// Credentials
// ---------------------------------------------------------------------------

/// In-memory [`UserCredentialRepository`].
///
/// Holds an `Arc` to the user repo so `find_for_login` can apply the
/// `enabled` join the real SQL does.
pub struct MockCredentialRepo {
    pub rows: Mutex<Vec<CredentialRow>>,
    users: Option<Arc<MockUserRepo>>,
    /// When set, `set_password` fails with this message — used to test the
    /// bootstrap rollback path.
    pub fail_set_password: Option<String>,
    /// When true, `insert` enforces the `(kind, subject)` unique index by
    /// returning [`CredentialAlreadyLinkedError`]. Off by default so tests that
    /// merely seed rows are not obliged to care.
    enforce_unique_subject: Mutex<bool>,
}

impl MockCredentialRepo {
    #[must_use]
    pub fn empty() -> Self {
        Self {
            rows: Mutex::new(Vec::new()),
            users: None,
            fail_set_password: None,
            enforce_unique_subject: Mutex::new(false),
        }
    }

    /// Turn on the `(kind, subject)` uniqueness constraint — the anti-hijack
    /// invariant that stops one provider account linking to two users.
    pub fn enforce_subject_uniqueness(&self) {
        *self.enforce_unique_subject.lock().unwrap() = true;
    }

    /// Join credentials to a user repo so the `enabled` filter is honoured.
    #[must_use]
    pub fn joined_to(users: Arc<MockUserRepo>) -> Self {
        Self {
            rows: Mutex::new(Vec::new()),
            users: Some(users),
            fail_set_password: None,
            enforce_unique_subject: Mutex::new(false),
        }
    }

    /// Make `set_password` fail, to drive the compensation path.
    #[must_use]
    pub fn failing_set_password(mut self, message: &str) -> Self {
        self.fail_set_password = Some(message.to_owned());
        self
    }

    /// Seed a password credential.
    #[must_use]
    pub fn with_password(self, id: &str, user_id: &str, subject: &str, secret: &str) -> Self {
        self.rows.lock().unwrap().push(CredentialRow {
            id: id.to_owned(),
            user_id: user_id.to_owned(),
            kind: CredentialKind::Password,
            subject: subject.to_owned(),
            secret: Some(secret.to_owned()),
            label: None,
            metadata: "{}".to_owned(),
            created_at: T0.to_owned(),
            last_used_at: None,
        });
        self
    }

    /// RFC 3339 `last_used_at` of one credential, for asserting the touch ran.
    #[must_use]
    pub fn last_used(&self, id: &str) -> Option<String> {
        self.rows
            .lock()
            .unwrap()
            .iter()
            .find(|r| r.id == id)
            .and_then(|r| r.last_used_at.clone())
    }
}

#[async_trait]
impl UserCredentialRepository for MockCredentialRepo {
    async fn insert(&self, row: &CredentialRow) -> anyhow::Result<()> {
        let mut rows = self.rows.lock().unwrap();
        if *self.enforce_unique_subject.lock().unwrap()
            && rows
                .iter()
                .any(|r| r.kind == row.kind && r.subject == row.subject)
        {
            // Downcastable, exactly as the real SQLite constraint violation is,
            // so the service can map it to a 409 rather than a 500.
            return Err(anyhow::Error::new(CredentialAlreadyLinkedError));
        }
        rows.push(row.clone());
        Ok(())
    }

    async fn find_for_login(
        &self,
        kind: CredentialKind,
        subject: &str,
    ) -> anyhow::Result<Option<CredentialLogin>> {
        let credential = self
            .rows
            .lock()
            .unwrap()
            .iter()
            .find(|r| r.kind == kind && r.subject == subject)
            .cloned();
        let Some(credential) = credential else {
            return Ok(None);
        };

        // The real query joins `users` and filters `enabled = 1`. Without a
        // user repo we cannot, so default to an enabled admin.
        let (role, display_name, enabled) = match &self.users {
            Some(users) => match users.find_by_id(&credential.user_id).await? {
                Some(u) => (u.role, u.display_name, u.enabled),
                // Credential pointing at a missing user: the FK makes this
                // impossible in production, and it must not authenticate.
                None => return Ok(None),
            },
            None => (UserRole::Admin, "admin".to_owned(), true),
        };

        if !enabled {
            // Deliberately identical to "no such credential" so a disabled
            // account cannot be probed.
            return Ok(None);
        }

        Ok(Some(CredentialLogin {
            credential,
            role,
            display_name,
        }))
    }

    async fn find_password(&self, user_id: &str) -> anyhow::Result<Option<CredentialRow>> {
        Ok(self
            .rows
            .lock()
            .unwrap()
            .iter()
            .find(|r| r.user_id == user_id && r.kind == CredentialKind::Password)
            .cloned())
    }

    async fn list_for_user(&self, user_id: &str) -> anyhow::Result<Vec<CredentialSummary>> {
        Ok(self
            .rows
            .lock()
            .unwrap()
            .iter()
            .filter(|r| r.user_id == user_id)
            .map(|r| CredentialSummary {
                id: r.id.clone(),
                user_id: r.user_id.clone(),
                kind: r.kind,
                subject: r.subject.clone(),
                label: r.label.clone(),
                metadata: r.metadata.clone(),
                created_at: r.created_at.clone(),
                last_used_at: r.last_used_at.clone(),
            })
            .collect())
    }

    async fn set_password(
        &self,
        id: &str,
        user_id: &str,
        subject: &str,
        secret: &str,
        now: &str,
    ) -> anyhow::Result<()> {
        if let Some(message) = &self.fail_set_password {
            anyhow::bail!("{message}");
        }
        let mut rows = self.rows.lock().unwrap();
        // Upsert, as the real one-statement version does.
        rows.retain(|r| !(r.user_id == user_id && r.kind == CredentialKind::Password));
        rows.push(CredentialRow {
            id: id.to_owned(),
            user_id: user_id.to_owned(),
            kind: CredentialKind::Password,
            subject: subject.to_owned(),
            secret: Some(secret.to_owned()),
            label: None,
            metadata: "{}".to_owned(),
            created_at: now.to_owned(),
            last_used_at: None,
        });
        Ok(())
    }

    async fn touch_last_used(&self, id: &str, now: &str) -> anyhow::Result<()> {
        let mut rows = self.rows.lock().unwrap();
        if let Some(row) = rows.iter_mut().find(|r| r.id == id) {
            row.last_used_at = Some(now.to_owned());
        }
        Ok(())
    }

    async fn update_metadata(&self, id: &str, metadata: &str) -> anyhow::Result<u64> {
        let mut rows = self.rows.lock().unwrap();
        match rows.iter_mut().find(|r| r.id == id) {
            Some(row) => {
                row.metadata = metadata.to_owned();
                Ok(1)
            }
            None => Ok(0),
        }
    }

    async fn delete(&self, id: &str, user_id: &str) -> anyhow::Result<u64> {
        let mut rows = self.rows.lock().unwrap();
        let before = rows.len();
        rows.retain(|r| !(r.id == id && r.user_id == user_id));
        Ok((before - rows.len()) as u64)
    }

    async fn delete_by_kind(&self, user_id: &str, kind: CredentialKind) -> anyhow::Result<u64> {
        let mut rows = self.rows.lock().unwrap();
        let before = rows.len();
        rows.retain(|r| !(r.user_id == user_id && r.kind == kind));
        Ok((before - rows.len()) as u64)
    }

    async fn count_by_kind(&self, user_id: &str, kind: CredentialKind) -> anyhow::Result<i64> {
        Ok(self
            .rows
            .lock()
            .unwrap()
            .iter()
            .filter(|r| r.user_id == user_id && r.kind == kind)
            .count()
            .try_into()
            .unwrap_or(i64::MAX))
    }
}

// ---------------------------------------------------------------------------
// Sessions
// ---------------------------------------------------------------------------

/// One stored session row.
#[derive(Debug, Clone)]
pub struct StoredSession {
    pub id: String,
    pub user_id: String,
    pub token_hash: String,
    pub created_at: String,
    pub expires_at: String,
    pub remember_me: bool,
    pub device_id: Option<String>,
    pub user_agent: Option<String>,
    pub absolute_expires_at: String,
}

/// In-memory [`SessionRepository`].
pub struct MockSessionRepo {
    pub rows: Mutex<Vec<StoredSession>>,
    users: Option<Arc<MockUserRepo>>,
    pub deleted_expired: Mutex<u64>,
}

impl MockSessionRepo {
    #[must_use]
    pub fn empty() -> Self {
        Self {
            rows: Mutex::new(Vec::new()),
            users: None,
            deleted_expired: Mutex::new(0),
        }
    }

    /// Join to a user repo so the `enabled` filter on reads is honoured.
    #[must_use]
    pub fn joined_to(users: Arc<MockUserRepo>) -> Self {
        Self {
            rows: Mutex::new(Vec::new()),
            users: Some(users),
            deleted_expired: Mutex::new(0),
        }
    }

    /// Seed a session row directly.
    #[must_use]
    pub fn with_session(self, session: StoredSession) -> Self {
        self.rows.lock().unwrap().push(session);
        self
    }

    #[must_use]
    pub fn count(&self) -> usize {
        self.rows.lock().unwrap().len()
    }

    /// The single stored row, for asserting what `login` wrote.
    #[must_use]
    pub fn only(&self) -> StoredSession {
        let rows = self.rows.lock().unwrap();
        assert_eq!(rows.len(), 1, "expected exactly one session row");
        rows[0].clone()
    }

    /// Is this user still allowed to authenticate?
    async fn user_enabled(&self, user_id: &str) -> anyhow::Result<bool> {
        match &self.users {
            Some(users) => Ok(users.find_by_id(user_id).await?.is_some_and(|u| u.enabled)),
            None => Ok(true),
        }
    }

    /// The role to report, read live from `users` as the real join does.
    async fn user_role(&self, user_id: &str) -> anyhow::Result<(UserRole, String)> {
        match &self.users {
            Some(users) => Ok(users
                .find_by_id(user_id)
                .await?
                .map_or((UserRole::Admin, "admin".to_owned()), |u| {
                    (u.role, u.display_name)
                })),
            None => Ok((UserRole::Admin, "admin".to_owned())),
        }
    }
}

#[async_trait]
impl SessionRepository for MockSessionRepo {
    async fn create(
        &self,
        id: &str,
        user_id: &str,
        token_hash: &str,
        created_at: &str,
        expires_at: &str,
        remember_me: bool,
        device_id: Option<&str>,
        user_agent: Option<&str>,
        absolute_expires_at: &str,
    ) -> anyhow::Result<()> {
        self.rows.lock().unwrap().push(StoredSession {
            id: id.to_owned(),
            user_id: user_id.to_owned(),
            token_hash: token_hash.to_owned(),
            created_at: created_at.to_owned(),
            expires_at: expires_at.to_owned(),
            remember_me,
            device_id: device_id.map(str::to_owned),
            user_agent: user_agent.map(str::to_owned),
            absolute_expires_at: absolute_expires_at.to_owned(),
        });
        Ok(())
    }

    async fn find_principal_by_token_hash(
        &self,
        token_hash: &str,
        now: &str,
    ) -> anyhow::Result<Option<SessionPrincipal>> {
        let row = self
            .rows
            .lock()
            .unwrap()
            .iter()
            .find(|r| r.token_hash == token_hash)
            .cloned();
        let Some(row) = row else { return Ok(None) };

        // All four SQL-side conditions, in the same order as the real query.
        if row.expires_at.as_str() <= now || row.absolute_expires_at.as_str() <= now {
            return Ok(None);
        }
        if !self.user_enabled(&row.user_id).await? {
            return Ok(None);
        }

        let (role, display_name) = self.user_role(&row.user_id).await?;
        Ok(Some(SessionPrincipal {
            user_id: row.user_id,
            role,
            display_name,
        }))
    }

    async fn delete_expired(&self, now: &str) -> anyhow::Result<u64> {
        let mut rows = self.rows.lock().unwrap();
        let before = rows.len();
        rows.retain(|r| r.expires_at.as_str() > now);
        let removed = (before - rows.len()) as u64;
        *self.deleted_expired.lock().unwrap() += removed;
        Ok(removed)
    }

    async fn delete_by_token_hash(&self, token_hash: &str) -> anyhow::Result<u64> {
        let mut rows = self.rows.lock().unwrap();
        let before = rows.len();
        rows.retain(|r| r.token_hash != token_hash);
        Ok((before - rows.len()) as u64)
    }

    async fn delete_by_id(&self, id: &str, user_id: &str) -> anyhow::Result<u64> {
        let mut rows = self.rows.lock().unwrap();
        let before = rows.len();
        rows.retain(|r| !(r.id == id && r.user_id == user_id));
        Ok((before - rows.len()) as u64)
    }

    async fn delete_all_for_user(&self, user_id: &str) -> anyhow::Result<u64> {
        let mut rows = self.rows.lock().unwrap();
        let before = rows.len();
        rows.retain(|r| r.user_id != user_id);
        Ok((before - rows.len()) as u64)
    }

    async fn list_for_user(&self, user_id: &str, now: &str) -> anyhow::Result<Vec<SessionSummary>> {
        Ok(self
            .rows
            .lock()
            .unwrap()
            .iter()
            .filter(|r| r.user_id == user_id && r.expires_at.as_str() > now)
            .map(|r| SessionSummary {
                id: r.id.clone(),
                user_id: r.user_id.clone(),
                device_id: r.device_id.clone(),
                user_agent: r.user_agent.clone(),
                created_at: r.created_at.clone(),
                expires_at: r.expires_at.clone(),
            })
            .collect())
    }

    async fn rotate_token(
        &self,
        old_token_hash: &str,
        new_token_hash: &str,
        new_expires_at: &str,
    ) -> anyhow::Result<()> {
        let mut rows = self.rows.lock().unwrap();
        if let Some(row) = rows.iter_mut().find(|r| r.token_hash == old_token_hash) {
            row.token_hash = new_token_hash.to_owned();
            row.expires_at = new_expires_at.to_owned();
        }
        Ok(())
    }

    async fn find_session_for_refresh(
        &self,
        token_hash: &str,
        now: &str,
    ) -> anyhow::Result<Option<SessionForRefresh>> {
        let row = self
            .rows
            .lock()
            .unwrap()
            .iter()
            .find(|r| r.token_hash == token_hash)
            .cloned();
        let Some(row) = row else { return Ok(None) };

        // The same four conditions as `find_principal_by_token_hash`: a refresh
        // that skipped the ceiling or the `enabled` join would be a revocation
        // that does not revoke.
        if row.expires_at.as_str() <= now || row.absolute_expires_at.as_str() <= now {
            return Ok(None);
        }
        if !self.user_enabled(&row.user_id).await? {
            return Ok(None);
        }

        Ok(Some(SessionForRefresh {
            user_id: row.user_id,
            remember_me: row.remember_me,
            created_at: row.created_at,
            absolute_expires_at: row.absolute_expires_at,
        }))
    }
}

// ---------------------------------------------------------------------------
// Enrolment tokens
// ---------------------------------------------------------------------------

/// In-memory [`UserEnrolmentRepository`].
///
/// `find_redeemable` and `mark_used` reproduce the SQL-side single-use and
/// expiry predicates. `mark_used` in particular re-checks `used_at IS NULL`
/// under the same lock it writes through, mirroring the real `UPDATE ... WHERE
/// used_at IS NULL` — the whole point of that predicate is that two concurrent
/// redemptions cannot both win, and a mock that checked-then-wrote would make
/// the race untestable.
pub struct MockEnrolmentRepo {
    pub rows: Mutex<Vec<EnrolmentTokenRow>>,
}

impl MockEnrolmentRepo {
    #[must_use]
    pub fn empty() -> Self {
        Self {
            rows: Mutex::new(Vec::new()),
        }
    }

    /// Seed a token row directly.
    pub fn push(&self, row: EnrolmentTokenRow) {
        self.rows.lock().unwrap().push(row);
    }

    /// The `used_at` of one token, for asserting redemption happened.
    #[must_use]
    pub fn used_at(&self, id: &str) -> Option<String> {
        self.rows
            .lock()
            .unwrap()
            .iter()
            .find(|r| r.id == id)
            .and_then(|r| r.used_at.clone())
    }

    #[must_use]
    pub fn count(&self) -> usize {
        self.rows.lock().unwrap().len()
    }
}

#[async_trait]
impl UserEnrolmentRepository for MockEnrolmentRepo {
    async fn create(&self, row: &EnrolmentTokenRow) -> anyhow::Result<()> {
        self.rows.lock().unwrap().push(row.clone());
        Ok(())
    }

    async fn find_redeemable(
        &self,
        token_hash: &str,
        now: &str,
    ) -> anyhow::Result<Option<EnrolmentTokenRow>> {
        Ok(self
            .rows
            .lock()
            .unwrap()
            .iter()
            .find(|r| {
                r.token_hash == token_hash && r.used_at.is_none() && r.expires_at.as_str() > now
            })
            .cloned())
    }

    async fn mark_used(&self, id: &str, now: &str) -> anyhow::Result<u64> {
        let mut rows = self.rows.lock().unwrap();
        match rows.iter_mut().find(|r| r.id == id && r.used_at.is_none()) {
            Some(row) => {
                row.used_at = Some(now.to_owned());
                Ok(1)
            }
            // Already spent: 0 rows affected, exactly as the guarded UPDATE
            // reports when it loses the race.
            None => Ok(0),
        }
    }

    async fn list_for_user(&self, user_id: &str) -> anyhow::Result<Vec<EnrolmentTokenRow>> {
        Ok(self
            .rows
            .lock()
            .unwrap()
            .iter()
            .filter(|r| r.user_id == user_id)
            .cloned()
            .collect())
    }

    async fn delete(&self, id: &str) -> anyhow::Result<u64> {
        let mut rows = self.rows.lock().unwrap();
        let before = rows.len();
        rows.retain(|r| r.id != id);
        Ok((before - rows.len()) as u64)
    }

    async fn delete_expired(&self, now: &str) -> anyhow::Result<u64> {
        let mut rows = self.rows.lock().unwrap();
        let before = rows.len();
        rows.retain(|r| r.expires_at.as_str() >= now);
        Ok((before - rows.len()) as u64)
    }
}

// ---------------------------------------------------------------------------
// API keys
// ---------------------------------------------------------------------------

/// In-memory [`ApiKeyRepository`].
pub struct MockApiKeyRepo {
    pub hashes: Mutex<Vec<(String, String)>>,
    pub touched: Mutex<Vec<String>>,
}

impl MockApiKeyRepo {
    #[must_use]
    pub fn empty() -> Self {
        Self {
            hashes: Mutex::new(Vec::new()),
            touched: Mutex::new(Vec::new()),
        }
    }

    #[must_use]
    pub fn with_hashes(hashes: Vec<(String, String)>) -> Self {
        Self {
            hashes: Mutex::new(hashes),
            touched: Mutex::new(Vec::new()),
        }
    }
}

#[async_trait]
impl ApiKeyRepository for MockApiKeyRepo {
    async fn find_all_hashes(&self) -> anyhow::Result<Vec<(String, String)>> {
        Ok(self.hashes.lock().unwrap().clone())
    }

    async fn create(
        &self,
        id: &str,
        _label: &str,
        key_hash: &str,
        _created_at: &str,
    ) -> anyhow::Result<()> {
        self.hashes
            .lock()
            .unwrap()
            .push((id.to_owned(), key_hash.to_owned()));
        Ok(())
    }

    async fn update_last_used(&self, id: &str, _now: &str) -> anyhow::Result<()> {
        self.touched.lock().unwrap().push(id.to_owned());
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// System config
// ---------------------------------------------------------------------------

/// In-memory [`SystemConfigRepository`] backed by one key/value map.
pub struct MockSystemConfigRepo {
    pub values: Mutex<HashMap<String, String>>,
}

impl MockSystemConfigRepo {
    #[must_use]
    pub fn empty() -> Self {
        Self {
            values: Mutex::new(HashMap::new()),
        }
    }

    #[must_use]
    pub fn with(key: &str, value: &str) -> Self {
        let mut map = HashMap::new();
        map.insert(key.to_owned(), value.to_owned());
        Self {
            values: Mutex::new(map),
        }
    }

    #[must_use]
    pub fn read(&self, key: &str) -> Option<String> {
        self.values.lock().unwrap().get(key).cloned()
    }
}

/// Only the six required methods are implemented.
///
/// The other twenty-six — `get_wizard_step`, `set_wizard_mode`,
/// `is_setup_completed` and friends — are trait defaults written in terms of
/// `get`/`set`, so backing those two with a real map means the mock exercises
/// the *actual* key names and encodings the daemon uses. Overriding them here
/// would let a test pass against a key the production code never writes.
#[async_trait]
impl SystemConfigRepository for MockSystemConfigRepo {
    async fn get(&self, key: &str) -> anyhow::Result<Option<String>> {
        Ok(self.values.lock().unwrap().get(key).cloned())
    }

    async fn set(&self, key: &str, value: &str) -> anyhow::Result<()> {
        self.values
            .lock()
            .unwrap()
            .insert(key.to_owned(), value.to_owned());
        Ok(())
    }

    async fn delete(&self, key: &str) -> anyhow::Result<()> {
        self.values.lock().unwrap().remove(key);
        Ok(())
    }

    async fn device_count(&self) -> anyhow::Result<i64> {
        Ok(0)
    }

    async fn tunnel_count(&self) -> anyhow::Result<i64> {
        Ok(0)
    }

    async fn db_size_bytes(&self) -> anyhow::Result<u64> {
        Ok(0)
    }
}

// ---------------------------------------------------------------------------
// Secret store
// ---------------------------------------------------------------------------

/// In-memory [`SecretStore`].
///
/// Lets a test assert that a client secret went to the vault and **not** into
/// `system_config`, which is the property that keeps it out of every API
/// response and every config dump.
pub struct MockSecretStore {
    pub values: Mutex<HashMap<String, Vec<u8>>>,
}

impl MockSecretStore {
    #[must_use]
    pub fn empty() -> Self {
        Self {
            values: Mutex::new(HashMap::new()),
        }
    }

    /// Whether a secret is stored at `path`.
    #[must_use]
    pub fn contains(&self, path: &str) -> bool {
        self.values.lock().unwrap().contains_key(path)
    }
}

#[async_trait]
impl wardnetd_data::secret_store::SecretStore for MockSecretStore {
    async fn put(&self, path: &str, value: &[u8]) -> anyhow::Result<()> {
        self.values
            .lock()
            .unwrap()
            .insert(path.to_owned(), value.to_vec());
        Ok(())
    }

    async fn get(&self, path: &str) -> anyhow::Result<Option<Vec<u8>>> {
        Ok(self.values.lock().unwrap().get(path).cloned())
    }

    async fn delete(&self, path: &str) -> anyhow::Result<()> {
        self.values.lock().unwrap().remove(path);
        Ok(())
    }

    async fn list(&self, prefix: &str) -> anyhow::Result<Vec<String>> {
        Ok(self
            .values
            .lock()
            .unwrap()
            .keys()
            .filter(|k| k.starts_with(prefix))
            .cloned()
            .collect())
    }
}
