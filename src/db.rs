use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;
use std::time::Duration;

/// Creates a connection pool for a PostgreSQL database.
///
/// # Arguments
///
/// * `database_url` - A string slice that holds the database connection URL.
///
/// # Configuration
///
/// The connection pool is configured with the following options:
/// - `max_connections`: The maximum number of connections in the pool (set to 5).
/// - `acquire_timeout`: The maximum duration to wait for a connection (set to 3 seconds).
///
/// # Errors
///
/// This function returns a `Result` with:
/// - `Ok(PgPool)` if the connection pool is successfully created.
/// - `Err(sqlx::Error)` if there is an error, such as:
///   - The `database_url` is invalid.
///   - The database server is unreachable.
///   - A timeout occurs while acquiring a connection.
///
/// # Example
///
/// ```
/// # use queensac::db::create_pool;
/// # use sqlx::PgPool;
/// # async fn example() -> Result<(), sqlx::Error> {
/// let database_url = "postgres://username:password@localhost/dbname";
/// let pool: PgPool = create_pool(database_url).await?;
/// # Ok(())
/// # }
/// ```
pub async fn create_pool(database_url: &str) -> Result<PgPool, sqlx::Error> {
    let max_connections: u32 = std::env::var("DB_MAX_CONNECTIONS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(5);
    let acquire_timeout: u64 = std::env::var("DB_ACQUIRE_TIMEOUT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(3);

    PgPoolOptions::new()
        .max_connections(max_connections)
        .acquire_timeout(Duration::from_secs(acquire_timeout))
        .connect(database_url)
        .await
}

pub async fn init_db(pool: &PgPool) -> Result<(), sqlx::Error> {
    sqlx::migrate!("./migrations").run(pool).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    #[tokio::test]
    async fn test_database_connection() {
        dotenvy::dotenv().ok();
        let database_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");

        let pool = create_pool(&database_url)
            .await
            .expect("Failed to create database pool");

        let row: (i32,) = sqlx::query_as("SELECT 1")
            .fetch_one(&pool)
            .await
            .expect("Failed to fetch query result");

        assert_eq!(row.0, 1);
    }
}
