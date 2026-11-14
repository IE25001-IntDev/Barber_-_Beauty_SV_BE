use axum::{
    Json,
    extract::{
        Path, Query, State,
        rejection::{PathRejection, QueryRejection},
    },
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::{
    application::services::appointment_service::{
        ListAppointmentsError, get_appointment, list_appointments,
    },
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

    match result {
        Ok(items) => Json(AppointmentList { items }).into_response(),
        Err(ListAppointmentsError::InvalidDate) => error_response(
            StatusCode::BAD_REQUEST,
            "FECHA_INVALIDA",
            "Envía una fecha válida en formato YYYY-MM-DD.",
        ),
        Err(ListAppointmentsError::Repository(error)) => repository_error(error),
    }
}

pub async fn get_by_id(
    State(couchdb): State<CouchDb>,
    path: Result<Path<String>, PathRejection>,
) -> Response {
    let Ok(Path(id)) = path else {
        return error_response(
            StatusCode::BAD_REQUEST,
            "CITA_ID_INVALIDO",
            "Envía un identificador de cita válido.",
        );
    };

    let repository = CouchDbAppointmentRepository(&couchdb);
    match get_appointment(&repository, &id).await {
        Ok(Some(appointment)) => Json(appointment).into_response(),
        Ok(None) => error_response(
            StatusCode::NOT_FOUND,
            "CITA_NO_ENCONTRADA",
            "No se encontró la cita solicitada",
        ),
        Err(error) => repository_error(error),
    }
}

fn repository_error(error: AppointmentReadError) -> Response {
    match error {
        AppointmentReadError::Unavailable => error_response(
            StatusCode::SERVICE_UNAVAILABLE,
            "COUCHDB_NO_DISPONIBLE",
            "La API no puede acceder a la base de datos.",
        ),
        AppointmentReadError::InvalidData => error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "CITAS_NO_DISPONIBLES",
            "La API no pudo leer los datos de las citas.",
        ),
    }
}

fn error_response(status: StatusCode, code: &str, message: &str) -> Response {
    (
        status,
        Json(json!({"error": {"codigo": code, "mensaje": message}})),
    )
        .into_response()
}
