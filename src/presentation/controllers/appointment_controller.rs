use axum::{
    Json,
    extract::{
        Path, Query, State,
        rejection::{JsonRejection, PathRejection, QueryRejection},
    },
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};

use crate::{
    application::services::appointment_service::{
        CreateAppointmentError, ListAppointmentsError, UpdateAppointmentError, create_appointment,
        get_appointment, list_appointments, update_appointment_status,
    },
    domain::{
        entities::appointment::{Appointment, AppointmentStatusChange, NewAppointment},
        repositories::{AppointmentUpdateError, RepositoryReadError},
    },
    infrastructure::{
        database::couchdb::CouchDb,
        repositories::{
            couchdb_appointment_repository::CouchDbAppointmentRepository,
            couchdb_catalog_repository::CouchDbCatalogRepository,
        },
    },
    shared::errors::error_response,
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
        return invalid_id();
    };

    let repository = CouchDbAppointmentRepository(&couchdb);
    match get_appointment(&repository, &id).await {
        Ok(Some(appointment)) => Json(appointment).into_response(),
        Ok(None) => not_found(),
        Err(error) => repository_error(error),
    }
}

fn repository_error(error: RepositoryReadError) -> Response {
    match error {
        RepositoryReadError::Unavailable => error_response(
            StatusCode::SERVICE_UNAVAILABLE,
            "COUCHDB_NO_DISPONIBLE",
            "La API no puede acceder a la base de datos.",
        ),
        RepositoryReadError::InvalidData => error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "CITAS_NO_DISPONIBLES",
            "La API no pudo leer los datos de las citas.",
        ),
    }
}

pub async fn create(
    State(couchdb): State<CouchDb>,
    body: Result<Json<NewAppointment>, JsonRejection>,
) -> Response {
    let request = match body {
        Ok(Json(request)) => request,
        Err(error) => return json_error(error),
    };

    let repository = CouchDbAppointmentRepository(&couchdb);
    let catalog = CouchDbCatalogRepository(&couchdb);
    let (status, code, message) = match create_appointment(&repository, &catalog, request).await {
        Ok(appointment) => return (StatusCode::CREATED, Json(appointment)).into_response(),
        Err(CreateAppointmentError::Repository(error)) => return repository_error(error),
        Err(CreateAppointmentError::InvalidFields) => (
            StatusCode::UNPROCESSABLE_ENTITY,
            "CITA_INVALIDA",
            "Envía un UUID v4 para la cita y valores no vacíos para cliente, empleado y servicio.",
        ),
        Err(CreateAppointmentError::InvalidTrace) => return invalid_trace(),
        Err(CreateAppointmentError::EmployeeUnavailable) => (
            StatusCode::UNPROCESSABLE_ENTITY,
            "EMPLEADO_NO_DISPONIBLE",
            "El empleado indicado debe existir y estar activo.",
        ),
        Err(CreateAppointmentError::ServiceUnavailable) => (
            StatusCode::UNPROCESSABLE_ENTITY,
            "SERVICIO_NO_DISPONIBLE",
            "El servicio indicado debe existir y estar activo.",
        ),
        Err(CreateAppointmentError::Conflict) => (
            StatusCode::CONFLICT,
            "CITA_EN_CONFLICTO",
            "El identificador de cita ya está en uso.",
        ),
    };
    error_response(status, code, message)
}

pub async fn update_status(
    State(couchdb): State<CouchDb>,
    path: Result<Path<String>, PathRejection>,
    body: Result<Json<AppointmentStatusChange>, JsonRejection>,
) -> Response {
    let Ok(Path(id)) = path else {
        return invalid_id();
    };
    let request = match body {
        Ok(Json(request)) => request,
        Err(error) => return json_error(error),
    };

    let repository = CouchDbAppointmentRepository(&couchdb);
    match update_appointment_status(&repository, &id, request).await {
        Ok(appointment) => Json(appointment).into_response(),
        Err(UpdateAppointmentError::InvalidTrace) => invalid_trace(),
        Err(UpdateAppointmentError::Repository(error)) => match error {
            AppointmentUpdateError::NotFound => not_found(),
            AppointmentUpdateError::Read(error) => repository_error(error),
            AppointmentUpdateError::InvalidTransition => error_response(
                StatusCode::UNPROCESSABLE_ENTITY,
                "TRANSICION_ESTADO_INVALIDA",
                "Una cita pendiente puede pasar a completada o cancelada.",
            ),
            AppointmentUpdateError::Conflict => error_response(
                StatusCode::CONFLICT,
                "CITA_EN_CONFLICTO",
                "La cita cambió durante la escritura o el op_id ya está en uso.",
            ),
        },
    }
}

fn not_found() -> Response {
    error_response(
        StatusCode::NOT_FOUND,
        "CITA_NO_ENCONTRADA",
        "No se encontró la cita solicitada",
    )
}

fn invalid_id() -> Response {
    error_response(
        StatusCode::BAD_REQUEST,
        "CITA_ID_INVALIDO",
        "Envía un identificador de cita válido.",
    )
}

fn invalid_trace() -> Response {
    error_response(
        StatusCode::UNPROCESSABLE_ENTITY,
        "TRAZABILIDAD_INVALIDA",
        "Envía juntos op_id (UUID v4), ts_cliente, vu (positivo) e iteracion.",
    )
}

fn json_error(error: JsonRejection) -> Response {
    let status = error.status();
    let (code, message) = match status {
        StatusCode::UNSUPPORTED_MEDIA_TYPE => (
            "TIPO_CONTENIDO_INVALIDO",
            "Envía la cita con Content-Type: application/json.",
        ),
        StatusCode::UNPROCESSABLE_ENTITY => (
            "CITA_INVALIDA",
            "Revisa los campos y tipos de datos de la cita.",
        ),
        StatusCode::PAYLOAD_TOO_LARGE => (
            "SOLICITUD_DEMASIADO_GRANDE",
            "El cuerpo de la solicitud excede el tamaño permitido.",
        ),
        _ => ("JSON_INVALIDO", "Envía un cuerpo JSON válido."),
    };
    error_response(status, code, message)
}
