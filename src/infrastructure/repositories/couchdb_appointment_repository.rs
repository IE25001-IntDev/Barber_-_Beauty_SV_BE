use chrono::{DateTime, Datelike, Duration, Utc};
use serde::Deserialize;
use serde_json::json;

use crate::{
    domain::{
        entities::appointment::{Appointment, AppointmentStatus},
        repositories::{AppointmentReadError, AppointmentRepository},
    },
    infrastructure::database::couchdb::CouchDb,
};

const PAGE_SIZE: usize = 100;

pub struct CouchDbAppointmentRepository<'a>(pub &'a CouchDb);

#[derive(Deserialize)]
struct AppointmentDocument {
    #[serde(rename = "_id")]
    id: String,
    cliente_nombre: String,
    empleado_id: String,
    servicio_id: String,
    inicio: DateTime<Utc>,
    estado: AppointmentStatus,
    precio_centavos: u64,
}

impl From<AppointmentDocument> for Appointment {
    fn from(document: AppointmentDocument) -> Self {
        Self {
            id: document.id,
            cliente_nombre: document.cliente_nombre,
            empleado_id: document.empleado_id,
            servicio_id: document.servicio_id,
            inicio: document.inicio,
            estado: document.estado,
            precio_centavos: document.precio_centavos,
        }
    }
}

#[derive(Deserialize)]
struct FindPage {
    docs: Vec<AppointmentDocument>,
    bookmark: Option<String>,
}

impl AppointmentRepository for CouchDbAppointmentRepository<'_> {
    async fn find_by_id(&self, id: &str) -> Result<Option<Appointment>, AppointmentReadError> {
        let query = json!({
            "selector": {
                "_id": {"$eq": id},
                "cliente_nombre": {"$exists": true},
                "servicio_id": {"$exists": true}
            },
            "limit": 1
        });

        Ok(self
            .find_page(&query)
            .await?
            .docs
            .pop()
            .map(Appointment::from))
    }

    async fn list_starting_between(
        &self,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<Vec<Appointment>, AppointmentReadError> {
        // Preselecciona por fecha escrita en el documento, incluso si usa
        // un desplazamiento RFC 3339. El filtro final compara instantes.
        let first_date = (start - Duration::days(1)).date_naive();
        let after_last_date = (end + Duration::days(2)).date_naive();
        let mut range = json!({"$type": "string"});
        if first_date.year() >= 0 {
            range["$gte"] = json!(first_date.format("%Y-%m-%d").to_string());
        }
        if after_last_date.year() <= 9999 {
            range["$lt"] = json!(after_last_date.format("%Y-%m-%d").to_string());
        }

        let mut query = json!({
            "selector": {
                "inicio": range,
                "cliente_nombre": {"$exists": true},
                "servicio_id": {"$exists": true}
            },
            "limit": PAGE_SIZE
        });

        let mut appointments: Vec<Appointment> = Vec::new();
        loop {
            let page = self.find_page(&query).await?;
            let count = page.docs.len();
            appointments.extend(
                page.docs
                    .into_iter()
                    .filter(|doc| doc.inicio >= start && doc.inicio < end)
                    .map(Appointment::from),
            );

            if count < PAGE_SIZE {
                break;
            }
            let bookmark = page
                .bookmark
                .filter(|bookmark| {
                    !bookmark.is_empty() && query["bookmark"].as_str() != Some(bookmark)
                })
                .ok_or(AppointmentReadError::InvalidData)?;
            query["bookmark"] = json!(bookmark);
        }

        appointments.sort_unstable_by(|a, b| a.inicio.cmp(&b.inicio).then_with(|| a.id.cmp(&b.id)));
        Ok(appointments)
    }
}

impl CouchDbAppointmentRepository<'_> {
    async fn find_page(&self, query: &serde_json::Value) -> Result<FindPage, AppointmentReadError> {
        let response = self
            .0
            .find(query)
            .send()
            .await
            .map_err(|_| AppointmentReadError::Unavailable)?;
        if !response.status().is_success() {
            return Err(AppointmentReadError::Unavailable);
        }

        response
            .json()
            .await
            .map_err(|_| AppointmentReadError::InvalidData)
    }
}
