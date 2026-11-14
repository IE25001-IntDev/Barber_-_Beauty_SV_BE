use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize)]
pub struct Service {
    pub nombre: String,
    pub duracion_minutos: u32,
    pub precio_centavos: u64,
    pub activo: bool,
}
