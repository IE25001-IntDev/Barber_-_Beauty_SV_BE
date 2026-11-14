use std::time::Duration;

use serde::{Deserialize, de::DeserializeOwned};
use serde_json::{Value, json};

use crate::domain::repositories::RepositoryReadError;

const PAGE_SIZE: usize = 100;

#[derive(Deserialize)]
pub(crate) struct FindPage<T> {
    pub(crate) docs: Vec<T>,
    bookmark: Option<String>,
}

impl<T> FindPage<T> {
    pub(crate) fn advance(&self, query: &mut Value) -> Result<bool, RepositoryReadError> {
        if self.docs.len() < PAGE_SIZE {
            return Ok(false);
        }

        let bookmark = self
            .bookmark
            .as_deref()
            .filter(|bookmark| {
                !bookmark.is_empty() && query["bookmark"].as_str() != Some(*bookmark)
            })
            .ok_or(RepositoryReadError::InvalidData)?;
        query["bookmark"] = json!(bookmark);
        Ok(true)
    }
}

use reqwest::{Client, RequestBuilder, StatusCode, Url, redirect::Policy};

#[derive(Clone)]
pub struct CouchDb {
    client: Client,
    database_url: Url,
    user: String,
    password: String,
}

impl CouchDb {
    pub fn new(
        base_url: &str,
        database: &str,
        user: String,
        password: String,
    ) -> Result<Self, String> {
        let mut database_url =
            Url::parse(base_url).map_err(|_| "COUCHDB_URL debe ser una URL válida.".to_owned())?;
        if database_url.scheme() != "http"
            || database_url.host().is_none()
            || !database_url.username().is_empty()
            || database_url.password().is_some()
            || database_url.query().is_some()
            || database_url.fragment().is_some()
        {
            return Err(
                "COUCHDB_URL debe usar HTTP sin credenciales, consulta ni fragmento.".into(),
            );
        }
        if !database
            .as_bytes()
            .first()
            .is_some_and(u8::is_ascii_lowercase)
            || !database.bytes().all(|byte| {
                byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"_-".contains(&byte)
            })
        {
            return Err("COUCHDB_DATABASE debe ser un nombre válido de base de datos.".into());
        }
        database_url
            .path_segments_mut()
            .map_err(|_| "COUCHDB_URL no puede usarse como URL base.".to_owned())?
            .pop_if_empty()
            .push(database);
        if user.contains(':') {
            return Err("COUCHDB_USER no puede contener dos puntos.".into());
        }

        let client = Client::builder()
            .no_proxy()
            .redirect(Policy::none())
            .connect_timeout(Duration::from_secs(2))
            .timeout(Duration::from_secs(3))
            .build()
            .map_err(|_| "No se pudo crear el cliente HTTP para CouchDB.".to_owned())?;

        Ok(Self {
            client,
            database_url,
            user,
            password,
        })
    }

    pub(crate) async fn find_page<T: DeserializeOwned>(
        &self,
        query: &Value,
    ) -> Result<FindPage<T>, RepositoryReadError> {
        let response = self
            .find(query)
            .send()
            .await
            .map_err(|_| RepositoryReadError::Unavailable)?;
        if !response.status().is_success() {
            return Err(RepositoryReadError::Unavailable);
        }

        response
            .json()
            .await
            .map_err(|_| RepositoryReadError::InvalidData)
    }

    pub(crate) fn paged_query(selector: Value) -> Value {
        json!({"selector": selector, "limit": PAGE_SIZE})
    }

    fn find(&self, query: &Value) -> RequestBuilder {
        let mut url = self.database_url.clone();
        url.path_segments_mut()
            .expect("la URL de CouchDB admite segmentos de ruta")
            .push("_find");

        self.client
            .post(url)
            .basic_auth(&self.user, Some(&self.password))
            .json(query)
    }

    pub(crate) fn put_document(&self, id: &str, document: &Value) -> RequestBuilder {
        let mut url = self.database_url.clone();
        url.path_segments_mut()
            .expect("la URL de CouchDB admite segmentos de ruta")
            .push(id);

        self.client
            .put(url)
            .basic_auth(&self.user, Some(&self.password))
            .json(document)
    }

    pub(crate) fn save_document(&self, document: &Value) -> RequestBuilder {
        self.client
            .post(self.database_url.clone())
            .basic_auth(&self.user, Some(&self.password))
            .json(document)
    }

    pub async fn database_available(&self) -> bool {
        self.client
            .head(self.database_url.clone())
            .basic_auth(&self.user, Some(&self.password))
            .send()
            .await
            .is_ok_and(|response| response.status() == StatusCode::OK)
    }
}
