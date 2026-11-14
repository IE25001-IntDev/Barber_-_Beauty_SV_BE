use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize)]
pub struct Barber {
    pub nombre: String,
    pub activo: bool,
}
