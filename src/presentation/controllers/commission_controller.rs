use crate::{
    application::services::commission_service::date_range,
    domain::repositories::{CommissionRepository, RepositoryReadError},
    infrastructure::{
        database::couchdb::CouchDb,
        repositories::couchdb_commission_repository::CouchDbCommissionRepository,
    },
    shared::errors::error_response,
};
use axum::{
    Json,
    extract::{Query, State, rejection::QueryRejection},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use serde_json::json;

#[derive(Deserialize)]
pub struct ListQuery {
    desde: String,
    hasta: String,
}

pub async fn list(
    State(db): State<CouchDb>,
    query: Result<Query<ListQuery>, QueryRejection>,
) -> Response {
    let range = query
        .ok()
        .and_then(|Query(q)| date_range(&q.desde, &q.hasta));
    let Some((start, end)) = range else {
        return error_response(
            StatusCode::BAD_REQUEST,
            "FECHA_INVALIDA",
            "Envía desde y hasta en formato YYYY-MM-DD, con desde menor o igual a hasta.",
        );
    };
    match CouchDbCommissionRepository(&db)
        .list_generated_between(start, end)
        .await
    {
        Ok(items) => Json(json!({"items": items})).into_response(),
        Err(error) => repository_error(error),
    }
}

pub(super) fn repository_error(error: RepositoryReadError) -> Response {
    match error {
        RepositoryReadError::Unavailable => error_response(
            StatusCode::SERVICE_UNAVAILABLE,
            "COUCHDB_NO_DISPONIBLE",
            "No se pudo asegurar o consultar la comisión. Si la cita quedó completada, reintenta el PUT con estado completada.",
        ),
        RepositoryReadError::InvalidData => error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "COMISIONES_NO_DISPONIBLES",
            "La API no pudo leer los datos de las comisiones.",
        ),
    }
}
