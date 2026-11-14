# Contrato de API del backend · BAD 215

**Estado** — Borrador v0.1 para revisión de Erick, Diego, César. Las rutas y los campos de este documento son propuestas para la integración. La API del laboratorio es la referencia de las pruebas preliminares.

## 1. Convenciones

- La API usa el prefijo `/api`. Los cuerpos de solicitudes y respuestas usan JSON en UTF-8. Para las escrituras se envía `Content-Type: application/json`.
- Los identificadores públicos son cadenas. Para nuevos registros se propone UUID v4. En las citas, el cliente genera el `id` al crear el registro para poder conservarlo aunque todavía no tenga conexión.
- Las marcas de tiempo de negocio usan RFC 3339 con zona horaria explícita. El backend devuelve los instantes en UTC. Las fechas de consulta se interpretan en `America/El_Salvador`.
- Los importes se representan como enteros en centavos de USD. Por ejemplo, `precio_centavos: 800` representa USD 8,00.
- La API expone `id`. La traducción a `_id` y el uso de `_rev` pertenecen a la capa de persistencia. Los controladores devuelven datos de la API en un formato consistente para el cliente.

## 2. Rutas propuestas

| Método y ruta | Operación | Respuesta exitosa | Responsable |
|---|---|---|---|
| `GET /api/health` | Comprobar que el proceso de la API responde | `200`, `{"status":"ok"}` | Erick |
| `GET /api/citas?fecha=YYYY-MM-DD` | Consultar las citas del día indicado (OP1) | `200`, lista | Diego |
| `GET /api/citas/{id}` | Consultar una cita por su ID | `200`, cita | Diego |
| `POST /api/citas` | Crear una cita (OP2) | `201`, cita | Diego |
| `PUT /api/citas/{id}` | Actualizar el estado de una cita (OP3) | `200`, cita actualizada | Diego |
| `GET /api/empleados` y `GET /api/servicios` | Consultar los catálogos | `200`, lista | Diego |
| `GET /api/comisiones?desde=YYYY-MM-DD&hasta=YYYY-MM-DD` | Consultar comisiones de un periodo | `200`, lista | César |

Las listas usan un objeto con el arreglo `items`. Una lista vacía se devuelve como `{"items":[]}`. En OP1, `fecha` es obligatoria y el cliente de pruebas envía una fecha fija del conjunto semilla en formato `YYYY-MM-DD`. José ajustará el ejemplo `fecha=hoy` del protocolo para que las repeticiones consulten los mismos datos. OP3 conserva la ruta del prototipo y admite únicamente el cambio de estado. Las escrituras de los catálogos se documentarán cuando Diego defina sus requisitos.

## 3. Datos que comparten los módulos

| Entidad | Campos de integración propuestos |
|---|---|
| Cita | `id`, `cliente_nombre`, `empleado_id`, `servicio_id`, `inicio`, `estado`, `precio_centavos` |
| Empleado | `id`, `nombre`, `activo` |
| Servicio | `id`, `nombre`, `duracion_minutos`, `precio_centavos`, `activo` |
| Comisión | `id`, `cita_id`, `empleado_id`, `monto_centavos` |

Para el primer alcance se propone una cita con un empleado y un servicio. El backend toma el precio del servicio al crear la cita y conserva ese importe en el registro para mantener su historial. En el código existente, `appointment`, `barber` y `commission` son los nombres de los módulos de dominio. Los nombres públicos de los campos y rutas se mantienen en español.

Los estados iniciales propuestos son `pendiente`, `completada` y `cancelada`. Una cita nace `pendiente`. Diego validará las transiciones permitidas y el criterio de disponibilidad. César confirmará el origen de la tasa de comisión, el evento que la genera y la fecha que determina el periodo de liquidación. La tabla define los campos de intercambio y deja esas reglas a sus responsables.

### Ejemplo de creación (OP2)

Solicitud a `POST /api/citas` para una cita ficticia.

```json
{
  "id": "42ac902f-1f10-4f47-bb89-33acccf21a14",
  "cliente_nombre": "Cliente de prueba",
  "empleado_id": "5c3d0d9f-5520-4236-99da-91473fc49132",
  "servicio_id": "ea2f6139-81e5-4bf1-b9fa-658b2e35306a",
  "inicio": "2026-11-20T14:00:00-06:00"
}
```

Respuesta `201 Created` propuesta.

```json
{
  "id": "42ac902f-1f10-4f47-bb89-33acccf21a14",
  "cliente_nombre": "Cliente de prueba",
  "empleado_id": "5c3d0d9f-5520-4236-99da-91473fc49132",
  "servicio_id": "ea2f6139-81e5-4bf1-b9fa-658b2e35306a",
  "inicio": "2026-11-20T20:00:00Z",
  "estado": "pendiente",
  "precio_centavos": 800
}
```

### Ejemplo de actualización (OP3)

Solicitud a `PUT /api/citas/42ac902f-1f10-4f47-bb89-33acccf21a14`.

```json
{
  "estado": "completada"
}
```

La respuesta `200 OK` devuelve la cita completa con el estado actualizado.

## 4. Trazabilidad y persistencia

- **ID de cita.** Identifica el mismo recurso durante toda su vida. Se usa en la URL y como `_id` del documento de cita.
- **`op_id`.** UUID v4 distinto para cada escritura lógica. Un reintento de la misma escritura reutiliza el mismo `op_id` y el mismo contenido. Se puede usar como `_id` en un documento de operación independiente.
- **Pruebas OP2 y OP3.** Las dos variantes del experimento añaden al cuerpo `op_id` (UUID v4), `ts_cliente` (época Unix en milisegundos), `vu` (entero positivo) e `iteracion` (entero desde 0). El simulador offline guarda los mismos datos y el tipo de operación. Para OP3, conserva también el ID de cita que venía en la URL.
- **Confirmaciones.** El cliente de simulación distingue la escritura local, la llegada del documento de operación a CouchDB central y su aplicación por Rust. La replicación traslada documentos. Rust debe validar la operación antes de modificar la cita. Se propone replicar documentos de operación y procesarlos en Rust, con el mismo conjunto de reglas que se usa en la variante en línea.
- **Concurrencia y reintentos.** La persistencia usa la revisión vigente de CouchDB al actualizar un documento. Un conflicto de revisión se informa como `409`. Para la campaña, el equipo debe confirmar el registro durable que permitirá reconocer un reintento tras una respuesta perdida, resolver conflictos de negocio y verificar cada operación por `op_id`.

## 5. Errores e integración

| HTTP | Caso propuesto |
|---|---|
| `400` | JSON mal formado, fecha inválida o parámetro obligatorio ausente |
| `404` | Cita solicitada inexistente |
| `409` | ID de cita en uso por otra creación, conflicto de escritura o reutilización de un `op_id` con contenido diferente |
| `415` | Tipo de contenido incompatible en una escritura |
| `422` | JSON válido con campos o valores que incumplen el contrato |
| `503` | CouchDB no está disponible para atender una operación |

El cuerpo de error usa un código estable y un mensaje breve. Por ejemplo, `{"error":{"codigo":"CITA_NO_ENCONTRADA","mensaje":"No se encontró la cita solicitada"}}`. La API omite credenciales y detalles internos de CouchDB en estas respuestas.

**Infraestructura de Erick.** La API lee `COUCHDB_URL`, `COUCHDB_DATABASE`, `COUCHDB_USER` y `COUCHDB_PASSWORD` desde el entorno. Se compartirá una plantilla `.env.example`. Cada integrante conserva su `.env` local fuera de Git. Los puertos del laboratorio se configuran por entorno. La ruta de salud comprueba el proceso de la API. El arranque del laboratorio verifica también el acceso autenticado a CouchDB.

**Acuerdos para cerrar el borrador**

- Diego valida las rutas de citas y catálogos, los campos obligatorios y las transiciones de estado.
- César valida los datos que recibe de la cita, la regla de comisión y el significado de `desde` y `hasta`.
- José y Erick validan la fecha semilla, los campos de trazabilidad, el formato de confirmaciones y el uso de OP3 en k6. También acuerdan quién implementará el cliente de simulación offline.

**Referencias técnicas**

- Apache CouchDB, [API de documentos y revisiones](https://docs.couchdb.org/en/stable/api/document/common.html).
- Apache CouchDB, [protocolo de replicación](https://docs.couchdb.org/en/stable/replication/protocol.html).
- IETF, [RFC 9110, semántica HTTP](https://www.rfc-editor.org/rfc/rfc9110.html).
