#!/usr/bin/env python3
"""Levanta CouchDB central y comprueba la base de la aplicación."""

import base64
import http.client
import json
import os
import re
import secrets
import subprocess
import sys
import tempfile
import time
from pathlib import Path

RAIZ = Path(__file__).resolve().parents[1]
COMPOSE = ["docker", "compose", "--env-file", ".env", "-f", "compose.yaml"]


def preparar_entorno():
    destino = RAIZ / ".env"
    if destino.exists():
        return

    plantilla = (RAIZ / ".env.example").read_text(encoding="utf-8")
    contenido = plantilla.replace(
        "COUCHDB_PASSWORD=\n", f"COUCHDB_PASSWORD={secrets.token_hex(24)}\n", 1
    )
    with tempfile.NamedTemporaryFile(mode="w", encoding="utf-8", dir=RAIZ) as temporal:
        temporal.write(contenido)
        temporal.flush()
        try:
            os.link(temporal.name, destino)
        except FileExistsError:
            return
    print("Archivo .env local creado con una contraseña aleatoria.", flush=True)


def solicitud(puerto, metodo, ruta, credencial=None):
    encabezados = {"Accept": "application/json"}
    if credencial is not None:
        encabezados["Authorization"] = f"Basic {credencial}"

    conexion = http.client.HTTPConnection("127.0.0.1", puerto, timeout=2)
    try:
        conexion.request(metodo, ruta, headers=encabezados)
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


def main():
    preparar_entorno()
    resultado = subprocess.run(
        [*COMPOSE, "config", "--format", "json"],
        cwd=RAIZ,
        check=True,
        stdout=subprocess.PIPE,
    )
    servicio = json.loads(resultado.stdout)["services"]["couchdb"]
    entorno = servicio["environment"]
    base = entorno["COUCHDB_DATABASE"]
    if not re.fullmatch(r"[a-z][a-z0-9_-]*", base):
        raise RuntimeError(
            "COUCHDB_DATABASE debe empezar con una letra minúscula "
            "y usar letras minúsculas, números, guiones o guiones bajos."
        )

    puerto = int(servicio["ports"][0]["published"])
    usuario = entorno["COUCHDB_USER"]
    clave = entorno["COUCHDB_PASSWORD"]
    if ":" in usuario:
        raise RuntimeError("COUCHDB_USER no puede contener dos puntos.")
    credencial = base64.b64encode(f"{usuario}:{clave}".encode("utf-8")).decode("ascii")

    subprocess.run([*COMPOSE, "up", "-d", "--build", "couchdb"], cwd=RAIZ, check=True)
    esperar_couchdb(puerto)

    codigo, cuerpo = solicitud(puerto, "PUT", f"/{base}", credencial)
    if codigo not in (201, 202) and not (
        codigo == 412 and json.loads(cuerpo).get("error") == "file_exists"
    ):
        raise RuntimeError(f"CouchDB respondió HTTP {codigo} al preparar la base.")

    codigo, cuerpo = solicitud(puerto, "GET", f"/{base}", credencial)
    if codigo != 200 or json.loads(cuerpo).get("db_name") != base:
        raise RuntimeError(f"La consulta autenticada de la base falló (HTTP {codigo}).")

    codigo, _ = solicitud(puerto, "GET", f"/{base}")
    if codigo not in (401, 403):
        raise RuntimeError(f"La base respondió HTTP {codigo} a una consulta sin credenciales.")

    print(f"CouchDB listo. Base: {base}. Consulta autenticada: HTTP 200.")
    print(f"Fauxton: http://127.0.0.1:{puerto}/_utils/")


if __name__ == "__main__":
    try:
        main()
    except FileNotFoundError as error:
        if error.filename == "docker":
            print("Docker no está disponible en esta terminal.", file=sys.stderr)
        else:
            print(f"Falta un archivo requerido: {error.filename}", file=sys.stderr)
        sys.exit(1)
    except subprocess.CalledProcessError:
        print(
            "Falló Docker Compose. Revisa el mensaje de Docker y ejecuta "
            "`docker compose logs --tail=60 couchdb` si el servicio llegó a iniciarse.",
            file=sys.stderr,
        )
        sys.exit(1)
    except (OSError, RuntimeError, ValueError, http.client.HTTPException) as error:
        print(f"No se pudo verificar CouchDB. {error}", file=sys.stderr)
        sys.exit(1)
