use std::sync::Arc;

use axum::{Extension, Json, Router, extract::State, http::StatusCode, routing::get};
use serde_json::{Value, json};
use tokio::sync::Mutex;

use crate::{
    infrastructure::database::couchdb::CouchDb,
    presentation::controllers::{
        appointment_controller, catalog_controller, commission_controller, measurement_controller,
    },
};

pub fn router(couchdb: CouchDb, percentage: u8, auth: Arc<crate::presentation::auth::Auth>) -> Router {
    use crate::presentation::auth;
    Router::new()
        .route(
            "/api/citas",
            get(appointment_controller::list).post(appointment_controller::create),
        )
        .route(
            "/api/citas/{id}",
            get(appointment_controller::get_by_id).put(appointment_controller::update_status),
        )
        .route("/api/comisiones", get(commission_controller::list))
        .route("/api/mediciones", get(measurement_controller::list).post(measurement_controller::create))
        .layer(Extension(percentage))
        .route("/api/empleados", get(catalog_controller::list_employees))
        .route("/api/servicios", get(catalog_controller::list_services))
        .layer(Extension(Arc::new(Mutex::new(()))))
        .route_layer(axum::middleware::from_fn(auth::protect))
        .route("/api/auth/login", axum::routing::post(auth::login))
        .route("/api/auth/logout", axum::routing::post(auth::logout))
        .route("/api/health", get(health))
        .route("/api/ready", get(ready))
        .layer(Extension(auth))
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
