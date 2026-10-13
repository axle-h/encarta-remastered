#!/usr/bin/env bash
# Mirrors the local converter output (`out/`) onto the `data` volume: rsync, tunnelled through
# `kubectl exec` into a throwaway loader pod (`loader.yml`) that mounts the volume read-write. No
# node SSH, and incremental: a re-run sends only what changed, and deletes what `out/` no longer has.
#
#   ./sync-data.sh                # from ../out
#   ./sync-data.sh /path/to/out
#
# Run it while the converter is not writing: a database copied mid-write is copied inconsistent.
# Interrupted, it leaves the volume's files whole (partial transfers wait in `.rsync-partial/`) and
# a re-run resumes. If the server is running it is restarted afterwards, so it opens the new files;
# NO_RESTART=1 skips that.
set -euo pipefail

NS=${NS:-encarta}
POD=data-loader

# rsync runs `<rsh> <host> rsync --server …`; this re-entry turns that into `kubectl exec`. Over
# SPDY, not kubectl's default websockets: on a sustained upload the websocket transport's pings
# time out behind the data, and kubectl drops stdin a few hundred MB in.
if [[ "${1:-}" == --rsh ]]; then
  pod=$2
  shift 2
  KUBECTL_REMOTE_COMMAND_WEBSOCKETS=false exec kubectl exec -i -n "$NS" "$pod" -- "$@"
fi

here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
src=$(cd "${1:-$here/../out}" && pwd)

if [[ ! -f "$src/content.sqlite" ]]; then
  echo "$src has no content.sqlite: is it the converter's out/?" >&2
  exit 1
fi
command -v rsync >/dev/null || { echo "rsync is not installed locally" >&2; exit 1; }

cleanup() {
  kubectl -n "$NS" delete pod "$POD" --ignore-not-found --wait=false >/dev/null || true
}

# A loader left over from an interrupted run is replaced, not reused: it may be on an old spec.
kubectl -n "$NS" delete pod "$POD" --ignore-not-found --wait=true
trap cleanup EXIT
kubectl apply -f "$here/loader.yml"
# The first run also binds the volume (`WaitForFirstConsumer`), and the pod installs rsync.
kubectl -n "$NS" wait --for=condition=Ready "pod/$POD" --timeout=300s

echo "syncing $src/ ($(du -sh "$src" | cut -f1)) to $NS/pvc/data"
# -rlt and not -a: owners are meaningless on the volume, and the server only needs to read, hence
# --chmod. --delete-after removes files only once everything new has arrived.
rsync -rlt --chmod=D755,F644 \
  --delete-after --partial-dir=.rsync-partial \
  --no-inc-recursive --info=progress2,stats1 --human-readable \
  --blocking-io -e "$here/sync-data.sh --rsh" \
  "$src/" "$POD:/data/"

kubectl -n "$NS" exec "$POD" -- df -h /data

if [[ -z "${NO_RESTART:-}" ]] \
  && [[ "$(kubectl -n "$NS" get deploy encarta -o jsonpath='{.status.readyReplicas}' 2>/dev/null)" -ge 1 ]] 2>/dev/null; then
  kubectl -n "$NS" rollout restart deploy/encarta
fi
