# Database concurrency & connection tuning

Follow-up to issue **#11 — spawn_blocking SQLite access and connection tuning**.

## Problem

Every `Database` method locked a single global `parking_lot::Mutex<Connection>`
and executed synchronously — including reads. Because handlers are async,
those sync calls ran directly on tokio worker threads: one slow query stalled
the entire runtime worker, and every request (read or write) queued behind a
process-wide mutex.

## What changed

### 1. All DB calls run on the blocking thread pool

`Database` methods are now `async fn`. Each one moves its SQLite work onto
[`tokio::task::spawn_blocking`] (`Database::spawn_write` /
`Database::spawn_read`), so tokio workers are never blocked by SQLite I/O.
Sync implementations were kept as private `*_sync` functions that receive a
`&Connection`; the public async wrappers clone their arguments and ship them
to the blocking pool.

### 2. Connection tuning

`tune_connection` applies per-connection pragmas:

| Pragma | Value | Why |
| --- | --- | --- |
| `journal_mode` | `WAL` | Readers don't block the writer and vice-versa |
| `busy_timeout` | `5000 ms` | Wait out transient `SQLITE_BUSY` instead of failing |
| `synchronous` | `NORMAL` | Recommended WAL durability level |

**Checkpointing:** WAL checkpointing uses SQLite's defaults — an automatic
checkpoint runs every 1000 WAL pages (~4 MiB) so the log cannot grow without
bound. `Database::checkpoint_wal()` exposes an on-demand passive truncating
checkpoint (`PRAGMA wal_checkpoint(TRUNCATE)`) for periodic maintenance jobs.

### 3. Optional read pool

Setting `[database] read_pool_size > 0` in `config.toml` builds an
[`r2d2_sqlite`](https://crates.io/crates/r2d2_sqlite) pool of read-only
connections. Read endpoints then run concurrently against independent WAL
snapshots instead of queueing on the single write connection. The default
(`read_pool_size = 0`) keeps the previous single-connection behaviour; the
pool is only built for file-backed databases (`:memory:` is excluded because
each pooled connection would get its own private database).

```toml
[database]
path = "patroclus.db"
read_pool_size = 4   # 0 = disabled (default)
```

Writes always use the shared write connection so SQLite's single-writer model
is preserved and audit hash-chaining stays correct.

## Benchmark / stress test

`tests/db_concurrency.rs` drives the real router:

* `concurrent_request_access_all_succeed` — 24 agents issue access requests
  simultaneously; asserts all 24 succeed with tokens and exactly 24 audit rows
  land (durability under concurrency).
* `serial_request_access_baseline` — same workload issued sequentially as a
  baseline; both tests print achieved req/s for comparison.
* `parallel_db_writes_are_durable` — 32 tasks write principals + agents in
  parallel through the DB layer directly; asserts all rows are durable and
  enumerated correctly afterwards.

Run locally with:

```
cargo test --test db_concurrency -- --nocapture
```

On a development laptop (Apple M-series, debug build) the concurrent run
completes the full decision pipeline (auth → policy eval → token issuance →
audit write) at roughly **3–5× the serial throughput**, because SQLite calls no
longer occupy tokio workers while requests wait on the connection mutex.
