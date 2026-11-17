use crate::{
    infrastructure::database::couchdb::CouchDb,
    presentation::auth::{Role, User, forbidden},
    shared::errors::error_response,
};
use axum::{
    Extension, Json,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

#[derive(Clone, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Measurement {
    id: String,
    fecha: String,
    fase: String,
    minutos_disponibles: u32,
    minutos_productivos: u32,
    segundos_planilla: u32,
}
impl Measurement {
    fn valid(&self) -> bool {
        Uuid::parse_str(&self.id).is_ok_and(|id| id.get_version_num() == 4)
            && chrono::NaiveDate::parse_from_str(&self.fecha, "%Y-%m-%d")
                .is_ok_and(|date| date.format("%Y-%m-%d").to_string() == self.fecha)
            && ["antes", "despues"].contains(&self.fase.as_str())
            && self.minutos_disponibles > 0
            && self.minutos_disponibles <= 144_000
            && self.minutos_productivos <= self.minutos_disponibles
            && self.segundos_planilla <= 86_400
    }
}
#[derive(Deserialize)]
struct Document {
    medicion: Measurement,
}
pub async fn list(State(db): State<CouchDb>, Extension(user): Extension<User>) -> Response {
    if user.rol != Role::Administrador {
        return forbidden();
    }
    let mut query = CouchDb::paged_query(json!({"tipo":"medicion"}));
    let mut items = Vec::new();
    loop {
        let Ok(page) = db.find_page::<Document>(&query).await else {
            return unavailable();
        };
        let Ok(more) = page.advance(&mut query) else {
            return unavailable();
        };
        items.extend(page.docs.into_iter().map(|d| d.medicion));
        if !more {
            break;
        }
    }
    Json(json!({"items":items})).into_response()
}
pub async fn create(
    State(db): State<CouchDb>,
    Extension(user): Extension<User>,
    Json(value): Json<Measurement>,
) -> Response {
    if user.rol != Role::Administrador {
        return forbidden();
    }
    if !value.valid() {
        return error_response(
            StatusCode::UNPROCESSABLE_ENTITY,
            "MEDICION_INVALIDA",
            "Revisa la fecha, la fase y los tiempos. El tiempo productivo no puede superar el disponible.",
        );
    }
    let id = format!("medicion:{}", value.id);
    let document = json!({"tipo":"medicion", "medicion":value, "registrado_por":user.usuario});
    let Ok(response) = db.put_document(&id, &document).send().await else {
        return unavailable();
    };
    if response.status().is_success() {
        return (StatusCode::CREATED, Json(value)).into_response();
    }
    if response.status() == reqwest::StatusCode::CONFLICT {
        if let Ok(response) = db.get_document(&id).send().await {
            if let Ok(existing) = response.json::<Document>().await {
                if existing.medicion == value {
                    return Json(value).into_response();
                }
            }
        }
        return error_response(
            StatusCode::CONFLICT,
            "MEDICION_EN_CONFLICTO",
            "Este identificador ya pertenece a otra medición.",
        );
    }
    unavailable()
}
fn unavailable() -> Response {
    error_response(
        StatusCode::SERVICE_UNAVAILABLE,
        "MEDICIONES_NO_DISPONIBLES",
        "No se pudo confirmar la medición. Conserva el formulario y reintenta sin cambiar sus datos.",
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn measurement_requires_realistic_complete_values() {
        let mut m = Measurement {
            id: Uuid::new_v4().to_string(),
            fecha: "2026-11-17".into(),
            fase: "antes".into(),
            minutos_disponibles: 480,
            minutos_productivos: 300,
            segundos_planilla: 60,
        };
        assert!(m.valid());
        m.minutos_productivos = 481;
        assert!(!m.valid());
        m.minutos_productivos = 0;
        m.minutos_disponibles = 0;
        assert!(!m.valid());
        m.minutos_disponibles = 480;
        m.fecha = "2026-02-30".into();
        assert!(!m.valid());
    }
}
