use std::future::Future;

use chrono::{DateTime, Utc};

use crate::domain::entities::appointment::Appointment;

pub enum RepositoryReadError {
    Unavailable,
    InvalidData,
}

pub trait AppointmentRepository {
    fn find_by_id(
        &self,
        id: &str,
    ) -> impl Future<Output = Result<Option<Appointment>, RepositoryReadError>> + Send;

    fn list_starting_between(
        &self,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> impl Future<Output = Result<Vec<Appointment>, RepositoryReadError>> + Send;
}
