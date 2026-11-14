#!/usr/bin/env python3
"""Inicia CouchDB o prueba una replicación del laboratorio."""

import argparse
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
import uuid
from pathlib import Path

RAIZ = Path(__file__).resolve().parents[1]
COMPOSE = ["docker", "compose", "--env-file", ".env", "-f", "compose.yaml"]
LOCAL = ["--env-file", ".env.local", "-f", "compose.local.yaml"]


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


def iniciar(compose, servicio, archivo_env, variable):
    if servicio == "couchdb-local" and not (RAIZ / ".env").is_file():
        raise RuntimeError("Configura primero CouchDB central para crear .env.")

    preparar_entorno(archivo_env, variable)
    puerto, base, credencial = parametros(servicios_compose(compose)[servicio])

    subprocess.run([*compose, "up", "-d", "--build", servicio], cwd=RAIZ, check=True)
    esperar_couchdb(puerto)

    codigo, cuerpo = solicitud(puerto, "PUT", f"/{base}", credencial)
    if codigo not in (201, 202) and not (
        codigo == 412 and json.loads(cuerpo).get("error") == "file_exists"
    ):
        raise RuntimeError(f"CouchDB respondió HTTP {codigo} al preparar la base.")

    instancia = "CouchDB local" if servicio == "couchdb-local" else "CouchDB"
    verificar_base(puerto, base, credencial, instancia)

    codigo, _ = solicitud(puerto, "GET", f"/{base}")
    if codigo not in (401, 403):
        raise RuntimeError(f"La base respondió HTTP {codigo} a una consulta sin credenciales.")

    print(f"{instancia} listo. Base: {base}. Consulta autenticada: HTTP 200.")
    print(f"Fauxton: http://127.0.0.1:{puerto}/_utils/")


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


def replicar_prueba():
    servicios = servicios_compose([*COMPOSE, *LOCAL])
    local = servicios["couchdb-local"]
    central = servicios["couchdb"]
    puerto_local, base_local, credencial_local = parametros(local)
    puerto_central, base_central, credencial_central = parametros(central)
    if puerto_local == puerto_central:
        raise RuntimeError("Las dos instancias deben usar puertos publicados distintos.")
    verificar_base(puerto_local, base_local, credencial_local, "CouchDB local")
    verificar_base(puerto_central, base_central, credencial_central, "CouchDB central")

    op_id = str(uuid.uuid4())
    doc_id = f"prueba-replicacion-{op_id}"
    ruta_local = f"/{base_local}/{doc_id}"
    ruta_central = f"/{base_central}/{doc_id}"
    documento = {"tipo": "prueba_replicacion", "op_id": op_id}
    codigo, cuerpo = solicitud(
        puerto_local, "PUT", ruta_local, credencial_local, datos=documento
    )
    if codigo not in (201, 202):
        raise RuntimeError(f"Falló la escritura del documento local (HTTP {codigo}).")
    guardado = json.loads(cuerpo)
    revision = guardado.get("rev")
    if guardado.get("ok") is not True or guardado.get("id") != doc_id or not revision:
        raise RuntimeError("CouchDB local no confirmó el ID y la revisión del documento.")
    print(f"Local: documento creado con HTTP {codigo}.", flush=True)

    codigo, _ = solicitud(puerto_central, "GET", ruta_central, credencial_central)
    if codigo != 404:
        raise RuntimeError(f"CouchDB central debía devolver HTTP 404 y devolvió {codigo}.")
    print(f"Central antes de replicar: HTTP 404 para {doc_id}.", flush=True)

    codigo, cuerpo = solicitud(
        puerto_local,
        "POST",
        "/_replicate",
        credencial_local,
        datos={
            "source": extremo_replicacion("couchdb-local", base_local, local),
            "target": extremo_replicacion("couchdb", base_central, central),
            "doc_ids": [doc_id],
            "continuous": False,
            "create_target": False,
        },
        timeout=45,
    )
    if codigo != 200 or json.loads(cuerpo).get("ok") is not True:
        raise RuntimeError(f"CouchDB no confirmó la replicación (HTTP {codigo}).")

    codigo, cuerpo = solicitud(puerto_central, "GET", ruta_central, credencial_central)
    esperado = {**documento, "_id": doc_id, "_rev": revision}
    if codigo != 200 or json.loads(cuerpo) != esperado:
        raise RuntimeError(f"El documento central no coincide con el original (HTTP {codigo}).")
    print("Central después de replicar: HTTP 200, mismo ID, revisión y contenido.")
    print("Prueba de replicación local → central completada.")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    modos = parser.add_mutually_exclusive_group()
    modos.add_argument("--local", action="store_true", help="Inicia la instancia del laboratorio.")
    modos.add_argument("--replicar-prueba", action="store_true", help="Replica un documento ficticio.")
    args = parser.parse_args()

    if args.local:
        compose, servicio = [*COMPOSE, *LOCAL], "couchdb-local"
        archivo_env, variable = ".env.local", "COUCHDB_LOCAL_PASSWORD"
    else:
        compose, servicio = COMPOSE, "couchdb"
        archivo_env, variable = ".env", "COUCHDB_PASSWORD"

    try:
        if args.replicar_prueba:
            replicar_prueba()
        else:
            iniciar(compose, servicio, archivo_env, variable)
    except FileNotFoundError as error:
        if error.filename == "docker":
            print("Docker no está disponible en esta terminal.", file=sys.stderr)
        else:
            print(f"Falta un archivo requerido: {error.filename}", file=sys.stderr)
        sys.exit(1)
    except subprocess.CalledProcessError:
        print(
            "Falló Docker Compose. Revisa el mensaje de Docker.",
            file=sys.stderr,
        )
        sys.exit(1)
    except (OSError, RuntimeError, ValueError, http.client.HTTPException) as error:
        print(f"No se pudo verificar CouchDB. {error}", file=sys.stderr)
        sys.exit(1)


if __name__ == "__main__":
    main()
