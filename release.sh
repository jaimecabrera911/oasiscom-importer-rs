#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")"

if ! git rev-parse --is-inside-work-tree >/dev/null 2>&1; then
    echo "Error: esta carpeta no es un repositorio git." >&2
    exit 1
fi

read -rp "Mensaje del commit: " commit_msg
if [[ -z "${commit_msg// /}" ]]; then
    echo "Error: el mensaje del commit no puede estar vacío." >&2
    exit 1
fi

git add .
if git diff --cached --quiet; then
    echo "No hay cambios nuevos; se publicará el commit actual."
else
    git commit -m "$commit_msg"
fi

git push origin master

while true; do
    read -rp "Versión/tag a publicar (ej. v0.1.3): " tag
    if [[ "$tag" != v* ]]; then
        echo "El tag debe empezar con 'v' para disparar el workflow." >&2
        continue
    fi
    if git rev-parse -q --verify "refs/tags/$tag" >/dev/null; then
        echo "El tag $tag ya existe localmente." >&2
        continue
    fi
    if git ls-remote --exit-code --tags origin "refs/tags/$tag" >/dev/null 2>&1; then
        echo "El tag $tag ya existe en origin." >&2
        continue
    fi
    break
done

git tag "$tag"
git push origin "$tag"

echo "Publicado $tag. Revisa la pestaña Actions y luego Releases en GitHub."
