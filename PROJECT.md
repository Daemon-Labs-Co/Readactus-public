# Readactus — Project Status

*Daemon Labs. Last updated: 2026-07-17.*

Readactus is a desktop application that creates safe, production-quality copies of relational databases: it connects to a production database read-only, identifies sensitive columns, and produces a copy in which PII has been replaced with realistic synthetic values — deterministically, so referential integrity and joins survive — while none of the original sensitive data can be recovered.

This document records what exists, what was decided and why, and what's next.

---

## 1. Architecture at a glance

Two sibling repositories:

```
~/workspace/DDBCore      the DAL — standalone, publishable Rust crate family
~/workspace/Readactus    the product — consumes DDBCore via path deps
```

**Language: Rust**, chosen deliberately: the threat model includes an observer of process memory, and Rust allows explicit zeroing of sensitive buffers (`zeroize`) with no GC copying data behind our backs. JS/TS ruled out from the start; Python rejected for the same memory-hygiene reasons.

### DDBCore ("Daemon DB Core")

Engine-agnostic database abstraction — the SQLAlchemy-Core-for-Rust that didn't exist. One `Connection` trait per engine covering: exhaustive schema reflection (tables, columns, PK/FK/unique/check/exclusion constraints, indexes, triggers, functions, views, sequences, identity, partitioning), cursor-backed streaming reads with bounded memory, engine-native bulk writes (Postgres `COPY`; MySQL batched INSERT with placeholder/byte caps), DDL generation and mutation, ad-hoc queries (materialized + streaming), and a `Dialect` capability surface so generic callers never hardcode one engine's SQL.

- Crates: `ddbcore` (canonical model + traits), `ddbcore-postgres`, `ddbcore-mysql`, `ddbcore-testkit` (engine-agnostic contract tests).
- Adapters implemented: **PostgreSQL, MySQL/MariaDB**. Planned: SQL Server (tiberius — sqlx has no mssql support), Oracle.
- Hardened by three staff-level review passes: all High findings from review 1 (correctness: COPY encoding, timestamp semantics, unsigned decode, identity/partition/check-constraint fidelity, etc.), all High+Medium from review 2 (performance: allocation-free COPY encoder, dedicated data-plane connections, MySQL cancellation cliff, placeholder cap, per-column decoder tables, `StreamOptions` with projection/key-range parallelism), and the Low-severity backlog. Review 3 found **no regressions**; its 3 Medium + 6 Low findings are pending a go decision.
- Testing: two tiers — contract tests written once against the trait, run against every adapter; plus per-engine integration tests on throwaway dockerized databases (`testcontainers`, config in `.env.testing` shared with `docker-compose.testing.yml`). Criterion benches lock in the COPY-encoder performance (~1.14 µs per 10-column row).

### Readactus crates

| Crate | Purpose |
|---|---|
| `readactus-detect` | Sensitive-column detection over reflected schema only (no data read). Rule-based v1: 9 PII kinds, name normalization, type gates, confidence + human-readable reasons. `Detector` trait ready for the planned local-LLM implementation (llama.cpp FFI) as a second pass. |
| `readactus-transform` | Deterministic, irreversible replacement. Keyed HMAC-SHA256 tokenization — same input → same output within a run (the referential-integrity guarantee), no reversible mapping stored anywhere, run key held in `Zeroizing` memory. Realistic synthesis per kind: valid-format emails, reserved 555-01xx phones, out-of-allocation SSNs, Luhn-valid test-range card numbers, TEST-NET IPs, year-preserving DOBs; credentials get deliberately unrealistic `REDACTED-…` tokens. |
| `readactus-core` | Pipeline: connect → reflect → detect → plan → copy. Source connections **forced read-only in code**. Plans are per-column, derived from detection but fully user-overridable, each carrying its finding and reason (built for the future UI's review screen). Three-phase copy: bare tables → transformed data → constraints/indexes. Extraction byte-metering (see licensing). |
| `readactus-license` | Registration keys and entitlements (see below). |
| `readactus-keygen` | Internal issuer tool — never shipped. `issuer.secret` is gitignored; treat it as the crown jewels. |
| `readactus-cli` | Dev harness (`scan`, `copy`, `activate`, `license`). Not the product surface. |

---

## 2. Business-model decisions (deliberate, recorded)

1. **Clear-text DB connections are the free default; TLS/SSL is a paid-tier feature.** Acknowledged tension with "security before convenience" — this is an intentional upgrade incentive, not an oversight. Do not "fix" it to default-secure.
2. **Free tier: extraction hard-capped at 50 MB per run.** Enforced by metering the raw source stream (pre-transform) with one counter shared across all tables. The moment the next row would cross the cap the run aborts — mid-stream, unresumable, partial copy stays partial. "Absolutely no larger."
3. **Registered (paid) tier: unlimited extraction, node-locked to exactly one computer.** Keys are Ed25519-signed (`RDX1-<payload>-<sig>`), verified fully offline against the issuer public key embedded in the app; unforgeable and tamper-evident. Activation binds the key to a hashed machine fingerprint; a copied activation file refuses to load elsewhere, and the signature is re-verified on every load.
4. **Known limitation, stated honestly:** offline node-locking stops casual key sharing but cannot detect the same key independently activated on two machines. True one-machine enforcement needs an activation server; the local data model (key id + fingerprint) is exactly what that server will record.

---

## 3. What is proven working (verified live, not assumed)

End-to-end against real Postgres containers:

- Detection found **all 7 planted PII columns, zero false positives** on a realistic schema.
- Copy produced a target where `alice.smith@corp.com` became the same synthetic address in *both* tables it appeared in — deterministic cross-table consistency; FK constraint recreated; joins intact.
- Birth years preserved (age cohorts stay representative), month/day scrambled; non-sensitive data byte-identical; **zero original PII in the target, verified by query**.
- The 50 MB cap killed an 80 MB unregistered copy mid-stream with a clear error; issuing and activating a key lifted the cap and the same copy completed (70,000 rows).
- The e2e run also flushed out a real DDBCore bug (`CREATE INDEX … USING` placement) — fixed, with contract-test coverage added.

Test counts: Readactus 26 (units + 2 testcontainers integration tests for cap enforcement and registered-copy completion); DDBCore 25+ plus benches. All green.

---

## 4. Open items

**Readactus:**
- Desktop GUI — the actual product surface. egui vs iced still undecided (JS/TS webview ruled out). CLI is scaffolding, not the answer.
- Local-LLM detection pass (llama.cpp FFI, fully offline) layered on the rule engine.
- Activation server for real one-machine key enforcement.
- Value sampling to raise detection confidence (currently schema-only).
- macOS/Windows machine fingerprints (Linux `/etc/machine-id` today).
- Tier interpretation to confirm: free = 50 MB cap / registered = unlimited is the implemented reading; if the key itself should carry the cap, it's a two-line `Entitlements` change.

**DDBCore:**
- Review 3's findings await a go: 3 Medium (MySQL `tinyint(1)` decodes as SmallInt not Boolean; InnoDB FK auto-index duplication in render replay; `CHECK … NOT VALID` framing strip) + 6 Low.
- SQL Server adapter (tiberius), then Oracle.
- Reflection N+1 batching rework (accepted debt; will restructure `reflect.rs`, at which point split it into a `reflect/` directory module).
- Uncommitted at time of writing: the `CREATE INDEX USING` fix + testkit coverage (repo may be on branch `fix/low_severity_review_pass` — check before committing).

---

## 5. Operating conventions

- **Never commit on the user's behalf. Ever.** Stage and draft messages; Mark commits manually.
- DDBCore and Readactus are separate projects with separate commits; don't mix work across them in one session without being asked.
- Two-tier testing everywhere: engine-agnostic contract tests + dockerized per-engine integration tests. Container config lives in `.env.testing` (committed; throwaway credentials only).
- `issuer.secret` never enters git, never ships, never leaves controlled storage.
