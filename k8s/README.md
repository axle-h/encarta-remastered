# Kubernetes deployment

k3s. The ingress assumes traefik and cert-manager, and the volume assumes the `local-path`
provisioner. One pod: `encarta-server` serves the SPA baked into the image and reads the converter's
output from the `data` volume, mounted read-only at `/data`.

The data is not in the image or in git, because it is Microsoft material. It gets onto the volume
from a local `out/` with `sync-data.sh`.

The public hostname is not in git either. `apply.sh` reads it from `$ENCARTA_HOST`, or else from the
first line of `host.local` beside it, which is gitignored. It is substituted into
`ingress.template.yml`, which the API server rejects as it stands.

```shell
echo encarta.example.com > k8s/host.local     # once; or export ENCARTA_HOST
./k8s/apply.sh --dry-run                      # server-side dry run; changes nothing
./k8s/apply.sh                                # namespace.yml, then encarta/, then the ingress
./k8s/sync-data.sh                            # out/ → the volume
```

The namespace must be `encarta`. The ingress names the redirect middleware as
`encarta-redirect-http-https@kubernetescrd`, and traefik resolves that by namespace. The certificate
is `encarta-tls`, from the `letsencrypt-production` ClusterIssuer.

## The data

`sync-data.sh [OUT_DIR]` mirrors `out/` (the default) onto the volume. It starts `loader.yml`, an
alpine pod and the only thing that mounts the volume read-write, then runs rsync with `kubectl exec`
as its transport, and deletes the loader when it is done. No node SSH is needed.

A re-run sends only what changed, and deletes from the volume whatever `out/` no longer has. An
interrupted run leaves the volume's files whole: partial transfers wait in `/data/.rsync-partial/`,
and the next run picks them up. A loader left behind by a killed run is replaced at the next start.

Run it while the converter is not writing, because a database copied mid-write arrives inconsistent.
The server only opens files at start-up, so after a sync it restarts the Deployment if one is
running. `NO_RESTART=1` skips that.

The claim asks for 64Gi against an 11G `out/`, leaving room for the media and atlas output that is
still growing. `local-path` neither enforces the size nor expands a bound claim. Raising it means
deleting the claim and running a full sync again. Nothing is lost, since all of it comes from `out/`.

## The image

`ghcr.io/axle-h/encarta-remastered:latest` is published on every push to main, after a smoke test
proves the image starts on an empty `/data` and serves `/healthz` and the SPA. Every build is also
tagged with its commit, which is the tag to pin or roll back to. `imagePullPolicy: Always` means a
`rollout restart` picks up the newest build:

```shell
kubectl -n encarta rollout restart deploy/encarta
```

GHCR creates a package private regardless of the repository's visibility. The first CI push
succeeds, and the cluster then fails to pull with `denied` (ImagePullBackOff). Make the package
public once, in its own settings.

The container runs as uid 10001 on a read-only root filesystem, with `/tmp` an emptyDir. The server
reads `ENCARTA_OUT=/data`, `ENCARTA_STATIC=/app/web` and `ENCARTA_LISTEN=0.0.0.0:8080` (see
`docs/api.md`). Service links are off, so the `encarta` Service does not inject `ENCARTA_PORT=…`
into that same variable namespace.
