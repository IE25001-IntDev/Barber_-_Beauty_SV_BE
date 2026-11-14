use serde::Serialize;

#[derive(Serialize)]
pub struct CatalogEntry<T> {
    pub id: String,
    #[serde(flatten)]
    pub data: T,
}
