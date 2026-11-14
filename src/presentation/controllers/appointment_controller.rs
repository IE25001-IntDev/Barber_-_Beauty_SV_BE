use axum::{
    Json,
    extract::{Query, State, rejection::QueryRejection},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::{
    application::services::appointment_service::{ListAppointmentsError, list_appointments},
    domain::{entities::appointment::Appointment, repositories::AppointmentReadError},
    infrastructure::{
        database::couchdb::CouchDb,
        repositories::couchdb_appointment_repository::CouchDbAppointmentRepository,
    },
};

#[derive(Deserialize)]
pub struct ListAppointmentsQuery {
    fecha: String,
}

#[derive(Serialize)]
struct AppointmentList {
    items: Vec<Appointment>,
}

pub async fn list(
    State(couchdb): State<CouchDb>,
    query: Result<Query<ListAppointmentsQuery>, QueryRejection>,
) -> Response {
    let repository = CouchDbAppointmentRepository(&couchdb);
    let result = match query {
        Ok(Query(query)) => list_appointments(&repository, &query.fecha).await,
        Err(_) => Err(ListAppointmentsError::InvalidDate),
    };

    let (status, code, message) = match result {
        Ok(items) => return Json(AppointmentList { items }).into_response(),
        Err(ListAppointmentsError::InvalidDate) => (
            StatusCode::BAD_REQUEST,
            "FECHA_INVALIDA",
            "Envía una fecha válida en formato YYYY-MM-DD.",
        ),
        Err(ListAppointmentsError::Repository(AppointmentReadError::Unavailable)) => (
            StatusCode::SERVICE_UNAVAILABLE,
            "COUCHDB_NO_DISPONIBLE",
            "La API no puede acceder a la base de datos.",
        ),
        Err(ListAppointmentsError::Repository(AppointmentReadError::InvalidData)) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            "CITAS_NO_DISPONIBLES",
            "La API no pudo leer los datos de las citas.",
        ),
    };

    (
        status,
        Json(json!({"error": {"codigo": code, "mensaje": message}})),
    )
        .into_response()
}
