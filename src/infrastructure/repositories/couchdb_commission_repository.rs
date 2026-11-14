use crate::{
    domain::{
        entities::commission::Commission,
        repositories::{CommissionRepository, RepositoryReadError},
    },
    infrastructure::database::couchdb::CouchDb,
};
use chrono::{DateTime, Utc};
use reqwest::StatusCode;
use serde_json::json;

pub struct CouchDbCommissionRepository<'a>(pub &'a CouchDb);
impl CommissionRepository for CouchDbCommissionRepository<'_> {
    async fn find_commission(&self, id: &str) -> Result<Option<Commission>, RepositoryReadError> {
        let response = self
            .0
            .get_document(id)
            .send()
            .await
            .map_err(|_| RepositoryReadError::Unavailable)?;
        match response.status() {
            StatusCode::NOT_FOUND => Ok(None),
            StatusCode::OK => response
                .json()
                .await
                .map(Some)
                .map_err(|_| RepositoryReadError::InvalidData),
            _ => Err(RepositoryReadError::Unavailable),
        }
    }
    async fn insert_commission(
        &self,
        commission: Commission,
    ) -> Result<Commission, RepositoryReadError> {
        let mut document =
            serde_json::to_value(&commission).map_err(|_| RepositoryReadError::InvalidData)?;
        document.as_object_mut().unwrap().remove("id");
        document["tipo"] = json!("comision");
        let response = self
            .0
            .put_document(&commission.id, &document)
            .send()
            .await
            .map_err(|_| RepositoryReadError::Unavailable)?;
        match response.status() {
            StatusCode::CREATED | StatusCode::ACCEPTED => Ok(commission),
            StatusCode::CONFLICT => self
                .find_commission(&commission.id)
                .await?
                .ok_or(RepositoryReadError::InvalidData),
            _ => Err(RepositoryReadError::Unavailable),
        }
    }
    async fn list_generated_between(
        &self,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<Vec<Commission>, RepositoryReadError> {
        // Compara instantes para admitir también representaciones RFC3339 equivalentes.
        let mut query = CouchDb::paged_query(json!({"tipo": "comision"}));
        let mut items = Vec::new();
        loop {
            let page = self.0.find_page::<Commission>(&query).await?;
            let more = page.advance(&mut query)?;
            items.extend(
                page.docs
                    .into_iter()
                    .filter(|c| c.fecha_generacion >= start && c.fecha_generacion < end),
            );
            if !more {
                break;
            }
        }
        items.sort_unstable_by(|a, b| {
            a.fecha_generacion
                .cmp(&b.fecha_generacion)
                .then_with(|| a.id.cmp(&b.id))
        });
        Ok(items)
    }
}
