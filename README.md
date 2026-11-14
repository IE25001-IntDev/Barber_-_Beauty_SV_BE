# Barber & Beauty SV · Backend

## CouchDB central para desarrollo

Desde la raíz del repositorio, con Docker Desktop o Docker Engine en ejecución y Docker Compose v2 o posterior, inicia CouchDB con Python 3.

```bash
python3 scripts/iniciar-couchdb.py
```

La primera ejecución crea un archivo local `.env` con una contraseña aleatoria. Para definir tus propias credenciales desde el principio, copia `.env.example` a `.env`, edita `COUCHDB_USER` y `COUCHDB_PASSWORD`, y ejecuta el mismo comando. Comparte la plantilla y conserva el archivo `.env` en tu equipo.

El comando construye la imagen a partir de CouchDB 3.5.3, arranca el servicio, espera a que responda y crea la base `barberia_citas` si hace falta. Comprueba que la consulta autenticada devuelve HTTP 200 y que la base exige credenciales. Puedes volver a ejecutarlo para verificar una instalación existente.

| Parámetro | Valor inicial |
|---|---|
| Proyecto de Compose | `bad215-backend` |
| URL desde Linux | `http://127.0.0.1:15986` |
| URL para otros servicios del mismo proyecto | `http://couchdb:5984` |
| Base de aplicación | `barberia_citas` |
| Volumen de datos | `bad215-backend_couchdb_data` |
| Límites de CouchDB | 512 MiB de RAM y 1 CPU |

Si el puerto está ocupado, actualiza `COUCHDB_PORT` en `.env` y pon el mismo puerto en `COUCHDB_URL`, que usará el backend Rust. El puerto se publica únicamente en la interfaz local del anfitrión. Fauxton está disponible en [http://127.0.0.1:15986/_utils/](http://127.0.0.1:15986/_utils/).

Al abrir Fauxton, el navegador muestra un cuadro de autenticación. Usa `COUCHDB_USER` y `COUCHDB_PASSWORD` del `.env` de este repositorio.

```bash
docker compose config --quiet
docker compose ps
docker compose logs --tail=60 couchdb
docker compose stop couchdb
```

`docker compose stop couchdb` permite liberar recursos y conservar los documentos en el volumen. El proyecto y el puerto son independientes del laboratorio previo, que usaba `15984` y `15985`. La comprobación de arranque usa la cuenta administrativa local para crear la base.

## API Rust y comprobación de CouchDB

Con el CouchDB central disponible, ejecuta desde la raíz del repositorio:

```bash
CARGO_BUILD_JOBS=1 CARGO_PROFILE_DEV_DEBUG=0 cargo run --locked
```

Rust lee `COUCHDB_URL`, `COUCHDB_DATABASE`, `COUCHDB_USER` y `COUCHDB_PASSWORD` desde el entorno o desde el `.env` existente. Los valores exportados en la terminal tienen prioridad. Usa la URL HTTP local del servicio de desarrollo y las credenciales con acceso a la base. La API escucha en `127.0.0.1:18080` por defecto. Se puede cambiar con `API_HOST` y `API_PORT`.

En una segunda terminal, ejecuta:

```bash
curl --noproxy '*' -fsS --max-time 5 http://127.0.0.1:18080/api/health
curl --noproxy '*' -fsS --max-time 5 http://127.0.0.1:18080/api/ready
```

Ambas rutas devuelven `{"status":"ok"}` cuando el proceso responde y la conexión a CouchDB está disponible. `/api/health` comprueba el proceso. `/api/ready` hace una consulta `HEAD` autenticada a la base configurada y devuelve HTTP 503 si CouchDB no responde, las credenciales se rechazan o la base no existe. La consulta es de solo lectura.

- [Contrato de API](CONTRATO_API.md)
- [Instalación en Docker](https://docs.couchdb.org/en/stable/install/docker.html)
- [Configuración de CouchDB en nodo único](https://docs.couchdb.org/en/stable/setup/single-node.html)
