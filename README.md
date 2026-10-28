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
| `src/cli.rs` | Comando `status` y ayuda |
| `src/main.rs` | Comandos, códigos de salida y señales |

## Seguridad

No se registran contraseñas, client secrets ni tokens completos. `.env`, `status/`, `inbox/*.xml`, `processed/` y `failed/` están en `.gitignore`.
