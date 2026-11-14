mod application;
mod config;
mod domain;
mod infrastructure;
mod presentation;
mod shared;

use crate::{config::Config, infrastructure::database::couchdb::CouchDb, presentation::routes};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = Config::load()?;
    let couchdb = CouchDb::new(
        &config.couchdb_url,
        &config.couchdb_database,
        config.couchdb_user,
        config.couchdb_password,
    )?;
    let listener = tokio::net::TcpListener::bind(config.api_addr).await?;
    println!(
        "API BAD 215 disponible en http://{}",
        listener.local_addr()?
    );
    axum::serve(listener, routes::router(couchdb)).await?;
    Ok(())
}
