use std::{env, str::FromStr as _};
use uuid::Uuid;

pub fn main() -> anyhow::Result<()> {
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async move {
        let database_url =
            env::var("DATABASE_URL").expect("DATABASE_URL is not set in the environment");

        let db = sqlx::Pool::<sqlx::Sqlite>::connect_with(
            sqlx::sqlite::SqliteConnectOptions::from_str(&database_url)?.create_if_missing(true),
        )
        .await?;

        let library_dir = dirs::audio_dir().unwrap_or_else(|| {
            let home = dirs::home_dir().expect("No Home Dir???");
            home.join("Music")
        });
        let r = sqlx::query!(
            r#"INSERT INTO libraries (id, path) VALUES (?, ?)"#,
            Uuid::new_v4(),
            library_dir.to_str().expect("Should be a valid utf-8 str"),
        )
        .execute(&db)
        .await?;

        anyhow::Ok(())
    })?;
    Ok(())
}
