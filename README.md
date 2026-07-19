# Readactus

Safe, production-quality copies of relational databases — with the PII replaced, the referential integrity intact, and the original data unrecoverable.

Readactus connects to a production database read-only, identifies sensitive columns by schema analysis, and produces a copy where every PII value has been deterministically replaced with a realistic synthetic equivalent. Same email in two tables becomes the same synthetic email — joins survive, the data does not.

**Daemon Labs** — v0.1.0

---

## Prerequisites

- **Rust 1.86+** (the workspace's `rust-version` minimum)
- **DDBCore** checked out as a sibling directory:
  ```
  ~/workspace/DDBCore      # the DAL — Readactus consumes it via path deps
  ~/workspace/Readactus    # this repo
  ```
  Readactus depends on `ddbcore`, `ddbcore-postgres`, and `ddbcore-mysql` via `path = "../DDBCore/crates/..."` in the workspace `Cargo.toml`. Both repos must be present before building.
- **Docker** (only for running the integration tests — not required for building)

If you use [mise](https://mise.jdx.dev/), `mise install` in the repo root will pin the Rust toolchain automatically.

## Build

```bash
cargo build
```

This compiles all six workspace crates. The dev CLI binary lands at `target/debug/readactus`.

For a release build:

```bash
cargo build --release
```

## Run tests

Unit tests (no Docker required):

```bash
cargo test
```

Integration tests use [testcontainers](https://docs.rs/testcontainers) and require a running Docker daemon:

```bash
cargo test -- --ignored
```

## CLI usage

The CLI (`readactus`) is a dev harness — it exercises the pipeline but is not the product surface (a desktop GUI is planned).

### Scan a database for sensitive columns

```bash
readactus scan \
  --engine postgres \
  --host localhost --port 5432 \
  --database mydb \
  --username readonly_user \
  --password "$DB_PASSWORD" \
  --threshold 0.5
```

Reports every column the detector flags above the confidence threshold, with the PII kind, confidence score, and human-readable reason.

### Copy with PII redacted

```bash
readactus copy \
  --engine postgres \
  --host prod-host --port 5432 \
  --database prod_db \
  --username readonly_user \
  --password "$DB_PASSWORD" \
  --target-engine postgres \
  --target-host localhost --target-port 5433 \
  --target-database safe_copy \
  --target-username admin \
  --target-password "$TARGET_PASSWORD" \
  --threshold 0.7
```

The password arguments also accept environment variables `READACTUS_DB_PASSWORD` and `READACTUS_TARGET_DB_PASSWORD`.

Source connections are forced read-only in code regardless of what credentials you supply. The copy runs in three phases: bare tables, transformed data, then constraints and indexes.

### Licensing

The free tier hard-caps extraction at 50 MB per run. Registering removes the cap.

```bash
# Activate a registration key on this machine
readactus activate RDX1-<your-key>

# Check current license status
readactus license
```

Keys are Ed25519-signed, verified fully offline, and node-locked to the activating machine.

## Workspace crates

| Crate | Purpose |
|---|---|
| `readactus-core` | Pipeline: connect, reflect, detect, plan, copy. Enforces source read-only and extraction metering. |
| `readactus-detect` | Schema-only sensitive-column detection. Rule-based v1 covering 9 PII kinds. |
| `readactus-transform` | Deterministic HMAC-SHA256 tokenization with realistic per-kind synthesis. |
| `readactus-license` | Ed25519 registration keys, offline verification, node-locked activation. |
| `readactus-keygen` | Internal key issuer tool. Never shipped. |
| `readactus-cli` | Dev CLI harness. |

## Supported engines

- **PostgreSQL**
- **MySQL / MariaDB**

SQL Server and Oracle adapters are planned via DDBCore.

## Security notes

- Source connections are forced read-only in code — Readactus never modifies a production database.
- The run key is held in `Zeroizing` memory; no reversible mapping is stored anywhere.
- `issuer.secret` (the Ed25519 signing key) is gitignored and must never leave controlled storage.
