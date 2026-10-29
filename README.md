# OasisCom XML Importer (Rust)

Implementación en Rust de `SPEC-oasiscom-importer.md` (versión 0.1.0).

Detecta XML de carga en una carpeta, obtiene un token de OasisCom, envía cada XML en Base64 y registra el resultado en un historial JSON Lines.

## Requisitos

- Rust estable (edición 2021). No requiere OpenSSL: usa `rustls`.

## Compilar y probar

```bash
cargo build --release
cargo test
```

El binario queda en `target/release/oasis`.

## Uso

```bash
# Con IS_MOCK=true no se llama a OasisCom
cp .env.example .env

# Vigilar ./inbox (o procesar un lote si TRIGGER_WATCH=false)
./target/release/oasis

# Comandos
./target/release/oasis watch
./target/release/oasis send examples/ejemplo.xml
./target/release/oasis token
./target/release/oasis status
./target/release/oasis status --json --estado FALLIDO --limit 0
./target/release/oasis version
```

Códigos de salida: `0` éxito, `1` fallo operativo, `2` error de configuración o uso.

## Configurar variables

```bash
./target/release/oasis config                      # menú interactivo
./target/release/oasis config list                 # valores efectivos
./target/release/oasis config get WATCH_DIR        # una variable
./target/release/oasis config set WATCH_DIR /datos/inbox
./target/release/oasis config unset WATCH_DIR      # vuelve al valor por defecto
```

`config` escribe en el `.env` del directorio actual (o junto al ejecutable si no hay uno local), conserva comentarios y crea el archivo con permisos `0600` en Unix. Si el `.env` no existe, lo crea con los valores por defecto (mismo contenido que `.env.example`) antes de continuar. En una terminal interactiva muestra todas las variables y se navega con `↑`/`↓` y `Enter`: los booleanos y las opciones se eligen de la misma forma, y los textos, enteros y duraciones se escriben directamente. La lista se pagina si no cabe en la pantalla. Si no hay terminal (por ejemplo desde un script), se usa un menú numérico equivalente.

Las variables del entorno del proceso siempre tienen prioridad sobre `.env`; si una variable está definida en el entorno, el comando lo advierte.

## Logs

Los logs se escriben en `stderr` y también en `LOG_FILE` (por defecto `./logs/oasis.log`). En una terminal usan colores y emoticones por nivel; el archivo `.log` es texto plano sin códigos ANSI. `NO_COLOR=1` desactiva los estilos. Un `LOG_FILE` vacío desactiva el archivo.

## Configuración

Se lee `.env` del directorio actual (o junto al ejecutable) sin sobrescribir variables de entorno. Todas las variables y valores por defecto están en `.env.example`; los detalles de comportamiento están en la especificación.

En modo real (`IS_MOCK=false`) son obligatorias: `OASIS_CONTEXT`, `OASIS_USERNAME`, `OASIS_PASSWORD`, `OASIS_COMPANY_CODE` y `OASIS_PUBLICATION_CODE`.

## Estructura

| Archivo | Responsabilidad |
|---|---|
| `src/config.rs` | `.env`, variables, tipos, derivaciones y validación |
| `src/client.rs` | Token, caché, reintentos, multipart/urlencoded y mock |
| `src/xml.rs` | Extracción best-effort de empresa y documentos |
| `src/processor.rs` | Hash, tamaño, envío, evento y movimiento |
| `src/status.rs` | Historial JSONL, resumen y lectura tolerante |
| `src/watcher.rs` | Sondeo de carpeta y estabilidad |
| `src/config_cmd.rs` | Comando `config` (TUI y subcomandos) |
| `src/logger.rs` | Logs a terminal y archivo |
| `src/cli.rs` | Comando `status` y ayuda |
| `src/main.rs` | Comandos, códigos de salida y señales |

## Seguridad

No se registran contraseñas, client secrets ni tokens completos. `.env`, `status/`, `inbox/*.xml`, `processed/` y `failed/` están en `.gitignore`.
