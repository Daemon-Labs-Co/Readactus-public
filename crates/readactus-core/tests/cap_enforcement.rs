//! Integration tests for extraction-cap enforcement on a real database:
//! an unregistered-entitlements copy must die mid-stream the moment the
//! cap is crossed (leaving the run unresumable and partial), and a
//! registered copy of the same source must sail through.
//!
//! The cap value is injected small so the test doesn't need to move 50 MB
//! through Docker — what's under test is the enforcement mechanism, which
//! is identical at any cap value.

use std::sync::Arc;

use ddbcore::{ConnectionConfig, EncryptionMode};
use readactus_core::{build_plan, connect_source, connect_target, run_copy, Engine, ReadactusError};
use readactus_license::Entitlements;
use readactus_transform::{RunKey, Tokenizer};
use testcontainers_modules::postgres::Postgres;
use testcontainers_modules::testcontainers::runners::AsyncRunner;
use testcontainers_modules::testcontainers::{ContainerAsync, ImageExt};

async fn start_postgres() -> (ContainerAsync<Postgres>, ConnectionConfig) {
    let container = Postgres::default().with_tag("16-alpine").start().await.expect("start postgres");
    let port = container.get_host_port_ipv4(5432).await.expect("mapped port");
    let config = ConnectionConfig {
        host: "127.0.0.1".into(),
        port,
        database: "postgres".into(),
        username: "postgres".into(),
        password: "postgres".into(),
        encryption: EncryptionMode::ClearText,
        read_only: false,
    };
    (container, config)
}

async fn seed_source(config: &ConnectionConfig) {
    let conn = connect_target(Engine::Postgres, config.clone()).await.expect("connect for seeding");
    conn.execute_query(
        "CREATE TABLE logs (id INT PRIMARY KEY, customer_email TEXT NOT NULL, payload TEXT NOT NULL)",
        &[],
    )
    .await
    .expect("create source table");
    // 50 rows × ~1KB payload ≈ 50KB of extractable data.
    conn.execute_query(
        "INSERT INTO logs SELECT g, 'user' || g || '@corp.com', repeat('x', 1000) FROM generate_series(1, 50) g",
        &[],
    )
    .await
    .expect("seed rows");
}

#[tokio::test]
async fn unregistered_copy_dies_at_cap_and_stays_partial() {
    let (_src_c, src_cfg) = start_postgres().await;
    let (_tgt_c, tgt_cfg) = start_postgres().await;
    seed_source(&src_cfg).await;

    let source = connect_source(Engine::Postgres, src_cfg).await.expect("source");
    let target = connect_target(Engine::Postgres, tgt_cfg).await.expect("target");

    let catalog = source.reflect_schema().await.expect("reflect");
    let (plan, _) = build_plan(&catalog, 0.6);
    let tokenizer = Arc::new(Tokenizer::new(RunKey::from_bytes([9u8; 32])));

    // Cap far below the ~50KB the source holds.
    let capped = Entitlements { max_extract_bytes: Some(10_000), registered: false };
    let result = run_copy(&*source, &*target, &catalog, &plan, Arc::clone(&tokenizer), &capped).await;

    match result {
        Err(ReadactusError::ExtractionCapExceeded { limit }) => assert_eq!(limit, 10_000),
        other => panic!("expected ExtractionCapExceeded, got {other:?}"),
    }

    // "If the stream dies, it dies": whatever landed stays partial — the
    // full source row count must NOT be present on the target.
    let rows = target.execute_query("SELECT count(*)::bigint FROM logs", &[]).await.expect("count");
    let copied = match rows[0].0[0] {
        ddbcore::Value::BigInt(n) => n,
        ref other => panic!("expected bigint count, got {other:?}"),
    };
    assert!(copied < 50, "cap-killed run must not have copied everything (got {copied}/50 rows)");
}

#[tokio::test]
async fn registered_copy_of_same_source_completes() {
    let (_src_c, src_cfg) = start_postgres().await;
    let (_tgt_c, tgt_cfg) = start_postgres().await;
    seed_source(&src_cfg).await;

    let source = connect_source(Engine::Postgres, src_cfg).await.expect("source");
    let target = connect_target(Engine::Postgres, tgt_cfg).await.expect("target");

    let catalog = source.reflect_schema().await.expect("reflect");
    let (plan, _) = build_plan(&catalog, 0.6);
    let tokenizer = Arc::new(Tokenizer::new(RunKey::from_bytes([9u8; 32])));

    let report = run_copy(&*source, &*target, &catalog, &plan, tokenizer, &Entitlements::registered())
        .await
        .expect("registered copy must complete");
    assert_eq!(report.total_rows, 50);

    // And the PII must still be transformed — registration lifts the cap,
    // not the redaction.
    let rows = target
        .execute_query("SELECT count(*)::bigint FROM logs WHERE customer_email LIKE '%corp.com%'", &[])
        .await
        .expect("pii check");
    assert_eq!(rows[0].0[0], ddbcore::Value::BigInt(0), "original emails must not survive a registered copy");
}
