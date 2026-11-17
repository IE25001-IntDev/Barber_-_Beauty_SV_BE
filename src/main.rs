mod application;
mod config;
mod domain;
mod infrastructure;
mod presentation;
mod shared;

use crate::{config::Config, infrastructure::database::couchdb::CouchDb, presentation::routes};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    if std::env::args().nth(1).as_deref() == Some("--crear-usuarios-demo") {
        return presentation::auth::create_demo(".auth-users.json");
    }
    let config = Config::load()?;
    let auth = presentation::auth::Auth::load(&config.auth_users_file)?;
    let couchdb = CouchDb::new(
        &config.couchdb_url,
        &config.couchdb_database,
        config.couchdb_user,
        config.couchdb_password,
    )?;
    if std::env::args().nth(1).as_deref() == Some("--preparar-catalogo-demo") {
        let mut documents = Vec::new();
        for (id, nombre) in [("ana", "Ana Martínez"), ("carlos", "Carlos Rivera"), ("sofia", "Sofía López")] {
            documents.push(serde_json::json!({"_id":format!("demo-empleado-{id}"), "tipo":"empleado", "nombre":nombre, "activo":true}));
        }
        for (id, nombre, duracion, precio) in [("corte", "Corte clásico", 30, 800), ("barba", "Diseño de barba", 20, 500), ("combo", "Corte y barba", 50, 1200), ("color", "Color y estilo", 60, 2500)] {
            documents.push(serde_json::json!({"_id":format!("demo-servicio-{id}"), "tipo":"servicio", "nombre":nombre, "activo":true, "duracion_minutos":duracion, "precio_centavos":precio}));
        }
        for document in documents {
            let id = document["_id"].as_str().unwrap();
            let response = couchdb.put_document(id, &document).send().await?;
            if !response.status().is_success() && response.status() != reqwest::StatusCode::CONFLICT {
                return Err(format!("No se pudo preparar {id}: HTTP {}. Verifica la base configurada.", response.status()).into());
            }
        }
        println!("Catálogo ficticio preparado. Los IDs existentes se conservaron sin cambios.");
        return Ok(());
    }
    let listener = tokio::net::TcpListener::bind(config.api_addr).await?;
    println!(
        "API BAD 215 disponible en http://{}",
        listener.local_addr()?
    );
    axum::serve(
        listener,
        routes::router(couchdb, config.comision_porcentaje, auth),
    )
    .await?;
    Ok(())
}
