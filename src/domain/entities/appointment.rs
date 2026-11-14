use chrono::{DateTime, Utc};
use serde::Serialize;

#[derive(Serialize)]
pub struct Appointment {
    pub id: String,
    pub cliente_nombre: String,
    pub empleado_id: String,
    pub servicio_id: String,
    pub inicio: DateTime<Utc>,
    pub estado: AppointmentStatus,
    pub precio_centavos: u64,
}

#[derive(Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AppointmentStatus {
    Pendiente,
    Completada,
    Cancelada,
}
