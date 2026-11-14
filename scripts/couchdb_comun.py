"""Conexión HTTP y configuración compartidas por los scripts de CouchDB."""

import base64
import http.client
import json
import os
import re
import secrets
import subprocess
import tempfile
import time
from pathlib import Path

RAIZ = Path(__file__).resolve().parents[1]


def preparar_entorno(nombre, variable):
    destino = RAIZ / nombre
    if destino.exists():
        return

    plantilla = (RAIZ / f"{nombre}.example").read_text(encoding="utf-8")
    contenido = plantilla.replace(
        f"{variable}=\n", f"{variable}={secrets.token_hex(24)}\n", 1
    )
    with tempfile.NamedTemporaryFile(mode="w", encoding="utf-8", dir=RAIZ) as temporal:
        temporal.write(contenido)
        temporal.flush()
        try:
            os.link(temporal.name, destino)
        except FileExistsError:
            return
    print(f"Archivo {nombre} creado con una contraseña aleatoria.", flush=True)


def credencial_basica(usuario, clave):
    if ":" in usuario:
        raise RuntimeError("El usuario de CouchDB no puede contener dos puntos.")
    return base64.b64encode(f"{usuario}:{clave}".encode("utf-8")).decode("ascii")


def solicitud(puerto, metodo, ruta, credencial=None, *, datos=None, timeout=2):
    encabezados = {"Accept": "application/json"}
    if credencial is not None:
        encabezados["Authorization"] = f"Basic {credencial}"
    cuerpo = None
    if datos is not None:
        encabezados["Content-Type"] = "application/json"
        cuerpo = json.dumps(datos)

    conexion = http.client.HTTPConnection("127.0.0.1", puerto, timeout=timeout)
    try:
        conexion.request(metodo, ruta, body=cuerpo, headers=encabezados)
        respuesta = conexion.getresponse()
        return respuesta.status, respuesta.read()
    finally:
        conexion.close()


def esperar_couchdb(puerto):
    limite = time.monotonic() + 60
    while time.monotonic() < limite:
        try:
            codigo, cuerpo = solicitud(puerto, "GET", "/_up")
            if codigo == 200 and json.loads(cuerpo).get("status") == "ok":
                return
        except (OSError, http.client.HTTPException, ValueError):
            pass
        time.sleep(1)
    raise RuntimeError("CouchDB no indicó que estuviera listo en 60 segundos.")


def servicios_compose(compose):
    resultado = subprocess.run(
        [*compose, "config", "--format", "json"],
        cwd=RAIZ,
        check=True,
        stdout=subprocess.PIPE,
    )
    return json.loads(resultado.stdout)["services"]


def parametros(servicio):
    entorno = servicio["environment"]
    base = entorno["COUCHDB_DATABASE"]
    if not re.fullmatch(r"[a-z][a-z0-9_-]*", base):
        raise RuntimeError(
            "El nombre de la base debe empezar con una letra minúscula "
            "y usar letras minúsculas, números, guiones o guiones bajos."
        )

    puerto = int(servicio["ports"][0]["published"])
    usuario = entorno["COUCHDB_USER"]
    clave = entorno["COUCHDB_PASSWORD"]
    return puerto, base, credencial_basica(usuario, clave)


def verificar_base(puerto, base, credencial, instancia):
    codigo, cuerpo = solicitud(puerto, "GET", f"/{base}", credencial)
    if codigo != 200 or json.loads(cuerpo).get("db_name") != base:
        raise RuntimeError(f"{instancia}: falló la consulta autenticada (HTTP {codigo}).")


def extremo_replicacion(nombre, base, servicio):
    entorno = servicio["environment"]
    return {
        "url": f"http://{nombre}:5984/{base}",
        "auth": {
            "basic": {
                "username": entorno["COUCHDB_USER"],
                "password": entorno["COUCHDB_PASSWORD"],
            },
        },
    }
