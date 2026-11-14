use serde::{Deserialize, de::DeserializeOwned};
use serde_json::json;

use crate::{
    domain::{
        entities::{barber::Barber, catalog::CatalogEntry, service::Service},
        repositories::{AppointmentCatalog, RepositoryReadError},
    },
    infrastructure::database::couchdb::CouchDb,
};

pub struct CouchDbCatalogRepository<'a>(pub &'a CouchDb);

#[derive(Deserialize)]
struct CatalogDocument<T> {
    #[serde(rename = "_id")]
    id: String,
    #[serde(flatten)]
    data: T,
}

impl CouchDbCatalogRepository<'_> {
    pub async fn list_employees(&self) -> Result<Vec<CatalogEntry<Barber>>, RepositoryReadError> {
        self.list("empleado").await
    }

    pub async fn list_services(&self) -> Result<Vec<CatalogEntry<Service>>, RepositoryReadError> {
        self.list("servicio").await
    }

    async fn list<T: DeserializeOwned>(
        &self,
        document_type: &str,
    ) -> Result<Vec<CatalogEntry<T>>, RepositoryReadError> {
        let mut query = CouchDb::paged_query(json!({"tipo": document_type}));
        let mut entries = Vec::new();
        loop {
            let page = self.0.find_page::<CatalogDocument<T>>(&query).await?;
            let has_more = page.advance(&mut query)?;
            entries.extend(page.docs.into_iter().map(|document| CatalogEntry {
                id: document.id,
                data: document.data,
            }));
            if !has_more {
                break;
            }
        }

        entries.sort_unstable_by(|a, b| a.id.cmp(&b.id));
        Ok(entries)
    }
}

impl AppointmentCatalog for CouchDbCatalogRepository<'_> {
    async fn find_employee(&self, id: &str) -> Result<Option<Barber>, RepositoryReadError> {
        self.find(id, "empleado").await
    }

    async fn find_service(&self, id: &str) -> Result<Option<Service>, RepositoryReadError> {
        self.find(id, "servicio").await
    }
}

impl CouchDbCatalogRepository<'_> {
    async fn find<T: DeserializeOwned>(
        &self,
        id: &str,
        document_type: &str,
    ) -> Result<Option<T>, RepositoryReadError> {
        let query = json!({"selector": {"_id": {"$eq": id}, "tipo": document_type}, "limit": 1});
        Ok(self
            .0
            .find_page::<CatalogDocument<T>>(&query)
            .await?
            .docs
            .pop()
            .map(|document| document.data))
    }
}
