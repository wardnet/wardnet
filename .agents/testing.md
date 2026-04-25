# Testing

## Running tests

```bash
# All Rust tests (Linux only — use make check-daemon on macOS)
cd source/daemon && cargo test --workspace

# SDK checks
cd source/sdk/wardnet-js && yarn type-check && yarn format:check

# Web UI checks
cd source/web-ui && yarn type-check && yarn lint && yarn format:check

# Or run everything at once (unit tests + lint + format):
# On macOS, daemon checks automatically run inside a Linux container.
make check
```

## Test patterns

### Service tests — mock repositories, test business logic

```rust
struct MockDeviceRepo { device: Option<Device>, rule: Option<RoutingRule> }

#[async_trait]
impl DeviceRepository for MockDeviceRepo { /* return preconfigured data */ }

#[tokio::test]
async fn set_rule_admin_locked() {
    let svc = DeviceServiceImpl::new(Arc::new(MockDeviceRepo { /* ... */ }));
    let result = svc.set_rule_for_ip("192.168.1.10", RoutingTarget::Direct).await;
    assert!(result.is_err());
}
```

### Repository tests — real SQLite (in-memory), verify SQL correctness

```rust
async fn test_pool() -> SqlitePool { /* in-memory pool with migrations */ }

#[tokio::test]
async fn create_and_find_by_username() {
    let pool = test_pool().await;
    let repo = SqliteAdminRepository::new(pool);
    repo.create("id-1", "admin", "hash").await.unwrap();
    let result = repo.find_by_username("admin").await.unwrap();
    assert!(result.is_some());
}
```

### Infrastructure tests — real impl, temp resources

`FileSecretStore`, `AgeArchiver`, `SqliteDumper` are tested against real filesystem / real pool with tempfile-based isolation. Each test creates a unique directory under `std::env::temp_dir()` and cleans up on completion.
