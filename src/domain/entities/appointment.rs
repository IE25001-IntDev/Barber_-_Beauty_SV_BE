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

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NewAppointment {
    pub id: String,
    pub cliente_nombre: String,
    pub empleado_id: String,
    pub servicio_id: String,
    pub inicio: DateTime<Utc>,
    pub op_id: Option<String>,
    pub ts_cliente: Option<u64>,
    pub vu: Option<u64>,
    pub iteracion: Option<u64>,
}

#[derive(Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct CreationTrace {
    pub op_id: String,
    pub ts_cliente: u64,
    pub vu: u64,
    pub iteracion: u64,
}
