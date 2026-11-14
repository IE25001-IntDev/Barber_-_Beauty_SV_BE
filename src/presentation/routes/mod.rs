use axum::{Json, Router, extract::State, http::StatusCode, routing::get};
use serde_json::{Value, json};

use crate::{
    infrastructure::database::couchdb::CouchDb,
    presentation::controllers::{appointment_controller, catalog_controller},
};

pub fn router(couchdb: CouchDb) -> Router {
    Router::new()
        .route("/api/health", get(health))
        .route("/api/ready", get(ready))
        .route(
            "/api/citas",
            get(appointment_controller::list).post(appointment_controller::create),
        )
        .route("/api/citas/{id}", get(appointment_controller::get_by_id))
        .route("/api/empleados", get(catalog_controller::list_employees))
        .route("/api/servicios", get(catalog_controller::list_services))
        .with_state(couchdb)
}

async fn health() -> Json<Value> {
    Json(json!({ "status": "ok" }))
}

async fn ready(State(couchdb): State<CouchDb>) -> (StatusCode, Json<Value>) {
    if couchdb.database_available().await {
        return (StatusCode::OK, Json(json!({ "status": "ok" })));
    }

    (
        StatusCode::SERVICE_UNAVAILABLE,
        Json(json!({
            "error": {
                "codigo": "COUCHDB_NO_DISPONIBLE",
                "mensaje": "La API no puede acceder a la base de datos."
            }
        })),
    )
}
