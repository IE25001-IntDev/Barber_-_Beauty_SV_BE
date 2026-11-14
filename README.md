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

## CouchDB local del laboratorio

Después de configurar CouchDB central, ejecuta desde la raíz del repositorio:

```bash
python3 scripts/iniciar-couchdb.py --local
```

El script reutiliza la imagen base y la configuración de nodo único de CouchDB central. En el primer arranque genera `.env.local` con una contraseña propia. Para elegir tus credenciales, copia `.env.local.example` a `.env.local` y define `COUCHDB_LOCAL_USER` y `COUCHDB_LOCAL_PASSWORD` antes del primer arranque. Compose exige un valor para ambas variables.

| Parámetro | CouchDB local |
|---|---|
| Servicio | `couchdb-local` |
| URL desde Linux | `http://127.0.0.1:15987` |
| URL dentro de la red de Compose | `http://couchdb-local:5984` |
| Base predeterminada | `barberia_citas` |
| Volumen de datos | `bad215-backend_couchdb_local_data` |
| Límites | 384 MiB de RAM y 1 CPU |

`COUCHDB_LOCAL_PORT` y `COUCHDB_LOCAL_DATABASE` permiten ajustar los valores en `.env.local`. El script espera a que CouchDB responda, crea la base si hace falta y verifica el acceso autenticado. La base `barberia_citas` de cada instancia guarda sus datos en el volumen que le corresponde. Para ver la base local en [Fauxton](http://127.0.0.1:15987/_utils/), usa las credenciales de `.env.local`.

El archivo `compose.local.yaml` agrega el servicio al proyecto del laboratorio cuando se carga junto a `compose.yaml`. Para ver ambas instancias y detener únicamente la local, ejecuta:

```bash
docker compose --env-file .env --env-file .env.local -f compose.yaml -f compose.local.yaml ps
docker compose --env-file .env --env-file .env.local -f compose.yaml -f compose.local.yaml stop couchdb-local
```

Con Docker Desktop para Linux, selecciona el contexto `desktop-linux` o antepone `DOCKER_CONTEXT=desktop-linux` a los comandos.

### Prueba de replicación local → central

Con ambos servicios activos, ejecuta:

```bash
python3 scripts/iniciar-couchdb.py --replicar-prueba
```

El script verifica las dos bases con sus credenciales, genera un `op_id` nuevo y crea el documento de prueba en CouchDB local. Tras comprobar que la central devuelve HTTP 404 para ese ID, solicita una replicación puntual mediante `POST /_replicate`. La opción `doc_ids` limita la transferencia a ese documento. La prueba termina cuando la central devuelve HTTP 200 con el mismo ID, revisión y contenido. El documento ficticio queda en ambas instancias como evidencia de la prueba.

La replicación usa las direcciones internas `couchdb-local:5984` y `couchdb:5984`, con las credenciales de cada servicio resueltas desde `.env.local` y `.env`.

### Replicación continua durante un corte del enlace

Con ambas instancias activas, ejecuta en Docker Desktop para Linux:

```bash
DOCKER_CONTEXT=desktop-linux python3 scripts/iniciar-couchdb.py --probar-corte
```

El script levanta `enlace-replicacion`, un intermediario TCP interno que reenvía a CouchDB central. El intermediario usa un hilo, admite hasta 64 conexiones simultáneas y dispone de un límite de memoria de 64 MiB. El script verifica el enlace desde el contenedor local antes de iniciar una replicación continua limitada a dos documentos ficticios. Una vez confirmado el primero en la central, detiene el intermediario durante 60 segundos, guarda el segundo documento en local y comprueba que puede leerlo allí mientras la central devuelve HTTP 404 para ese ID.

El trabajo de prueba configura 10 reintentos por solicitud para sostener la replicación continua durante el corte. Al restablecer el enlace, el mismo trabajo dispone de hasta 120 segundos para entregar el documento. El script verifica el ID, la revisión y el contenido y registra en la terminal las marcas UTC del corte y el tiempo hasta la primera lectura exitosa en central. Al terminar cancela el trabajo de prueba. Los documentos ficticios quedan disponibles para inspección.

Esta prueba verifica la recuperación del enlace de replicación. La campaña E4 también contempla el enlace cliente → API y la validación de las operaciones por Rust.

Si la terminal se cierra durante el corte, puedes restablecer el intermediario desde el repositorio:

```bash
docker --context desktop-linux compose --env-file .env --env-file .env.local \
  -f compose.yaml -f compose.local.yaml --profile replicacion start enlace-replicacion
```

### Corte del enlace cliente → API

Con CouchDB central y el backend Rust activos, ejecuta en otra terminal:

```bash
python3 scripts/iniciar-couchdb.py --probar-corte-api
```

La prueba inicia un intermediario HTTP temporal en `127.0.0.1` y usa `GET /api/ready` como solicitud del cliente. Comprueba que el intermediario devuelve HTTP 200, lo detiene durante 60 segundos y verifica que las conexiones a ese puerto son rechazadas al inicio y al final del corte. La ruta directa a Rust debe devolver HTTP 200 en ambas comprobaciones durante la interrupción. Después abre el intermediario en el mismo puerto y confirma que vuelve a responder con HTTP 200. El script registra las marcas UTC y el tiempo de la primera consulta después de restablecer el enlace.

Si Rust usa otro puerto, añade `--puerto-api` con su valor. El intermediario se ejecuta en el mismo equipo que Rust y se cierra al terminar el comando. Esta prueba usa el endpoint de disponibilidad y deja para la ejecución integrada el corte simultáneo del enlace de replicación y la comprobación de las operaciones de negocio recuperadas por Rust.

- [Contrato de API](CONTRATO_API.md)
- [Instalación en Docker](https://docs.couchdb.org/en/stable/install/docker.html)
- [Configuración de CouchDB en nodo único](https://docs.couchdb.org/en/stable/setup/single-node.html)

### Ensayo E4 de infraestructura con ambos enlaces

Con CouchDB central, CouchDB local y la API Rust activos, ejecuta:

```bash
DOCKER_CONTEXT=desktop-linux python3 scripts/iniciar-couchdb.py --probar-e4
```

El ensayo verifica los dos enlaces antes de la interrupción. Después los mantiene cortados durante una ventana común de 60 segundos y escribe un documento ficticio en CouchDB local. Durante el corte, el puerto del intermediario HTTP rechaza conexiones, la API directa responde y el documento se lee en CouchDB local mientras CouchDB central devuelve HTTP 404 para su ID.

Al restablecer los enlaces, el ensayo comprueba HTTP 200 a través del intermediario cliente → API y verifica que el mismo trabajo de replicación entrega a la central el documento con su ID, revisión y contenido. La salida esperada termina con `Ensayo E4 de infraestructura completado.` La prueba de validación y aplicación de operaciones de citas requiere el módulo de negocio integrado.
