//! SQL rendering of migrations for offline review or out-of-band application.
//!
//! See [`MigrateRender`] for the per-driver trait, and
//! [`Migrator::render_pending_sql`](super::Migrator::render_pending_sql) for the
//! ergonomic entry point that dumps all pending migrations.

use crate::migrate::Migration;

/// Renders the SQL that [`Migrate::apply`](super::Migrate::apply) would execute,
/// for offline review or out-of-band application via the database's own client
/// (`psql`, `mysql`, `sqlite3`).
///
/// Implemented per-driver because the SQL form differs across databases:
/// transaction framing (Postgres/SQLite honor [`Migration::no_tx`]; MySQL always
/// wraps and uses a `success=FALSE`/`UPDATE … TRUE` pair due to implicit DDL
/// commits), bytea/blob literal syntax (`'\xHEX'::bytea` vs. `X'HEX'`), and so on.
///
/// The rendered SQL is **self-applying**: executing it produces the same post-state
/// as `apply()` (including the bookkeeping insert into the migrations table), so a
/// subsequent [`Migrator::run`](super::Migrator::run) sees the version as already
/// applied and does not re-apply.
pub trait MigrateRender {
    /// Append to `buf` the SQL that [`apply`](super::Migrate::apply) would execute
    /// for `migration`, using `table_name` for the migrations table.
    ///
    /// Output includes any per-driver transaction framing and the bookkeeping
    /// `INSERT` (or `INSERT`/`UPDATE` flow) into the migrations table.
    /// `&self` is unused but required for trait dispatch via the connection type.
    fn append_apply_sql(&self, table_name: &str, migration: &Migration, buf: &mut String);
}

/// Append `bytes` to `buf` as lowercase hex (no separators, no prefix).
///
/// Useful for building per-driver bytea/blob literals (`'\x…'::bytea`, `X'…'`).
#[doc(hidden)]
pub fn append_hex(buf: &mut String, bytes: &[u8]) {
    use std::fmt::Write;
    for b in bytes {
        let _ = write!(buf, "{b:02x}");
    }
}

/// Append `s` to `buf` as a single-quoted SQL string literal, doubling embedded
/// single quotes per ANSI SQL.
#[doc(hidden)]
pub fn append_sql_string(buf: &mut String, s: &str) {
    buf.push('\'');
    for c in s.chars() {
        if c == '\'' {
            buf.push('\'');
        }
        buf.push(c);
    }
    buf.push('\'');
}
