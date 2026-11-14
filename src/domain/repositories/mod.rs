use std::future::Future;

use chrono::{DateTime, Utc};

use crate::domain::entities::{
    appointment::{
        Appointment, AppointmentStatus, NewAppointment, OperationTrace, ScheduledAppointment,
    },
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

pub enum AppointmentUpdateError {
    NotFound,
    InvalidTransition,
    Conflict,
    Read(RepositoryReadError),
}

pub trait AppointmentRepository {
    fn update_status(
        &self,
        id: &str,
        status: AppointmentStatus,
        trace: Option<&OperationTrace>,
    ) -> impl Future<Output = Result<Appointment, AppointmentUpdateError>> + Send;

    fn find_creation(
        &self,
        request: &NewAppointment,
        trace: &OperationTrace,
    ) -> impl Future<Output = Result<Option<Appointment>, AppointmentInsertError>> + Send;

    fn insert(
        &self,
        appointment: Appointment,
        duration_minutes: u32,
        trace: Option<&OperationTrace>,
    ) -> impl Future<Output = Result<Appointment, AppointmentInsertError>> + Send;

    fn list_employee_schedule(
        &self,
        employee_id: &str,
    ) -> impl Future<Output = Result<Vec<ScheduledAppointment>, RepositoryReadError>> + Send;

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
