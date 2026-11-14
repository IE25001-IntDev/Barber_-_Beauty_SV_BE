use super::appointment_service::start_of_day;
use crate::domain::{
    entities::{
        appointment::Appointment,
        commission::{Commission, amount},
    },
    repositories::{CommissionRepository, RepositoryReadError},
};
use chrono::{DateTime, NaiveDate, Utc};

pub async fn ensure_commission(
    repository: &impl CommissionRepository,
    appointment: &Appointment,
    percentage: u8,
) -> Result<Commission, RepositoryReadError> {
    let id = format!("comision:{}", appointment.id);
    if let Some(existing) = repository.find_commission(&id).await? {
        return Ok(existing);
    }
    repository
        .insert_commission(Commission {
            id,
            cita_id: appointment.id.clone(),
            empleado_id: appointment.empleado_id.clone(),
            porcentaje: percentage,
            monto_centavos: amount(appointment.precio_centavos, percentage),
            fecha_generacion: std::time::SystemTime::now().into(),
        })
        .await
}

pub fn date_range(from: &str, to: &str) -> Option<(DateTime<Utc>, DateTime<Utc>)> {
    fn parse(value: &str) -> Option<NaiveDate> {
        let date = NaiveDate::parse_from_str(value, "%Y-%m-%d").ok()?;
        (value.len() == 10 && date.format("%Y-%m-%d").to_string() == value).then_some(date)
    }
    let from = parse(from)?;
    let to = parse(to)?;
    if from > to {
        return None;
    }
    Some((start_of_day(from)?, start_of_day(to.succ_opt()?)?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        Json, Router,
        extract::{Path, State},
        http::StatusCode,
        routing::{get, post},
    };
    use serde_json::{Value, json};
    use std::sync::{Arc, Mutex};

    struct Database {
        appointment: Value,
        commission: Option<Value>,
        fail_write: bool,
        writes: usize,
    }
    type MockState = Arc<Mutex<Database>>;

    async fn find(State(state): State<MockState>, Json(query): Json<Value>) -> Json<Value> {
        let db = state.lock().unwrap();
        let docs = if query["selector"]["tipo"] == "comision" {
            db.commission.clone().into_iter().collect::<Vec<_>>()
        } else {
            vec![db.appointment.clone()]
        };
        Json(json!({"docs": docs}))
    }
    async fn save(State(state): State<MockState>, Json(value): Json<Value>) -> StatusCode {
        state.lock().unwrap().appointment = value;
        StatusCode::CREATED
    }
    async fn read_commission(State(state): State<MockState>) -> (StatusCode, Json<Value>) {
        match state.lock().unwrap().commission.clone() {
            Some(value) => (StatusCode::OK, Json(value)),
            None => (StatusCode::NOT_FOUND, Json(json!({}))),
        }
    }
    async fn write_commission(
        State(state): State<MockState>,
        Path(id): Path<String>,
        Json(mut value): Json<Value>,
    ) -> StatusCode {
        let mut db = state.lock().unwrap();
        db.writes += 1;
        if db.fail_write {
            db.fail_write = false;
            return StatusCode::SERVICE_UNAVAILABLE;
        }
        if db.commission.is_some() {
            return StatusCode::CONFLICT;
        }
        value["_id"] = json!(id);
        db.commission = Some(value);
        StatusCode::CREATED
    }
    async fn serve(router: Router) -> (String, tokio::task::JoinHandle<()>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let task = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        (url, task)
    }

    #[tokio::test]
    async fn completed_put_retries_failed_commission_and_preserves_existing_percentage() {
        use crate::{
            infrastructure::{
                database::couchdb::CouchDb,
                repositories::couchdb_commission_repository::CouchDbCommissionRepository,
            },
            presentation::routes,
        };
        let state = Arc::new(Mutex::new(Database {
            appointment: json!({"_id":"cita-1", "_rev":"1-test", "cliente_nombre":"Ana", "empleado_id":"empleado-1", "servicio_id":"servicio-1", "inicio":"2026-11-14T12:00:00Z", "estado":"pendiente", "precio_centavos":1002}),
            commission: None,
            fail_write: true,
            writes: 0,
        }));
        let mock = Router::new()
            .route("/test/_find", post(find))
            .route("/test", post(save))
            .route("/test/{id}", get(read_commission).put(write_commission))
            .with_state(state.clone());
        let (db_url, db_task) = serve(mock).await;
        let couch = CouchDb::new(&db_url, "test", "admin".into(), "secret".into()).unwrap();
        let (api_url, api_task) = serve(routes::router(couch.clone(), 40)).await;
        let client = reqwest::Client::new();
        let endpoint = format!("{api_url}/api/citas/cita-1");
        let trace = json!({"estado":"completada", "op_id":"550e8400-e29b-41d4-a716-446655440000", "ts_cliente":1, "vu":1, "iteracion":1});
        let first = client.put(&endpoint).json(&trace).send().await.unwrap();
        assert_eq!(first.status(), StatusCode::SERVICE_UNAVAILABLE);
        {
            let db = state.lock().unwrap();
            assert_eq!(db.appointment["estado"], "completada");
            assert!(db.commission.is_none());
        }
        assert_eq!(
            client
                .put(&endpoint)
                .json(&trace)
                .send()
                .await
                .unwrap()
                .status(),
            StatusCode::OK
        );
        let original = state.lock().unwrap().commission.clone().unwrap();
        assert_eq!(original["monto_centavos"], 401);
        assert_eq!(original["porcentaje"], 40);
        assert_eq!(original["cita_id"], "cita-1");
        assert_eq!(original["empleado_id"], "empleado-1");
        assert!(
            original["fecha_generacion"]
                .as_str()
                .unwrap()
                .ends_with('Z')
        );
        // Reinicio con otra configuración y reintento sin trazabilidad.
        let (second_url, second_task) = serve(routes::router(couch.clone(), 90)).await;
        assert_eq!(
            client
                .put(format!("{second_url}/api/citas/cita-1"))
                .json(&json!({"estado":"completada"}))
                .send()
                .await
                .unwrap()
                .status(),
            StatusCode::OK
        );
        {
            let db = state.lock().unwrap();
            assert_eq!(db.commission.as_ref(), Some(&original));
            assert_eq!(db.writes, 2);
        }
        // Una inserción que pierde la carrera obtiene el documento ganador sin sobrescribirlo.
        let repository = CouchDbCommissionRepository(&couch);
        let mut competing = repository
            .find_commission("comision:cita-1")
            .await
            .unwrap_or_else(|_| panic!("lectura"))
            .unwrap();
        competing.porcentaje = 90;
        competing.monto_centavos = 902;
        let winner = repository
            .insert_commission(competing)
            .await
            .unwrap_or_else(|_| panic!("conflicto"));
        assert_eq!(winner.porcentaje, 40);
        assert_eq!(state.lock().unwrap().commission.as_ref(), Some(&original));
        // Los límites son fechas locales inclusivas, aplicadas a generación, no a inicio.
        state.lock().unwrap().commission.as_mut().unwrap()["fecha_generacion"] =
            json!("2026-11-15T05:59:59Z");
        let list_url = format!("{api_url}/api/comisiones?desde=2026-11-14&hasta=2026-11-14");
        let result: Value = client
            .get(&list_url)
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(result["items"].as_array().unwrap().len(), 1);
        state.lock().unwrap().commission.as_mut().unwrap()["fecha_generacion"] =
            json!("2026-11-15T06:00:00Z");
        let result: Value = client
            .get(&list_url)
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert!(result["items"].as_array().unwrap().is_empty());
        for suffix in [
            "",
            "?desde=2026-11-15&hasta=2026-11-14",
            "?desde=2026-02-30&hasta=2026-11-14",
        ] {
            assert_eq!(
                client
                    .get(format!("{api_url}/api/comisiones{suffix}"))
                    .send()
                    .await
                    .unwrap()
                    .status(),
                StatusCode::BAD_REQUEST
            );
        }
        api_task.abort();
        second_task.abort();
        db_task.abort();
    }

    #[test]
    fn salvador_date_boundaries() {
        let (start, end) = date_range("2026-11-14", "2026-11-14").unwrap();
        assert_eq!(start.to_rfc3339(), "2026-11-14T06:00:00+00:00");
        assert_eq!(end.to_rfc3339(), "2026-11-15T06:00:00+00:00");
        for (from, to) in [
            ("2026-1-01", "2026-11-14"),
            ("2026-11-15", "2026-11-14"),
            ("bad", "bad"),
        ] {
            assert!(date_range(from, to).is_none());
        }
    }
}
