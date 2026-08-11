use async_trait::async_trait;
use sqlx::SqlitePool;

use crate::db::DbPools;
use crate::repository::user::UserRole;
use crate::repository::user_credential::{
    CredentialAlreadyLinkedError, CredentialKind, CredentialLogin, CredentialRow,
    CredentialSummary, UserAlreadyHasPasswordError, UserCredentialRepository,
};

#[derive(sqlx::FromRow)]
struct DbCredentialRow {
    id: String,
    user_id: String,
    kind: String,
    subject: String,
    secret: Option<String>,
    label: Option<String>,
    metadata: String,
    created_at: String,
    last_used_at: Option<String>,
}

impl DbCredentialRow {
    /// An unparseable `kind` is an error, never a default: an unknown
    /// credential kind must not be silently treated as a usable one.
    fn into_domain(self) -> anyhow::Result<CredentialRow> {
        let kind = CredentialKind::parse(&self.kind).ok_or_else(|| {
            anyhow::anyhow!(
                "credential {} has unrecognised kind {:?}",
                self.id,
                self.kind
            )
        })?;
        Ok(CredentialRow {
            id: self.id,
            user_id: self.user_id,
            kind,
            subject: self.subject,
            secret: self.secret,
            label: self.label,
            metadata: self.metadata,
            created_at: self.created_at,
            last_used_at: self.last_used_at,
        })
    }
}

/// Login lookup row: the credential joined to the columns of `users` the
/// caller needs to build an `AuthContext`.
#[derive(sqlx::FromRow)]
struct DbCredentialLoginRow {
    id: String,
    user_id: String,
    kind: String,
    subject: String,
    secret: Option<String>,
    label: Option<String>,
    metadata: String,
    created_at: String,
    last_used_at: Option<String>,
    role: String,
    display_name: String,
}

impl DbCredentialLoginRow {
    fn into_domain(self) -> anyhow::Result<CredentialLogin> {
        let kind = CredentialKind::parse(&self.kind).ok_or_else(|| {
            anyhow::anyhow!(
                "credential {} has unrecognised kind {:?}",
                self.id,
                self.kind
            )
        })?;
        let role = UserRole::parse(&self.role).ok_or_else(|| {
            anyhow::anyhow!(
                "user {} has unrecognised role {:?}",
                self.user_id,
                self.role
            )
        })?;
        Ok(CredentialLogin {
            credential: CredentialRow {
                id: self.id,
                user_id: self.user_id,
                kind,
                subject: self.subject,
                secret: self.secret,
                label: self.label,
                metadata: self.metadata,
                created_at: self.created_at,
                last_used_at: self.last_used_at,
            },
            role,
            display_name: self.display_name,
        })
    }
}

/// A credential row without its secret. Separate `FromRow` type so the secret
/// column is never even selected on the listing path.
#[derive(sqlx::FromRow)]
struct DbCredentialSummaryRow {
    id: String,
    user_id: String,
    kind: String,
    subject: String,
    label: Option<String>,
    metadata: String,
    created_at: String,
    last_used_at: Option<String>,
}

impl DbCredentialSummaryRow {
    fn into_domain(self) -> anyhow::Result<CredentialSummary> {
        let kind = CredentialKind::parse(&self.kind).ok_or_else(|| {
            anyhow::anyhow!(
                "credential {} has unrecognised kind {:?}",
                self.id,
                self.kind
            )
        })?;
        Ok(CredentialSummary {
            id: self.id,
            user_id: self.user_id,
            kind,
            subject: self.subject,
            label: self.label,
            metadata: self.metadata,
            created_at: self.created_at,
            last_used_at: self.last_used_at,
        })
    }
}

const INSERT: &str = "INSERT INTO user_credentials \
     (id, user_id, kind, subject, secret, label, metadata, created_at, last_used_at) \
     VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)";

/// The login join. `u.enabled = 1` lives here, in SQL, so no caller can forget
/// it: a disabled user must be indistinguishable from a nonexistent one.
const FIND_FOR_LOGIN: &str = "SELECT c.id, c.user_id, c.kind, c.subject, c.secret, c.label, \
            c.metadata, c.created_at, c.last_used_at, u.role, u.display_name \
     FROM user_credentials c \
     JOIN users u ON u.id = c.user_id \
     WHERE c.kind = ? AND c.subject = ? AND u.enabled = 1";

const FIND_PASSWORD: &str = "SELECT id, user_id, kind, subject, secret, label, metadata, created_at, last_used_at \
     FROM user_credentials WHERE user_id = ? AND kind = 'password'";

/// Note the absent `secret` column — the listing path never reads it.
const LIST_FOR_USER: &str = "SELECT id, user_id, kind, subject, label, metadata, created_at, last_used_at \
     FROM user_credentials WHERE user_id = ? ORDER BY created_at DESC";

/// Upsert on the partial unique index, so a password change is one statement.
/// Delete-then-insert would leave a window in which the user has no password
/// at all, and a crash inside it would lock them out permanently.
///
/// The conflict target is the **partial `user_id` index**, not `(kind,
/// subject)`. A user who changes their email changes their password
/// credential's `subject` too, so a `(kind, subject)` target would miss the
/// existing row and then trip the one-password-per-user index as a hard error
/// instead of updating. `subject` is therefore part of the `DO UPDATE`.
///
/// If the new `subject` collides with *another* user's credential, the
/// `(kind, subject)` index still fires and surfaces as
/// `CredentialAlreadyLinkedError` — which is the correct answer.
const SET_PASSWORD: &str = "INSERT INTO user_credentials \
         (id, user_id, kind, subject, secret, label, metadata, created_at) \
     VALUES (?, ?, 'password', ?, ?, NULL, '{}', ?) \
     ON CONFLICT(user_id) WHERE kind = 'password' \
     DO UPDATE SET subject = excluded.subject, secret = excluded.secret";

const TOUCH_LAST_USED: &str = "UPDATE user_credentials SET last_used_at = ? WHERE id = ?";

const UPDATE_METADATA: &str = "UPDATE user_credentials SET metadata = ? WHERE id = ?";

const DELETE: &str = "DELETE FROM user_credentials WHERE id = ? AND user_id = ?";

const DELETE_BY_KIND: &str = "DELETE FROM user_credentials WHERE user_id = ? AND kind = ?";

const COUNT_BY_KIND: &str = "SELECT COUNT(*) FROM user_credentials WHERE user_id = ? AND kind = ?";

/// Classify a `SQLite` unique-constraint failure on `user_credentials`.
///
/// Two different indexes can fire and they mean very different things to the
/// caller, so they must not collapse into one generic error:
///
/// - `(kind, subject)` — the anti-hijack invariant: that provider account is
///   already linked to *some* user.
/// - the partial `WHERE kind='password'` index on `user_id` — this user
///   already has a password.
///
/// `SQLite` does not reliably populate `constraint()` with the index name — for
/// a partial index it reports neither the index nor a constraint, only the
/// offending **column**. So the dependable signal is the rendered message:
///
/// - `UNIQUE constraint failed: user_credentials.user_id` — the partial
///   one-password-per-user index. `user_id` alone is unambiguous: no other
///   unique index on this table mentions it.
/// - `UNIQUE constraint failed: user_credentials.kind, user_credentials.subject`
///   — the anti-hijack invariant.
///
/// `constraint()` is still checked first, in case a future driver populates it.
fn map_credential_conflict(err: sqlx::Error) -> anyhow::Error {
    let sqlx::Error::Database(ref db_err) = err else {
        return anyhow::Error::new(err);
    };
    let message = db_err.message();

    if db_err.constraint() == Some("idx_user_credentials_one_password")
        || message.contains("user_credentials.user_id")
    {
        return anyhow::Error::new(UserAlreadyHasPasswordError);
    }
    if message.contains("user_credentials.kind") || message.contains("user_credentials.subject") {
        return anyhow::Error::new(CredentialAlreadyLinkedError);
    }
    anyhow::Error::new(err)
}

/// `SQLite`-backed [`UserCredentialRepository`].
pub struct SqliteUserCredentialRepository {
    pools: DbPools,
}

impl SqliteUserCredentialRepository {
    /// Create a new repository backed by the given connection pool.
    #[must_use]
    pub fn new(pool: SqlitePool) -> Self {
        Self::new_pools(DbPools::single(pool))
    }

    /// Create a new repository with split reader/writer pools.
    #[must_use]
    pub fn new_pools(pools: DbPools) -> Self {
        Self { pools }
    }
}

#[async_trait]
impl UserCredentialRepository for SqliteUserCredentialRepository {
    async fn insert(&self, row: &CredentialRow) -> anyhow::Result<()> {
        sqlx::query(INSERT)
            .bind(&row.id)
            .bind(&row.user_id)
            .bind(row.kind.as_str())
            .bind(&row.subject)
            .bind(row.secret.as_deref())
            .bind(row.label.as_deref())
            .bind(&row.metadata)
            .bind(&row.created_at)
            .bind(row.last_used_at.as_deref())
            .execute(&self.pools.write)
            .await
            .map_err(map_credential_conflict)?;
        Ok(())
    }

    async fn find_for_login(
        &self,
        kind: CredentialKind,
        subject: &str,
    ) -> anyhow::Result<Option<CredentialLogin>> {
        let row = sqlx::query_as::<_, DbCredentialLoginRow>(FIND_FOR_LOGIN)
            .bind(kind.as_str())
            .bind(subject)
            .fetch_optional(&self.pools.read)
            .await?;
        row.map(DbCredentialLoginRow::into_domain).transpose()
    }

    async fn find_password(&self, user_id: &str) -> anyhow::Result<Option<CredentialRow>> {
        let row = sqlx::query_as::<_, DbCredentialRow>(FIND_PASSWORD)
            .bind(user_id)
            .fetch_optional(&self.pools.read)
            .await?;
        row.map(DbCredentialRow::into_domain).transpose()
    }

    async fn list_for_user(&self, user_id: &str) -> anyhow::Result<Vec<CredentialSummary>> {
        let rows = sqlx::query_as::<_, DbCredentialSummaryRow>(LIST_FOR_USER)
            .bind(user_id)
            .fetch_all(&self.pools.read)
            .await?;
        rows.into_iter()
            .map(DbCredentialSummaryRow::into_domain)
            .collect()
    }

    async fn set_password(
        &self,
        id: &str,
        user_id: &str,
        subject: &str,
        secret: &str,
        now: &str,
    ) -> anyhow::Result<()> {
        sqlx::query(SET_PASSWORD)
            .bind(id)
            .bind(user_id)
            .bind(subject)
            .bind(secret)
            .bind(now)
            .execute(&self.pools.write)
            .await
            .map_err(map_credential_conflict)?;
        Ok(())
    }

    async fn touch_last_used(&self, id: &str, now: &str) -> anyhow::Result<()> {
        sqlx::query(TOUCH_LAST_USED)
            .bind(now)
            .bind(id)
            .execute(&self.pools.write)
            .await?;
        Ok(())
    }

    async fn update_metadata(&self, id: &str, metadata: &str) -> anyhow::Result<u64> {
        let result = sqlx::query(UPDATE_METADATA)
            .bind(metadata)
            .bind(id)
            .execute(&self.pools.write)
            .await?;
        Ok(result.rows_affected())
    }

    async fn delete(&self, id: &str, user_id: &str) -> anyhow::Result<u64> {
        let result = sqlx::query(DELETE)
            .bind(id)
            .bind(user_id)
            .execute(&self.pools.write)
            .await?;
        Ok(result.rows_affected())
    }

    async fn delete_by_kind(&self, user_id: &str, kind: CredentialKind) -> anyhow::Result<u64> {
        let result = sqlx::query(DELETE_BY_KIND)
            .bind(user_id)
            .bind(kind.as_str())
            .execute(&self.pools.write)
            .await?;
        Ok(result.rows_affected())
    }

    async fn count_by_kind(&self, user_id: &str, kind: CredentialKind) -> anyhow::Result<i64> {
        let count = sqlx::query_scalar::<_, i64>(COUNT_BY_KIND)
            .bind(user_id)
            .bind(kind.as_str())
            .fetch_one(&self.pools.read)
            .await?;
        Ok(count)
    }
}
