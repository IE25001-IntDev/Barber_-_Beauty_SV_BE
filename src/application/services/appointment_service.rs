use chrono::{NaiveDate, TimeZone, Utc};
use chrono_tz::America::El_Salvador;

use crate::domain::{
    entities::appointment::{
        Appointment, AppointmentStatus, AppointmentStatusChange, NewAppointment, OperationTrace,
    },
    repositories::{
        AppointmentCatalog, AppointmentInsertError, AppointmentRepository, AppointmentUpdateError,
        RepositoryReadError,
    },
};

pub enum ListAppointmentsError {
    InvalidDate,
    Repository(RepositoryReadError),
}

pub async fn get_appointment(
    repository: &impl AppointmentRepository,
    id: &str,
) -> Result<Option<Appointment>, RepositoryReadError> {
    repository.find_by_id(id).await
}

pub async fn list_appointments(
    repository: &impl AppointmentRepository,
    date: &str,
) -> Result<Vec<Appointment>, ListAppointmentsError> {
    let invalid_date = ListAppointmentsError::InvalidDate;
    let date = NaiveDate::parse_from_str(date, "%Y-%m-%d")
        .ok()
        .filter(|parsed| date.len() == 10 && parsed.format("%Y-%m-%d").to_string() == date)
        .ok_or(invalid_date)?;

    let start = start_of_day(date).ok_or(ListAppointmentsError::InvalidDate)?;
    let end = date
        .succ_opt()
        .and_then(start_of_day)
        .ok_or(ListAppointmentsError::InvalidDate)?;

    repository
        .list_starting_between(start, end)
        .await
        .map_err(ListAppointmentsError::Repository)
}

fn start_of_day(day: NaiveDate) -> Option<chrono::DateTime<Utc>> {
    let midnight = day.and_hms_opt(0, 0, 0)?;
    El_Salvador
        .from_local_datetime(&midnight)
        .earliest()
        // En 1987 y 1988, el cambio de hora saltó de medianoche a las 01:00.
        .or_else(|| {
            let one_am = day.and_hms_opt(1, 0, 0)?;
            El_Salvador.from_local_datetime(&one_am).earliest()
        })
        .map(|instant| instant.with_timezone(&Utc))
}

pub enum CreateAppointmentError {
    InvalidFields,
    InvalidTrace,
    EmployeeUnavailable,
    ServiceUnavailable,
    Conflict,
    Repository(RepositoryReadError),
}

pub async fn create_appointment(
    repository: &impl AppointmentRepository,
    catalog: &impl AppointmentCatalog,
    mut request: NewAppointment,
) -> Result<Appointment, CreateAppointmentError> {
    request.id = uuid_v4(&request.id).ok_or(CreateAppointmentError::InvalidFields)?;
    if request.cliente_nombre.trim().is_empty()
        || request.empleado_id.trim().is_empty()
        || request.servicio_id.trim().is_empty()
    {
        return Err(CreateAppointmentError::InvalidFields);
    }

    let trace = operation_trace(
        request.op_id.as_deref(),
        request.ts_cliente,
        request.vu,
        request.iteracion,
    )
    .map_err(|()| CreateAppointmentError::InvalidTrace)?;

    if let Some(trace) = &trace
        && let Some(appointment) = repository
            .find_creation(&request, trace)
            .await
            .map_err(insert_error)?
    {
        return Ok(appointment);
    }

    catalog
        .find_employee(&request.empleado_id)
        .await
        .map_err(CreateAppointmentError::Repository)?
        .filter(|employee| employee.activo)
        .ok_or(CreateAppointmentError::EmployeeUnavailable)?;
    let service = catalog
        .find_service(&request.servicio_id)
        .await
        .map_err(CreateAppointmentError::Repository)?
        .filter(|service| service.activo)
        .ok_or(CreateAppointmentError::ServiceUnavailable)?;

    let appointment = Appointment {
        id: request.id.clone(),
        cliente_nombre: request.cliente_nombre.clone(),
        empleado_id: request.empleado_id.clone(),
        servicio_id: request.servicio_id.clone(),
        inicio: request.inicio,
        estado: AppointmentStatus::Pendiente,
        precio_centavos: service.precio_centavos,
    };

    match (repository.insert(appointment, trace.as_ref()).await, trace) {
        (Err(AppointmentInsertError::Conflict), Some(trace)) => repository
            .find_creation(&request, &trace)
            .await
            .map_err(insert_error)?
            .ok_or(CreateAppointmentError::Conflict),
        (result, _) => result.map_err(insert_error),
    }
}

fn insert_error(error: AppointmentInsertError) -> CreateAppointmentError {
    match error {
        AppointmentInsertError::Conflict => CreateAppointmentError::Conflict,
        AppointmentInsertError::Read(error) => CreateAppointmentError::Repository(error),
    }
}

fn uuid_v4(value: &str) -> Option<String> {
    let parsed = uuid::Uuid::parse_str(value).ok()?;
    let canonical = parsed.to_string();
    (parsed.get_version() == Some(uuid::Version::Random)
        && parsed.get_variant() == uuid::Variant::RFC4122
        && canonical.eq_ignore_ascii_case(value))
    .then_some(canonical)
}

pub enum UpdateAppointmentError {
    InvalidTrace,
    Repository(AppointmentUpdateError),
}

pub async fn update_appointment_status(
    repository: &impl AppointmentRepository,
    id: &str,
    request: AppointmentStatusChange,
) -> Result<Appointment, UpdateAppointmentError> {
    let trace = operation_trace(
        request.op_id.as_deref(),
        request.ts_cliente,
        request.vu,
        request.iteracion,
    )
    .map_err(|()| UpdateAppointmentError::InvalidTrace)?;

    repository
        .update_status(id, request.estado, trace.as_ref())
        .await
        .map_err(UpdateAppointmentError::Repository)
}

fn operation_trace(
    op_id: Option<&str>,
    ts_cliente: Option<u64>,
    vu: Option<u64>,
    iteracion: Option<u64>,
) -> Result<Option<OperationTrace>, ()> {
    match (op_id, ts_cliente, vu, iteracion) {
        (None, None, None, None) => Ok(None),
        (Some(op_id), Some(ts_cliente), Some(vu), Some(iteracion)) if vu > 0 => {
            Ok(Some(OperationTrace {
                op_id: uuid_v4(op_id).ok_or(())?,
                ts_cliente,
                vu,
                iteracion,
            }))
        }
        _ => Err(()),
    }
}
