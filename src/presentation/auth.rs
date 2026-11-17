use crate::shared::errors::error_response;
use argon2::{Argon2, PasswordHash, PasswordHasher, PasswordVerifier, password_hash::SaltString};
use axum::{
    Extension, Json,
    extract::Request,
    http::{HeaderMap, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::{Mutex, Semaphore};
use uuid::Uuid;

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct User {
    pub usuario: String,
    pub nombre: String,
    pub rol: Role,
    pub empleado_id: Option<String>,
}
#[derive(Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Administrador,
    Estilista,
}
impl User {
    pub fn allows(&self, employee: &str) -> bool {
        self.rol == Role::Administrador || self.empleado_id.as_deref() == Some(employee)
    }
}
#[derive(Clone, Deserialize, Serialize)]
struct Account {
    user: User,
    password_hash: String,
}
struct Session {
    user: User,
    expires: Instant,
}
struct Data {
    sessions: HashMap<String, Session>,
    attempts: HashMap<String, (Instant, u32)>,
}
// shortcut: sesiones de una instancia; compartirlas antes de desplegar varias réplicas.
pub struct Auth {
    accounts: Vec<Account>,
    data: Mutex<Data>,
    hashing: Arc<Semaphore>,
}
impl Auth {
    pub fn load(path: &str) -> Result<Arc<Self>, String> {
        let text = std::fs::read_to_string(path).map_err(|_| {
            "Falta el archivo de usuarios. Ejecuta --crear-usuarios-demo para el prototipo."
                .to_owned()
        })?;
        let accounts: Vec<Account> =
            serde_json::from_str(&text).map_err(|_| "Archivo de usuarios inválido.".to_owned())?;
        let mut names = HashSet::new();
        if accounts.is_empty()
            || !accounts.iter().any(|a| a.user.rol == Role::Administrador)
            || accounts.iter().any(|a| {
                a.user.usuario.is_empty()
                    || a.user.usuario.len() > 80
                    || a.user.nombre.trim().is_empty()
                    || !names.insert(a.user.usuario.clone())
                    || PasswordHash::new(&a.password_hash).is_err()
                    || (a.user.rol == Role::Estilista
                        && a.user
                            .empleado_id
                            .as_ref()
                            .is_none_or(|s| s.trim().is_empty()))
            })
        {
            return Err("Usuarios duplicados, campos inválidos o falta administrador.".into());
        }
        Ok(Arc::new(Self {
            accounts,
            data: Mutex::new(Data {
                sessions: HashMap::new(),
                attempts: HashMap::new(),
            }),
            hashing: Arc::new(Semaphore::new(2)),
        }))
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Login {
    usuario: String,
    password: String,
}
pub async fn login(Extension(auth): Extension<Arc<Auth>>, Json(input): Json<Login>) -> Response {
    if input.usuario.len() > 80 || input.password.len() > 256 || input.password.is_empty() {
        return unauthorized();
    }
    let account = auth
        .accounts
        .iter()
        .find(|a| a.user.usuario == input.usuario)
        .cloned();
    {
        let mut data = auth.data.lock().await;
        data.attempts
            .retain(|_, (time, _)| time.elapsed() < Duration::from_secs(60));
        // Una clave fija para desconocidos evita crecimiento arbitrario de memoria.
        let key = account
            .as_ref()
            .map(|a| a.user.usuario.as_str())
            .unwrap_or("")
            .to_owned();
        let entry = data.attempts.entry(key).or_insert((Instant::now(), 0));
        if entry.1 >= 5 {
            return error_response(
                StatusCode::TOO_MANY_REQUESTS,
                "ESPERA",
                "Demasiados intentos. Espera un minuto e inténtalo de nuevo.",
            );
        }
        entry.1 += 1;
    }
    let Ok(permit) = auth.hashing.clone().try_acquire_owned() else {
        return error_response(
            StatusCode::TOO_MANY_REQUESTS,
            "ESPERA",
            "Inténtalo de nuevo en unos segundos.",
        );
    };
    let hash = account
        .as_ref()
        .unwrap_or(&auth.accounts[0])
        .password_hash
        .clone();
    let valid = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        PasswordHash::new(&hash).is_ok_and(|h| {
            Argon2::default()
                .verify_password(input.password.as_bytes(), &h)
                .is_ok()
        })
    })
    .await
    .unwrap_or(false);
    let Some(account) = account.filter(|_| valid) else {
        return unauthorized();
    };
    let token = Uuid::new_v4().to_string();
    let mut data = auth.data.lock().await;
    data.sessions.retain(|_, s| s.expires > Instant::now());
    // Una sesión por cuenta: el acceso nuevo revoca la anterior.
    data.sessions
        .retain(|_, s| s.user.usuario != account.user.usuario);
    data.sessions.insert(
        token.clone(),
        Session {
            user: account.user.clone(),
            expires: Instant::now() + Duration::from_secs(8 * 3600),
        },
    );
    (
        [("cache-control", "no-store")],
        Json(json!({"token": token, "user": account.user})),
    )
        .into_response()
}
fn token(headers: &HeaderMap) -> Option<&str> {
    headers
        .get("authorization")?
        .to_str()
        .ok()?
        .strip_prefix("Bearer ")
}
pub async fn protect(
    Extension(auth): Extension<Arc<Auth>>,
    mut req: Request,
    next: Next,
) -> Response {
    let user = {
        let data = auth.data.lock().await;
        token(req.headers())
            .and_then(|t| data.sessions.get(t))
            .filter(|s| s.expires > Instant::now())
            .map(|s| s.user.clone())
    };
    let Some(user) = user else {
        return unauthorized();
    };
    req.extensions_mut().insert(user);
    let mut response = next.run(req).await;
    response
        .headers_mut()
        .insert("cache-control", "no-store".parse().unwrap());
    response
}
pub async fn logout(
    Extension(auth): Extension<Arc<Auth>>,
    headers: HeaderMap,
) -> Json<serde_json::Value> {
    if let Some(t) = token(&headers) {
        auth.data.lock().await.sessions.remove(t);
    }
    Json(json!({"status": "ok"}))
}
pub fn forbidden() -> Response {
    error_response(
        StatusCode::FORBIDDEN,
        "SIN_PERMISO",
        "No tienes permiso para acceder a los datos de otro profesional.",
    )
}
fn unauthorized() -> Response {
    error_response(
        StatusCode::UNAUTHORIZED,
        "SESION_REQUERIDA",
        "Inicia sesión con un usuario y contraseña válidos. La sesión puede haber vencido.",
    )
}

pub fn create_demo(path: &str) -> Result<(), Box<dyn std::error::Error>> {
    use std::io::Write;
    let mut accounts = Vec::new();
    let mut credentials = String::from("CUENTAS FICTICIAS LOCALES. No compartir ni subir a Git.\n");
    for (username, name, employee) in [
        ("admin", "Administración", None),
        ("ana", "Ana Martínez", Some("demo-empleado-ana")),
        ("carlos", "Carlos Rivera", Some("demo-empleado-carlos")),
        ("sofia", "Sofía López", Some("demo-empleado-sofia")),
    ] {
        let password = Uuid::new_v4().simple().to_string();
        let salt = SaltString::encode_b64(Uuid::new_v4().as_bytes()).map_err(|e| e.to_string())?;
        let hash = Argon2::default()
            .hash_password(password.as_bytes(), &salt)
            .map_err(|e| e.to_string())?
            .to_string();
        accounts.push(Account {
            user: User {
                usuario: username.into(),
                nombre: name.into(),
                rol: if employee.is_none() {
                    Role::Administrador
                } else {
                    Role::Estilista
                },
                empleado_id: employee.map(str::to_owned),
            },
            password_hash: hash,
        });
        credentials.push_str(&format!("{username}: {password}\n"));
    }
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    // create_new impide reemplazar cuentas existentes por accidente.
    let mut file = options.open(path)?;
    let mut passwords = options.open(format!("{path}.credenciales.txt"))?;
    passwords.write_all(credentials.as_bytes())?;
    file.write_all(serde_json::to_string_pretty(&accounts)?.as_bytes())?;
    println!("Usuarios creados. Consulta las claves locales en {path}.credenciales.txt");
    Ok(())
}
#[cfg(test)]
impl Auth {
    pub fn test_accounts() -> Arc<Self> {
        let salt = SaltString::encode_b64(b"test-salt-for-unit").unwrap();
        let hash = Argon2::default()
            .hash_password(b"test-password", &salt)
            .unwrap()
            .to_string();
        Arc::new(Self {
            accounts: vec![
                Account {
                    user: User {
                        usuario: "admin".into(),
                        nombre: "Admin".into(),
                        rol: Role::Administrador,
                        empleado_id: None,
                    },
                    password_hash: hash.clone(),
                },
                Account {
                    user: User {
                        usuario: "ana".into(),
                        nombre: "Ana".into(),
                        rol: Role::Estilista,
                        empleado_id: Some("otro-empleado".into()),
                    },
                    password_hash: hash,
                },
            ],
            data: Mutex::new(Data {
                sessions: HashMap::new(),
                attempts: HashMap::new(),
            }),
            hashing: Arc::new(Semaphore::new(2)),
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn permissions_cannot_cross_employees() {
        let mut user = User {
            usuario: "ana".into(),
            nombre: "Ana".into(),
            rol: Role::Estilista,
            empleado_id: Some("ana".into()),
        };
        assert!(user.allows("ana"));
        assert!(!user.allows("carlos"));
        user.empleado_id = None;
        assert!(!user.allows("ana"));
        user.rol = Role::Administrador;
        assert!(user.allows("carlos"));
    }
}
