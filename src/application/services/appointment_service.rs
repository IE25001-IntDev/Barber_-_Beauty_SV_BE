use chrono::{NaiveDate, TimeZone, Utc};
use chrono_tz::America::El_Salvador;

use crate::domain::{
    entities::appointment::Appointment,
    repositories::{AppointmentRepository, RepositoryReadError},
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
