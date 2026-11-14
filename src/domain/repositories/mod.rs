use std::future::Future;

use chrono::{DateTime, Utc};

use crate::domain::entities::{
    appointment::{Appointment, CreationTrace, NewAppointment},
    barber::Barber,
    service::Service,
};

pub enum RepositoryReadError {
    Unavailable,
    InvalidData,
}

pub enum AppointmentInsertError {
    Conflict,
    Read(RepositoryReadError),
}

pub trait AppointmentRepository {
    fn find_creation(
        &self,
        request: &NewAppointment,
        trace: &CreationTrace,
    ) -> impl Future<Output = Result<Option<Appointment>, AppointmentInsertError>> + Send;

    fn insert(
        &self,
        appointment: Appointment,
        trace: Option<&CreationTrace>,
    ) -> impl Future<Output = Result<Appointment, AppointmentInsertError>> + Send;

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

pub trait AppointmentCatalog {
    fn find_employee(
        &self,
        id: &str,
    ) -> impl Future<Output = Result<Option<Barber>, RepositoryReadError>> + Send;

    fn find_service(
        &self,
        id: &str,
    ) -> impl Future<Output = Result<Option<Service>, RepositoryReadError>> + Send;
}
