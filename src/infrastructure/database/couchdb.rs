use std::time::Duration;

use reqwest::{Client, StatusCode, Url, redirect::Policy};

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

    pub async fn database_available(&self) -> bool {
        self.client
            .head(self.database_url.clone())
            .basic_auth(&self.user, Some(&self.password))
            .send()
            .await
            .is_ok_and(|response| response.status() == StatusCode::OK)
    }
}
