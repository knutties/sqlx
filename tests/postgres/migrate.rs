use sqlx::migrate::Migrator;
use sqlx::pool::PoolConnection;
use sqlx::postgres::{PgConnection, Postgres};
use sqlx::AssertSqlSafe;
use sqlx::Executor;
use sqlx::Row;
use std::path::Path;

#[sqlx::test(migrations = false)]
async fn simple(mut conn: PoolConnection<Postgres>) -> anyhow::Result<()> {
    clean_up(&mut conn).await?;

    let migrator = Migrator::new(Path::new("tests/postgres/migrations_simple")).await?;

    // run migration
    migrator.run(&mut conn).await?;

    // check outcome
    let res: String = conn
        .fetch_one("SELECT some_payload FROM migrations_simple_test")
        .await?
        .get(0);
    assert_eq!(res, "110_suffix");

    // running it a 2nd time should still work
    migrator.run(&mut conn).await?;

    Ok(())
}

#[sqlx::test(migrations = false)]
async fn reversible(mut conn: PoolConnection<Postgres>) -> anyhow::Result<()> {
    clean_up(&mut conn).await?;

    let migrator = Migrator::new(Path::new("tests/postgres/migrations_reversible")).await?;

    // run migration
    migrator.run(&mut conn).await?;

    // check outcome
    let res: i64 = conn
        .fetch_one("SELECT some_payload FROM migrations_reversible_test")
        .await?
        .get(0);
    assert_eq!(res, 101);

    // roll back nothing (last version)
    migrator.undo(&mut conn, 20220721125033).await?;

    // check outcome
    let res: i64 = conn
        .fetch_one("SELECT some_payload FROM migrations_reversible_test")
        .await?
        .get(0);
    assert_eq!(res, 101);

    // roll back one version
    migrator.undo(&mut conn, 20220721124650).await?;

    // check outcome
    let res: i64 = conn
        .fetch_one("SELECT some_payload FROM migrations_reversible_test")
        .await?
        .get(0);
    assert_eq!(res, 100);

    Ok(())
}

#[sqlx::test(migrations = false)]
async fn skip(mut conn: PoolConnection<Postgres>) -> anyhow::Result<()> {
    clean_up(&mut conn).await?;
    let migrator = Migrator::new(Path::new("tests/postgres/migrations_reversible")).await?;

    // get to the state of after the first migration manually
    let sql = include_str!("migrations_reversible/20220721124650_add_table.up.sql");
    let statements: Vec<&str> = sql.split(';').filter(|s| !s.trim().is_empty()).collect();
    for statement in statements {
        conn.execute(statement).await?;
    }

    // skip first migration
    migrator.skip(&mut conn, Some(20220721124650)).await?;

    // check outcome
    let res: i64 = conn
        .fetch_one("SELECT some_payload FROM migrations_reversible_test")
        .await?
        .get(0);
    assert_eq!(res, 100);

    // run remaining migration
    migrator.run(&mut conn).await?;

    // check outcome
    let res: i64 = conn
        .fetch_one("SELECT some_payload FROM migrations_reversible_test")
        .await?
        .get(0);
    assert_eq!(res, 101);

    // roll back one version
    migrator.undo(&mut conn, 20220721124650).await?;

    // check outcome
    let res: i64 = conn
        .fetch_one("SELECT some_payload FROM migrations_reversible_test")
        .await?
        .get(0);
    assert_eq!(res, 100);

    Ok(())
}

#[sqlx::test(migrations = false)]
async fn no_tx(mut conn: PoolConnection<Postgres>) -> anyhow::Result<()> {
    clean_up(&mut conn).await?;
    let migrator = Migrator::new(Path::new("tests/postgres/migrations_no_tx")).await?;

    // run migration
    migrator.run(&mut conn).await?;

    // check outcome
    let res: String = conn
        .fetch_one("SELECT datname FROM pg_database WHERE datname = 'test_db'")
        .await?
        .get(0);

    assert_eq!(res, "test_db");

    Ok(())
}

#[sqlx::test(migrations = false)]
async fn pending(mut conn: PoolConnection<Postgres>) -> anyhow::Result<()> {
    clean_up(&mut conn).await?;

    let migrator = Migrator::new(Path::new("tests/postgres/migrations_simple")).await?;
    let total = migrator.iter().count();
    assert!(total >= 2, "test fixture changed");

    // Initial: every migration is pending, in version order.
    let pending: Vec<_> = migrator
        .pending(&mut conn)
        .await?
        .into_iter()
        .map(|m| m.version)
        .collect();
    let all_versions: Vec<_> = migrator.iter().map(|m| m.version).collect();
    assert_eq!(pending, all_versions);

    // After run: nothing pending.
    migrator.run(&mut conn).await?;
    assert!(migrator.pending(&mut conn).await?.is_empty());

    Ok(())
}

#[sqlx::test(migrations = false)]
async fn render_pending_sql(mut conn: PoolConnection<Postgres>) -> anyhow::Result<()> {
    clean_up(&mut conn).await?;

    let migrator = Migrator::new(Path::new("tests/postgres/migrations_simple")).await?;

    let sql = migrator.render_pending_sql(&mut conn).await?;
    assert!(!sql.is_empty());
    for migration in migrator.iter() {
        let header = format!("-- migration {}", migration.version);
        assert!(sql.contains(&header), "missing header for {header}");
    }

    // Apply the rendered script as raw SQL. After applying, `pending` must be
    // empty — proving the script is self-applying (including bookkeeping rows).
    conn.execute(sqlx::raw_sql(AssertSqlSafe(sql.clone()))).await?;
    assert!(migrator.pending(&mut conn).await?.is_empty());

    // `run` against the populated state must be a no-op (no checksum mismatch).
    migrator.run(&mut conn).await?;

    assert!(migrator.render_pending_sql(&mut conn).await?.is_empty());

    Ok(())
}

/// Ensure that we have a clean initial state.
async fn clean_up(conn: &mut PgConnection) -> anyhow::Result<()> {
    conn.execute("DROP DATABASE IF EXISTS test_db").await.ok();
    conn.execute("DROP TABLE migrations_simple_test").await.ok();
    conn.execute("DROP TABLE migrations_reversible_test")
        .await
        .ok();
    conn.execute("DROP TABLE _sqlx_migrations").await.ok();

    Ok(())
}
