use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Commission {
    #[serde(alias = "_id")]
    pub id: String,
    pub cita_id: String,
    pub empleado_id: String,
    pub porcentaje: u8,
    pub monto_centavos: u64,
    pub fecha_generacion: DateTime<Utc>,
}

pub fn amount(price: u64, percentage: u8) -> u64 {
    // Redondeo al centavo más cercano; las mitades se redondean hacia arriba.
    ((u128::from(price) * u128::from(percentage) + 50) / 100) as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn calculation_and_rounding() {
        assert_eq!(amount(1000, 40), 400);
        assert_eq!(amount(101, 40), 40);
        assert_eq!(amount(102, 40), 41);
        assert_eq!(amount(1, 50), 1);
        assert_eq!(amount(u64::MAX, 100), u64::MAX);
        assert_eq!(amount(u64::MAX, 0), 0);
    }
}
