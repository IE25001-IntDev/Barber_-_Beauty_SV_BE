use chrono::{DateTime, Datelike, Duration, Utc};
use reqwest::StatusCode;
use serde::Deserialize;
use serde_json::json;

use crate::{
    domain::{
        entities::appointment::{Appointment, AppointmentStatus, CreationTrace, NewAppointment},
        repositories::{AppointmentInsertError, AppointmentRepository, RepositoryReadError},
    },
    infrastructure::database::couchdb::CouchDb,
};

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
    #[serde(default)]
    creacion: Option<CreationTrace>,
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

impl CouchDbAppointmentRepository<'_> {
    async fn find_document(
        &self,
        id: &str,
    ) -> Result<Option<AppointmentDocument>, RepositoryReadError> {
        let query = json!({
            "selector": {
                "_id": {"$eq": id},
                "cliente_nombre": {"$exists": true},
                "servicio_id": {"$exists": true}
            },
            "limit": 1
        });

        Ok(self
            .0
            .find_page::<AppointmentDocument>(&query)
            .await?
            .docs
            .pop())
    }
}

impl AppointmentRepository for CouchDbAppointmentRepository<'_> {
    async fn find_by_id(&self, id: &str) -> Result<Option<Appointment>, RepositoryReadError> {
        Ok(self.find_document(id).await?.map(Appointment::from))
    }

    async fn find_creation(
        &self,
        request: &NewAppointment,
        trace: &CreationTrace,
    ) -> Result<Option<Appointment>, AppointmentInsertError> {
        let Some(document) = self
            .find_document(&request.id)
            .await
            .map_err(AppointmentInsertError::Read)?
        else {
            return Ok(None);
        };
        if document.creacion.as_ref() == Some(trace)
            && document.cliente_nombre == request.cliente_nombre
            && document.empleado_id == request.empleado_id
            && document.servicio_id == request.servicio_id
            && document.inicio == request.inicio
        {
            return Ok(Some(document.into()));
        }
        Err(AppointmentInsertError::Conflict)
    }

    async fn insert(
        &self,
        appointment: Appointment,
        trace: Option<&CreationTrace>,
    ) -> Result<Appointment, AppointmentInsertError> {
        let mut document = json!({
            "tipo": "cita",
            "cliente_nombre": appointment.cliente_nombre,
            "empleado_id": appointment.empleado_id,
            "servicio_id": appointment.servicio_id,
            "inicio": appointment.inicio,
            "estado": appointment.estado,
            "precio_centavos": appointment.precio_centavos
        });
        if let Some(trace) = trace {
            document["creacion"] = json!(trace);
        }

        let response = self
            .0
            .put_document(&appointment.id, &document)
            .send()
            .await
            .map_err(|_| AppointmentInsertError::Read(RepositoryReadError::Unavailable))?;

        match response.status() {
            StatusCode::CREATED | StatusCode::ACCEPTED => Ok(appointment),
            StatusCode::CONFLICT => Err(AppointmentInsertError::Conflict),
            _ => Err(AppointmentInsertError::Read(
                RepositoryReadError::Unavailable,
            )),
        }
    }

    async fn list_starting_between(
        &self,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<Vec<Appointment>, RepositoryReadError> {
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

        let mut query = CouchDb::paged_query(json!({
            "inicio": range,
            "cliente_nombre": {"$exists": true},
            "servicio_id": {"$exists": true}
        }));

        let mut appointments: Vec<Appointment> = Vec::new();
        loop {
            let page = self.0.find_page::<AppointmentDocument>(&query).await?;
            let has_more = page.advance(&mut query)?;
            appointments.extend(
                page.docs
                    .into_iter()
                    .filter(|doc| doc.inicio >= start && doc.inicio < end)
                    .map(Appointment::from),
            );

            if !has_more {
                break;
            }
        }

        appointments.sort_unstable_by(|a, b| a.inicio.cmp(&b.inicio).then_with(|| a.id.cmp(&b.id)));
        Ok(appointments)
    }
}
