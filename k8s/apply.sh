#!/usr/bin/env bash
# Applies the whole deployment: the namespace, then `encarta/`, then the ingress with the public
# hostname substituted in. The hostname is never in git: it comes from `$ENCARTA_HOST`, or else the
# first line of the gitignored `host.local` beside this script.
#
#   ENCARTA_HOST=encarta.example.com ./apply.sh
#   ./apply.sh --dry-run          # server-side dry run of everything; changes nothing
set -euo pipefail

here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
NS=encarta

dry=()
case "${1:-}" in
  --dry-run) dry=(--dry-run=server) ;;
  "") ;;
  *) echo "usage: $0 [--dry-run]" >&2; exit 2 ;;
esac

host=${ENCARTA_HOST:-}
if [[ -z "$host" && -f "$here/host.local" ]]; then
  host=$(grep -v '^[[:space:]]*\(#\|$\)' "$here/host.local" | head -n1 | tr -d '[:space:]')
fi
if [[ -z "$host" ]]; then
  echo "No hostname: set ENCARTA_HOST, or put it in $here/host.local (gitignored)." >&2
  exit 1
fi
if ! [[ "$host" =~ ^[a-z0-9]([a-z0-9-]*[a-z0-9])?(\.[a-z0-9]([a-z0-9-]*[a-z0-9])?)+$ ]]; then
  echo "Not a hostname: '$host'" >&2
  exit 1
fi

kubectl apply "${dry[@]}" -f "$here/namespace.yml"

# A server-side dry run of a namespaced object in a namespace that does not exist yet is refused.
if [[ ${#dry[@]} -gt 0 ]] && ! kubectl get namespace "$NS" >/dev/null 2>&1; then
  echo "namespace/$NS does not exist yet, so the rest cannot be dry-run; apply without --dry-run first." >&2
  exit 0
fi

kubectl apply "${dry[@]}" -f "$here/encarta/"
# Only the literal `${ENCARTA_HOST}` is replaced; `host` is validated above, so it is sed-safe.
sed "s/\${ENCARTA_HOST}/$host/g" "$here/ingress.template.yml" | kubectl apply "${dry[@]}" -f -
