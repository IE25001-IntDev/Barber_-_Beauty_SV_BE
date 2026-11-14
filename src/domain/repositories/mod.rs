use std::future::Future;

use chrono::{DateTime, Utc};

use crate::domain::entities::appointment::Appointment;

pub enum AppointmentReadError {
    Unavailable,
    InvalidData,
}

pub trait AppointmentRepository {
    fn list_starting_between(
        &self,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> impl Future<Output = Result<Vec<Appointment>, AppointmentReadError>> + Send;
}
