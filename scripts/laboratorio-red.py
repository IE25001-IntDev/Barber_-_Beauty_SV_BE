#!/usr/bin/env python3
"""Prepara y ejecuta el laboratorio de red BAD 215 en un proyecto Compose aislado."""

import argparse
import csv
import fcntl
import hashlib
import http.client
import json
import random
import shlex
import subprocess
import sys
import time
import uuid
from collections import Counter
from contextlib import contextmanager
from datetime import datetime, timedelta, timezone
from pathlib import Path

from couchdb_comun import (
    RAIZ, esperar_couchdb, extremo_replicacion, parametros, preparar_entorno,
    servicios_compose, solicitud, verificar_base,
)

PROYECTO = "bad215-laboratorio-red"
COMPOSE = [
    "docker", "compose", "-p", PROYECTO, "--env-file", ".env", "--env-file",
    ".env.local", "-f", "compose.yaml", "-f", "compose.local.yaml",
    "-f", "compose.laboratorio.yaml",
]
BASE_CITAS = "bad215_lab_citas"
BASE_OPS = "bad215_lab_operaciones"
RESULTADOS = RAIZ / "resultados/bad215-lab"
ENLACES = ("enlace-api-lab", "enlace-replica-lab")
PERFILES = {
    "E0": [],
    "E1": ["delay", "50ms", "loss", "1%", "rate", "10mbit"],
    "E2": ["delay", "150ms", "loss", "5%", "rate", "2mbit"],
    "E3": ["delay", "300ms", "loss", "10%", "rate", "512kbit"],
    "E4": ["loss", "100%"],
}
FECHA = datetime(2026, 11, 20, 14, tzinfo=timezone.utc)
SEMILLA = 42
ERRORES_HTTP = (OSError, http.client.HTTPException)


def ahora_ms():
    return time.time_ns() // 1_000_000


def ejecutar(*args, capturar=False):
    return subprocess.run(
        [*COMPOSE, *args], cwd=RAIZ, check=True,
        stdout=subprocess.PIPE if capturar else None, text=True,
        timeout=20 if capturar else None,
    ).stdout


def escribir_log(archivo, evento, detalle=""):
    marca = ahora_ms()
    fecha = datetime.fromtimestamp(marca / 1000, timezone.utc)
    with archivo.open("a", encoding="utf-8") as salida:
        print(f"{marca}\t{fecha.isoformat()}\t{evento}\t{detalle}", file=salida)
    return marca


def tc(archivo, enlace, *args):
    comando = ("exec", "-T", enlace, "tc", *args)
    escribir_log(archivo, "comando", shlex.join([*COMPOSE, *comando]))
    resultado = ejecutar(*comando, capturar=True).strip()
    if resultado:
        escribir_log(archivo, "qdisc", f"{enlace} {resultado}")
    return resultado


def tiene_netem(salida):
    return any(linea.startswith("qdisc netem ") and " root " in linea for linea in salida.splitlines())


def retirar_netem(archivo, enlace):
    if tiene_netem(tc(archivo, enlace, "qdisc", "show", "dev", "eth0")):
        tc(archivo, enlace, "qdisc", "del", "dev", "eth0", "root")


def aplicar(perfil, enlaces, archivo):
    opciones = PERFILES[perfil]
    for enlace in enlaces:
        if opciones:
            tc(archivo, enlace, "qdisc", "replace", "dev", "eth0", "root", "netem", *opciones)
        else:
            retirar_netem(archivo, enlace)

    inicio = time.monotonic()
    marca = escribir_log(archivo, "t_interrupcion" if perfil == "E4" else "perfil_activo", perfil)
    for enlace in enlaces:
        if tiene_netem(tc(archivo, enlace, "qdisc", "show", "dev", "eth0")) != bool(opciones):
            raise RuntimeError(f"No se confirmó {perfil} en {enlace}.")
    print(f"Perfil {perfil} verificado en {', '.join(enlaces)}.", flush=True)
    return marca, inicio


def restaurar_red(enlaces, archivo):
    errores = []
    for enlace in enlaces:
        try:
            retirar_netem(archivo, enlace)
        except (OSError, RuntimeError, subprocess.SubprocessError) as error:
            errores.append(enlace)
            escribir_log(archivo, "error_reversion", f"{enlace} {error}")
    if errores:
        raise RuntimeError(f"Ejecuta perfil E0 para recuperar: {', '.join(errores)}.")
    marca = escribir_log(archivo, "t_restablecimiento", ",".join(enlaces))
    for enlace in enlaces:
        if tiene_netem(tc(archivo, enlace, "qdisc", "show", "dev", "eth0")):
            raise RuntimeError(f"Sigue activo netem en {enlace}. Ejecuta perfil E0.")
    return marca


def peticion(instancia, metodo, ruta="", *, datos=None, timeout=5):
    puerto, base, credencial = instancia
    codigo, cuerpo = solicitud(puerto, metodo, f"/{base}{ruta}", credencial, datos=datos, timeout=timeout)
    return codigo, json.loads(cuerpo)


def exigir(codigo, esperados, contexto):
    if codigo not in esperados:
        raise RuntimeError(f"{contexto} devolvió HTTP {codigo}; se esperaba {esperados}.")


def carga_semilla():
    azar = random.Random(SEMILLA)
    empleados = [
        {"_id": f"empleado-{i:02}", "tipo": "empleado", "nombre": f"Empleado ficticio {i}", "activo": True}
        for i in range(1, 11)
    ]
    servicios = [
        {"_id": f"servicio-{i:02}", "tipo": "servicio", "nombre": f"Servicio ficticio {i}",
         "precio_centavos": azar.randrange(500, 1600, 100), "duracion_minutos": 30, "activo": True}
        for i in range(1, 6)
    ]
    clientes = [
        {"_id": f"cliente-{i:03}", "tipo": "cliente", "nombre": f"Cliente ficticio {i}"}
        for i in range(1, 121)
    ]
    citas = []
    for i, cliente in enumerate(clientes, start=1):
        servicio = azar.choice(servicios)
        citas.append({
            "_id": f"cita-{i:03}", "tipo": "cita", "cliente_nombre": cliente["nombre"],
            "empleado_id": empleados[(i - 1) % len(empleados)]["_id"], "servicio_id": servicio["_id"],
            "inicio": (FECHA + timedelta(minutes=30 * ((i - 1) // len(empleados)))).isoformat(),
            "estado": "pendiente", "precio_centavos": servicio["precio_centavos"],
        })
    return empleados + servicios + clientes + citas


def checksum(documento):
    cuerpo = {k: v for k, v in documento.items() if k not in ("_rev", "_conflicts")}
    canonico = json.dumps(cuerpo, sort_keys=True, ensure_ascii=False, separators=(",", ":"))
    return hashlib.sha256(canonico.encode("utf-8")).hexdigest()


def restaurar_datos(origen, destino):
    semilla = carga_semilla()
    for nombre, instancia in (("local", origen), ("central", destino)):
        for base in (BASE_CITAS, BASE_OPS):
            db = (instancia[0], base, instancia[2])
            exigir(peticion(db, "DELETE")[0], (200, 202, 404), f"Borrar {base} en {nombre}")
            exigir(peticion(db, "PUT")[0], (201, 202), f"Crear {base} en {nombre}")
            docs = semilla if base == BASE_CITAS else []
            if docs:
                codigo, filas = peticion(db, "POST", "/_bulk_docs", datos={"docs": docs})
                exigir(codigo, (201, 202), f"Cargar {base} en {nombre}")
                if len(filas) != len(docs) or any(not fila.get("ok") for fila in filas):
                    raise RuntimeError(f"CouchDB no guardó todos los documentos semilla en {nombre}.")
            codigo, estado = peticion(db, "GET")
            exigir(codigo, (200,), f"Comprobar {base} en {nombre}")
            if estado["doc_count"] != len(docs):
                raise RuntimeError(f"Conteo incorrecto en {base} ({nombre}).")
            print(f"{nombre} / {base}: {len(docs)} documentos.", flush=True)

    huella = checksum({"documentos": semilla})
    print(f"Semilla {SEMILLA}, fecha {FECHA.date()}, SHA-256 {huella}.", flush=True)


def puerto(servicios, nombre):
    return int(servicios[nombre]["ports"][0]["published"])


def verificar_api(puerto_api, *, timeout=5):
    codigo, cuerpo = solicitud(puerto_api, "GET", "/api/ready", timeout=timeout)
    exigir(codigo, (200,), "API /api/ready")
    if json.loads(cuerpo).get("status") != "ok":
        raise RuntimeError("La API no confirmó su conexión con CouchDB.")


def calibrar(servicios, archivo):
    directo = puerto(servicios, "api-laboratorio")
    enlace = puerto(servicios, ENLACES[0])
    verificar_api(directo)
    for muestra in range(1, 6):
        inicio = time.monotonic()
        try:
            verificar_api(enlace)
            estado = "ok"
        except (*ERRORES_HTTP, RuntimeError):
            estado = "error"
        ms = round((time.monotonic() - inicio) * 1000, 2)
        escribir_log(archivo, "calibracion_http", f"{muestra} {estado} {ms} ms")
        print(f"Consulta /api/ready por el enlace {muestra}: {estado}, {ms} ms.", flush=True)


@contextmanager
def replicacion(origen, servicios, ensayo):
    trabajo = {
        "source": extremo_replicacion("couchdb-local", BASE_OPS, servicios["couchdb-local"]),
        "target": extremo_replicacion(ENLACES[1], BASE_OPS, servicios["couchdb"]),
        "selector": {"ensayo": ensayo},
        "continuous": True,
        "create_target": False,
        "connection_timeout": 5000,
        "retries_per_request": 20,
    }
    puerto_local, _, credencial = origen
    try:
        codigo, cuerpo = solicitud(puerto_local, "POST", "/_replicate", credencial, datos=trabajo, timeout=15)
        exigir(codigo, (202,), "Replicación continua")
        if json.loads(cuerpo).get("ok") is not True:
            raise RuntimeError("CouchDB local no aceptó la replicación.")
        yield
    finally:
        error_en_curso = sys.exc_info()[0] is not None
        try:
            codigo, _ = solicitud(
                puerto_local, "POST", "/_replicate", credencial,
                datos={**trabajo, "cancel": True}, timeout=15,
            )
            exigir(codigo, (200, 404), "Cancelar replicación")
        except (*ERRORES_HTTP, RuntimeError) as error:
            if not error_en_curso:
                raise
            print(f"Cancelación pendiente. {error}", file=sys.stderr)


def operacion(ensayo, vu, iteracion, fase):
    op_id = str(uuid.uuid4())
    creacion = (vu + iteracion) % 5 < 3
    cita_id = str(uuid.uuid4()) if creacion else f"cita-{(vu + iteracion - 1) % 120 + 1:03}"
    cuerpo = {
        "id": cita_id, "cliente_nombre": f"Cliente ficticio VU {vu}",
        "empleado_id": f"empleado-{(vu - 1) % 10 + 1:02}", "servicio_id": "servicio-01",
        "inicio": (FECHA + timedelta(days=1, minutes=30 * (iteracion * 100 + vu))).isoformat(),
    } if creacion else {"estado": "completada"}
    return {
        "_id": op_id, "op_id": op_id, "ensayo": ensayo, "ts_cliente": ahora_ms(),
        "tipo": "operacion", "operacion": "OP2" if creacion else "OP3",
        "vu": vu, "iteracion": iteracion, "fase": fase, "cita_id": cita_id, "cuerpo": cuerpo,
    }


def guardar_lote(origen, documentos):
    try:
        codigo, filas = peticion(origen, "POST", "/_bulk_docs", datos={"docs": documentos})
        exigir(codigo, (201, 202), "Escritura de operaciones locales")
        if not isinstance(filas, list) or len(filas) != len(documentos):
            raise RuntimeError("CouchDB devolvió un lote con longitud inesperada.")
        revisiones = {fila["id"]: fila["rev"] for fila in filas if fila.get("ok") and fila.get("rev")}
        return revisiones
    except (*ERRORES_HTTP, RuntimeError, ValueError) as error:
        print(f"Escritura local sin confirmación. {error}", file=sys.stderr)
        return {}


def leer_lote(instancia, ids):
    for inicio in range(0, len(ids), 250):
        codigo, datos = peticion(
            instancia, "POST", "/_all_docs?include_docs=true",
            datos={"keys": ids[inicio:inicio + 250]},
        )
        exigir(codigo, (200,), "Lectura de operaciones por ID")
        for fila in datos["rows"]:
            if fila.get("doc"):
                yield fila["doc"]


def operaciones_centrales(instancia, ensayo):
    consulta = {"selector": {"ensayo": ensayo, "tipo": "operacion"}, "conflicts": True, "limit": 250}
    while True:
        codigo, datos = peticion(instancia, "POST", "/_find", datos=consulta, timeout=15)
        exigir(codigo, (200,), "Verificación final de conflictos")
        yield from datos["docs"]
        if len(datos["docs"]) < consulta["limit"]:
            return
        if datos["bookmark"] == consulta.get("bookmark"):
            raise RuntimeError("La consulta de operaciones no avanzó.")
        consulta["bookmark"] = datos["bookmark"]


def exportar_json(archivo, datos):
    archivo.write_text(json.dumps(datos, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")


def esperar_hasta(instante):
    time.sleep(max(0, instante - time.monotonic()))


def simular(args, servicios, origen, destino):
    ensayo = str(uuid.uuid4())
    carpeta = args.salida or RESULTADOS / "distribuida/E4" / f"{args.usuarios}vu" / ensayo
    carpeta.mkdir(parents=True, exist_ok=True)
    for nombre in ("ids.csv", "verificacion.json", "tc.log", "entorno.txt"):
        if (carpeta / nombre).exists():
            raise RuntimeError(f"Ya existe {nombre} en {carpeta}. Usa una carpeta nueva.")
    print(f"Evidencia: {carpeta}", flush=True)
    archivo = carpeta / "tc.log"
    verificar_base(*origen, "CouchDB local del laboratorio")
    verificar_base(*destino, "CouchDB central del laboratorio")
    directo = puerto(servicios, "api-laboratorio")
    enlace = puerto(servicios, ENLACES[0])
    aplicar("E0", ENLACES, archivo)
    verificar_api(directo)
    verificar_api(enlace)
    commit = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=RAIZ, text=True).strip()
    estado = subprocess.check_output(["git", "status", "--porcelain"], cwd=RAIZ, text=True)
    compose_version = ejecutar("version", "--short", capturar=True).strip()
    with (carpeta / "entorno.txt").open("x", encoding="utf-8") as entorno:
        print(f"commit={commit} cambios_locales={bool(estado.strip())}", file=entorno)
        print(f"compose={compose_version} python={sys.version.split()[0]}", file=entorno)
        print(f"semilla={SEMILLA} fecha_citas={FECHA.date()}", file=entorno)
        print(f"usuarios={args.usuarios} antes={args.antes} corte={args.corte} despues={args.despues} espera={args.espera}", file=entorno)
        for nombre, servicio in servicios.items():
            if nombre in ("couchdb", "couchdb-local", "api-laboratorio", *ENLACES):
                print(f"{nombre} memoria_bytes={servicio.get('mem_limit')} cpus={servicio.get('cpus', 'sin límite')}", file=entorno)

    registros = {}
    pendientes = {}
    conflictos = set()
    diferencias = set()
    duplicadas = set()
    centrales_sin_intento = set()
    marcas = {}
    llego_durante_corte = set()

    with (carpeta / "ids.csv").open("x", encoding="utf-8", newline="") as salida:
        csv_ops = csv.DictWriter(salida, fieldnames=[
            "op_id", "ts_cliente", "vu", "iteracion", "operacion", "cita_id",
            "fase", "estado", "ts_estado", "sha256",
        ], extrasaction="ignore")
        csv_ops.writeheader()

        def registrar(operacion, estado):
            marca = ahora_ms()
            csv_ops.writerow({**operacion, "estado": estado, "ts_estado": marca, "sha256": checksum(operacion)})
            return marca

        def confirmar():
            for doc in leer_lote(destino, list(pendientes)):
                op_id = doc["_id"]
                esperado = pendientes[op_id]
                if checksum(doc) != checksum(esperado) or doc.get("_rev") != esperado["_rev"]:
                    diferencias.add(op_id)
                else:
                    instante = registrar(esperado, "confirmada_servidor")
                    registros[op_id]["ts_central"] = instante
                    del pendientes[op_id]
            salida.flush()

        def fase(nombre, segundos, iteracion, inicio=None):
            if inicio is None:
                inicio = time.monotonic()
            print(f"Fase {nombre}: {segundos} s, {args.usuarios} operaciones por segundo.", flush=True)
            for paso in range(segundos):
                esperar_hasta(inicio + paso)
                documentos = [operacion(ensayo, vu, iteracion + paso, nombre)
                              for vu in range(1, args.usuarios + 1)]
                for doc in documentos:
                    registros[doc["_id"]] = {"doc": doc, "revision": None, "ts_central": None}
                    registrar(doc, "intentada")
                salida.flush()
                revisiones = guardar_lote(origen, documentos)
                for doc in documentos:
                    op_id = doc["_id"]
                    revision = revisiones.get(op_id)
                    registros[op_id]["revision"] = revision
                    registrar(doc, "local" if revision else "sin_confirmacion_local")
                    if revision:
                        pendientes[op_id] = {**doc, "_rev": revision}
                salida.flush()
                if nombre == "durante":
                    llego_durante_corte.update(
                        doc["_id"] for doc in leer_lote(destino, list(revisiones))
                    )
                else:
                    confirmar()
            esperar_hasta(inicio + segundos)

        try:
            with replicacion(origen, servicios, ensayo):
                # Un documento real confirma que la replicación está activa antes del primer minuto.
                control = {"_id": f"control-{ensayo}", "ensayo": ensayo, "tipo": "control"}
                revisiones = guardar_lote(origen, [control])
                if control["_id"] not in revisiones:
                    raise RuntimeError("No se guardó el control local.")
                limite = time.monotonic() + 45
                while not list(leer_lote(destino, [control["_id"]])):
                    if time.monotonic() >= limite:
                        raise RuntimeError("La replicación inicial no llegó a central en 45 segundos.")
                    time.sleep(0.5)

                marcas["t_inicio"] = escribir_log(archivo, "inicio_carga", ensayo)
                fase("antes", args.antes, 0)
                if pendientes:
                    confirmar()
                try:
                    marcas["t_interrupcion"], inicio_corte = aplicar("E4", ENLACES, archivo)
                    verificar_api(directo)
                    try:
                        verificar_api(enlace, timeout=0.6)
                    except ERRORES_HTTP:
                        escribir_log(archivo, "api_interrumpida", "directa HTTP 200")
                    else:
                        raise RuntimeError("La API respondió por el enlace durante E4.")
                    fase("durante", args.corte, args.antes, inicio_corte)
                finally:
                    marcas["t_restablecimiento"] = restaurar_red(ENLACES, archivo)
                corte_ms = marcas["t_restablecimiento"] - marcas["t_interrupcion"]
                escribir_log(archivo, "corte_medido", f"{corte_ms} ms")
                # Una retransmisión previa al restablecimiento puede retrasar la primera consulta.
                limite_api = time.monotonic() + 15
                while True:
                    try:
                        verificar_api(enlace, timeout=2)
                        break
                    except ERRORES_HTTP:
                        if time.monotonic() >= limite_api:
                            raise
                        time.sleep(0.2)
                marcas["t_api_recuperada"] = escribir_log(archivo, "api_recuperada", "HTTP 200")
                fase("despues", args.despues, args.antes + args.corte)
                limite = time.monotonic() + args.espera
                while pendientes and time.monotonic() < limite:
                    confirmar()
                    if pendientes:
                        esperar_hasta(min(limite, time.monotonic() + 0.5))

                conteo_central = Counter()
                for doc in operaciones_centrales(destino, ensayo):
                    op_id = doc["op_id"]
                    conteo_central[op_id] += 1
                    if doc.get("_conflicts"):
                        conflictos.add(op_id)
                    if op_id not in registros:
                        centrales_sin_intento.add(op_id)
                        continue
                    esperado = registros[op_id]
                    if checksum(doc) != checksum(esperado["doc"]) or doc["_rev"] != esperado["revision"]:
                        diferencias.add(op_id)
                duplicadas.update(op for op, cantidad in conteo_central.items() if cantidad > 1)
                # El conteo final refleja el destino, incluso si cambió después de una confirmación.
                inconsistentes = conflictos | diferencias | duplicadas
                for op_id, fila in registros.items():
                    if op_id in inconsistentes or conteo_central[op_id] == 0:
                        if fila["revision"]:
                            pendientes[op_id] = fila["doc"]
                        fila["ts_central"] = None
        finally:
            # La evidencia parcial también queda disponible si se interrumpe el ensayo.
            fases = {}
            for nombre in ("antes", "durante", "despues"):
                filas = [r for r in registros.values() if r["doc"]["fase"] == nombre]
                fases[nombre] = {
                    "intentadas": len(filas),
                    "confirmadas_localmente": sum(bool(r["revision"]) for r in filas),
                    "confirmadas_servidor": sum(r["ts_central"] is not None for r in filas),
                }
            durante = [r for r in registros.values() if r["doc"]["fase"] == "durante"]
            confirmaciones = [r["ts_central"] for r in durante if r["ts_central"] is not None]
            if durante and len(confirmaciones) == len(durante) and "t_restablecimiento" in marcas:
                marcas["t_sync_fin"] = max(confirmaciones)
            restablecido = marcas.get("t_restablecimiento")
            reanudacion = min((r["ts_central"] for r in registros.values()
                               if r["ts_central"] is not None and restablecido
                               and r["ts_central"] >= restablecido), default=None)
            if reanudacion is not None:
                marcas["t_reanudacion_servidor"] = reanudacion
            exportar_json(carpeta / "verificacion.json", {
                "ensayo": ensayo, "usuarios": args.usuarios, "semilla_datos": SEMILLA,
                "fases": fases, "marcas_utc_ms": marcas,
                "recuperacion_api_ms": marcas["t_api_recuperada"] - restablecido
                if "t_api_recuperada" in marcas else None,
                "sincronizacion_ms": marcas["t_sync_fin"] - restablecido
                if "t_sync_fin" in marcas else None,
                "omitidas": sorted(pendientes.keys() - conflictos - diferencias - duplicadas),
                "porcentaje_conservacion_corte": round(
                    100 * fases["durante"]["confirmadas_localmente"] / len(durante), 2
                ) if durante else None,
                "porcentaje_replicacion_corte": round(
                    100 * len(confirmaciones) / fases["durante"]["confirmadas_localmente"], 2
                ) if fases["durante"]["confirmadas_localmente"] else None,
                "sin_confirmacion_local": sorted(op for op, r in registros.items() if not r["revision"]),
                "duplicadas": sorted(duplicadas),
                "centrales_sin_intento": sorted(centrales_sin_intento),
                "en_conflicto": sorted(conflictos), "diferencias": sorted(diferencias),
                "presentes_en_central_durante_corte": sorted(llego_durante_corte),
            })

    fallos = pendientes or conflictos or diferencias or duplicadas or llego_durante_corte or centrales_sin_intento
    if fallos or any(not r["revision"] for r in registros.values()):
        raise RuntimeError(f"El ensayo registró operaciones pendientes o inconsistentes. Revisa {carpeta}.")
    print(f"Verificadas {len(registros)} operaciones; {len(durante)} se guardaron durante el corte.", flush=True)
    print(f"Cola del corte confirmada en central en {marcas['t_sync_fin'] - restablecido} ms.", flush=True)


def entero_positivo(valor):
    n = int(valor)
    if n < 1:
        raise argparse.ArgumentTypeError("debe ser mayor que cero")
    return n


def argumentos():
    parser = argparse.ArgumentParser(description=__doc__)
    modos = parser.add_subparsers(dest="modo", required=True)
    modos.add_parser("iniciar")
    modos.add_parser("restaurar")
    perfil = modos.add_parser("perfil")
    perfil.add_argument("escenario", choices=PERFILES)
    perfil.add_argument("--arquitectura", choices=("distribuida", "monolitica"), default="distribuida")
    perfil.add_argument("--calibrar", action="store_true")
    perfil.add_argument("--log", type=Path, default=RESULTADOS / "tc.log")
    perfil.add_argument("--corte", type=entero_positivo, default=60)

    offline = modos.add_parser("offline")
    offline.add_argument("--usuarios", type=int, choices=range(1, 101), metavar="1..100", default=1)
    offline.add_argument("--antes", type=entero_positivo, default=60)
    offline.add_argument("--corte", type=entero_positivo, default=60)
    offline.add_argument("--despues", type=entero_positivo, default=60)
    offline.add_argument("--espera", type=entero_positivo, default=120)
    offline.add_argument("--salida", type=Path)
    return parser.parse_args()


def ejecutar_laboratorio(args):
    if args.modo == "iniciar":
        binario = RAIZ / "target/debug/BD_Backend"
        if not binario.is_file():
            raise RuntimeError("Compila primero el binario Linux con cargo build --locked.")
        preparar_entorno(".env", "COUCHDB_PASSWORD")
        preparar_entorno(".env.local", "COUCHDB_LOCAL_PASSWORD")
    servicios = servicios_compose(COMPOSE)
    origen = parametros(servicios["couchdb-local"])
    destino = parametros(servicios["couchdb"])
    if origen[1] != BASE_CITAS or destino[1] != BASE_CITAS or origen[0] == destino[0]:
        raise RuntimeError("La configuración debe señalar las dos instancias aisladas del laboratorio.")

    if args.modo == "iniciar":
        ejecutar("up", "-d", "--build", "couchdb", "couchdb-local", "api-laboratorio", *ENLACES)
        for instancia in (origen, destino):
            esperar_couchdb(instancia[0])
        print(f"Laboratorio iniciado. BASE_URL=http://127.0.0.1:{puerto(servicios, ENLACES[0])}")
    elif args.modo == "restaurar":
        activos = ejecutar("ps", "--status", "running", "--services", capturar=True).splitlines()
        if not {"couchdb", "couchdb-local"}.issubset(activos):
            raise RuntimeError("Inicia las dos instancias del proyecto de laboratorio primero.")
        restaurar_datos(origen, destino)
    elif args.modo == "offline":
        simular(args, servicios, (origen[0], BASE_OPS, origen[2]), (destino[0], BASE_OPS, destino[2]))
    else:
        args.log.parent.mkdir(parents=True, exist_ok=True)
        activos = ENLACES if args.arquitectura == "distribuida" else ENLACES[:1]
        if args.arquitectura == "monolitica":
            aplicar("E0", ENLACES[1:], args.log)
        if args.escenario == "E4":
            try:
                _, inicio = aplicar("E4", activos, args.log)
                esperar_hasta(inicio + args.corte)
            finally:
                restaurar_red(activos, args.log)
        else:
            try:
                aplicar(args.escenario, activos, args.log)
                if args.calibrar:
                    calibrar(servicios, args.log)
            except BaseException:
                restaurar_red(activos, args.log)
                raise


def main():
    args = argumentos()
    RESULTADOS.mkdir(parents=True, exist_ok=True)
    try:
        with (RESULTADOS / ".lock").open("a") as bloqueo:
            try:
                fcntl.flock(bloqueo, fcntl.LOCK_EX | fcntl.LOCK_NB)
            except BlockingIOError as error:
                raise RuntimeError("Ya hay un comando del laboratorio en ejecución.") from error
            ejecutar_laboratorio(args)
    except KeyboardInterrupt:
        sys.exit(130)
    except (*ERRORES_HTTP, RuntimeError, ValueError, subprocess.SubprocessError) as error:
        print(f"Laboratorio: {error}", file=sys.stderr)
        sys.exit(1)


if __name__ == "__main__":
    main()
