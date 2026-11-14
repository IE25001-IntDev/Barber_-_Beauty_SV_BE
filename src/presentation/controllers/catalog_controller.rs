use axum::{
    Json,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Serialize;

use crate::{
    domain::{entities::catalog::CatalogEntry, repositories::RepositoryReadError},
    infrastructure::{
        database::couchdb::CouchDb,
        repositories::couchdb_catalog_repository::CouchDbCatalogRepository,
    },
    shared::errors::error_response,
};

#[derive(Serialize)]
struct CatalogList<T> {
    items: Vec<CatalogEntry<T>>,
}

pub async fn list_employees(State(couchdb): State<CouchDb>) -> Response {
    catalog_response(CouchDbCatalogRepository(&couchdb).list_employees().await)
}

pub async fn list_services(State(couchdb): State<CouchDb>) -> Response {
    catalog_response(CouchDbCatalogRepository(&couchdb).list_services().await)
}

fn catalog_response<T: Serialize>(
    result: Result<Vec<CatalogEntry<T>>, RepositoryReadError>,
) -> Response {
    match result {
        Ok(items) => Json(CatalogList { items }).into_response(),
        Err(RepositoryReadError::Unavailable) => error_response(
            StatusCode::SERVICE_UNAVAILABLE,
            "COUCHDB_NO_DISPONIBLE",
            "La API no puede acceder a la base de datos.",
        ),
        Err(RepositoryReadError::InvalidData) => error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "CATALOGO_NO_DISPONIBLE",
            "La API no pudo leer los datos del catálogo.",
        ),
    }
}
