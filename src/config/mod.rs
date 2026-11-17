use std::{
    collections::HashMap,
    env, io,
    net::{IpAddr, SocketAddr},
    path::Path,
};

pub struct Config {
    pub auth_users_file: String,
    pub comision_porcentaje: u8,
    pub api_addr: SocketAddr,
    pub couchdb_url: String,
    pub couchdb_database: String,
    pub couchdb_user: String,
    pub couchdb_password: String,
}

impl Config {
    pub fn load() -> Result<Self, String> {
        let dotenv = read_dotenv(Path::new(".env"))?;

        let host = value(&dotenv, "API_HOST", Some("127.0.0.1"))?
            .parse::<IpAddr>()
            .map_err(|_| "API_HOST debe ser una dirección IP válida.".to_owned())?;
        let port = value(&dotenv, "API_PORT", Some("18080"))?
            .parse::<u16>()
            .ok()
            .filter(|port| *port != 0)
            .ok_or_else(|| "API_PORT debe estar entre 1 y 65535.".to_owned())?;

        let comision_porcentaje =
            parse_percentage(&value(&dotenv, "COMISION_PORCENTAJE", Some("40"))?)?;

        Ok(Self {
            auth_users_file: value(&dotenv, "AUTH_USERS_FILE", Some(".auth-users.json"))?,
            comision_porcentaje,
            api_addr: SocketAddr::new(host, port),
            couchdb_url: value(&dotenv, "COUCHDB_URL", None)?,
            couchdb_database: value(&dotenv, "COUCHDB_DATABASE", None)?,
            couchdb_user: value(&dotenv, "COUCHDB_USER", None)?,
            couchdb_password: value(&dotenv, "COUCHDB_PASSWORD", None)?,
        })
    }
}

fn read_dotenv(path: &Path) -> Result<HashMap<String, String>, String> {
    match dotenvy::from_path_iter(path) {
        Ok(entries) => entries
            .collect::<Result<_, _>>()
            .map_err(|_| "Hay una línea inválida en .env.".to_owned()),
        Err(dotenvy::Error::Io(error)) if error.kind() == io::ErrorKind::NotFound => {
            Ok(HashMap::new())
        }
        Err(_) => Err("No se pudo leer el archivo .env.".to_owned()),
    }
}

fn value(
    dotenv: &HashMap<String, String>,
    key: &str,
    default: Option<&str>,
) -> Result<String, String> {
    let setting = match env::var(key) {
        Ok(setting) => Some(setting),
        Err(env::VarError::NotPresent) => dotenv
            .get(key)
            .cloned()
            .or_else(|| default.map(str::to_owned)),
        Err(env::VarError::NotUnicode(_)) => return Err(format!("{key} debe ser texto UTF-8.")),
    };

    setting
        .filter(|setting| !setting.trim().is_empty())
        .ok_or_else(|| format!("Define {key} en el entorno o en .env."))
}

fn parse_percentage(value: &str) -> Result<u8, String> {
    value
        .parse::<u8>()
        .ok()
        .filter(|v| *v <= 100)
        .ok_or_else(|| "COMISION_PORCENTAJE debe ser un entero entre 0 y 100.".into())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn percentage_validation() {
        for value in ["0", "40", "100"] {
            assert!(parse_percentage(value).is_ok());
        }
        for value in ["-1", "101", "256", "40.5", ""] {
            assert!(parse_percentage(value).is_err());
        }
        assert_eq!(
            value(&HashMap::new(), "COMISION_TEST_DEFAULT", Some("40")).unwrap(),
            "40"
        );
    }
}
