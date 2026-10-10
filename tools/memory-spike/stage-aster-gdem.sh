#!/bin/sh
set -eu

if [ "$#" -eq 0 ]; then
  printf 'usage: stage-aster-gdem.sh DEM_DIRECTORY [DEM_DIRECTORY ...]\n' >&2
  exit 2
fi

repository=$(CDPATH='' cd -- "$(dirname -- "$0")/../.." && pwd)
# shellcheck source=tools/memory-spike/real/aster-gdem-tiles.env
. "${repository}/tools/memory-spike/real/aster-gdem-tiles.env"

archive=${ASTER_GDEM_ARCHIVE:-${repository}/target/aster-gdem-tiles/aster-gdem-tiles-003.2.oci.tar}
environment_file=${GITLAB_ENV_FILE:-${repository}/.env}
stage_host=${STAGE_HOST:-}

sha256_digest() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | awk '{ print $1 }'
  else
    shasum -a 256 "$1" | awk '{ print $1 }'
  fi
}

verify_checksums() {
  if command -v sha256sum >/dev/null 2>&1; then
    (CDPATH='' cd -- "$1" && sha256sum -c checksums.sha256)
  else
    (CDPATH='' cd -- "$1" && shasum -a 256 -c checksums.sha256)
  fi
}

if [ -n "${stage_host}" ]; then
  case "${stage_host}" in
    *[!A-Za-z0-9._-]*)
      printf 'STAGE_HOST contains unsupported characters: %s\n' "${stage_host}" >&2
      exit 2
      ;;
  esac
fi

command -v curl >/dev/null
command -v gdalinfo >/dev/null
command -v gdalwarp >/dev/null
command -v oras >/dev/null
command -v rsync >/dev/null
if [ -n "${stage_host}" ]; then
  command -v ssh >/dev/null
fi

if [ ! -f "${archive}" ]; then
  if [ -z "${GITLAB_TOKEN:-}" ]; then
    if [ ! -f "${environment_file}" ]; then
      printf 'GITLAB_TOKEN is unset and %s does not exist\n' "${environment_file}" >&2
      exit 1
    fi
    GITLAB_TOKEN=$(awk 'index($0, "GITLAB_TOKEN=") == 1 { print substr($0, length("GITLAB_TOKEN=") + 1); exit }' "${environment_file}")
    case "${GITLAB_TOKEN}" in
      \"*\") GITLAB_TOKEN=${GITLAB_TOKEN#\"}; GITLAB_TOKEN=${GITLAB_TOKEN%\"} ;;
      \'*\') GITLAB_TOKEN=${GITLAB_TOKEN#\'}; GITLAB_TOKEN=${GITLAB_TOKEN%\'} ;;
    esac
  fi
  if [ -z "${GITLAB_TOKEN}" ]; then
    printf 'GITLAB_TOKEN is empty\n' >&2
    exit 1
  fi
  mkdir -p "$(dirname -- "${archive}")"
  printf 'Downloading pinned ASTER GDEM bundle\n' >&2
  curl --continue-at - \
    --fail-with-body \
    --header "PRIVATE-TOKEN: ${GITLAB_TOKEN}" \
    --location \
    --output "${archive}.part" \
    --retry 5 \
    --retry-all-errors \
    --retry-delay 2 \
    --silent \
    --show-error \
    "${ASTER_GDEM_PACKAGE_URL}"
  actual_download_sha256=$(sha256_digest "${archive}.part")
  if [ "${actual_download_sha256}" != "${ASTER_GDEM_ARCHIVE_SHA256}" ]; then
    printf 'ASTER GDEM downloaded archive checksum mismatch: expected %s, found %s\n' \
      "${ASTER_GDEM_ARCHIVE_SHA256}" "${actual_download_sha256}" >&2
    exit 1
  fi
  mv "${archive}.part" "${archive}"
fi

actual_archive_sha256=$(sha256_digest "${archive}")
if [ "${actual_archive_sha256}" != "${ASTER_GDEM_ARCHIVE_SHA256}" ]; then
  printf 'ASTER GDEM archive checksum mismatch: expected %s, found %s\n' "${ASTER_GDEM_ARCHIVE_SHA256}" "${actual_archive_sha256}" >&2
  exit 1
fi

actual_oci_digest=$(oras resolve --oci-layout "${archive}:${ASTER_GDEM_OCI_TAG}")
if [ "${actual_oci_digest}" != "${ASTER_GDEM_OCI_DIGEST}" ]; then
  printf 'ASTER GDEM OCI digest mismatch: expected %s, found %s\n' "${ASTER_GDEM_OCI_DIGEST}" "${actual_oci_digest}" >&2
  exit 1
fi

work=$(mktemp -d "${TMPDIR:-/tmp}/xylem-aster-gdem.XXXXXX")
trap 'rm -rf -- "${work}"' EXIT HUP INT TERM
pulled=${work}/pulled
staged=${work}/staged
mkdir -p "${pulled}" "${staged}"
oras pull --oci-layout "${archive}:${ASTER_GDEM_OCI_TAG}" --no-tty --output "${pulled}"
verify_checksums "${pulled}"

cp "${pulled}/granules/ASTGTMV003_N36E071_dem.tif" "${staged}/"
cp "${pulled}/granules/ASTGTMV003_N48E038_dem.tif" "${staged}/"
cp "${pulled}/granules/ASTGTMV003_N48E039_dem.tif" "${staged}/"
cp "${pulled}/granules/ASTGTMV003_N49E038_dem.tif" "${staged}/"
cp "${pulled}/granules/ASTGTMV003_N49E039_dem.tif" "${staged}/"

wv02_mosaic=${staged}/23AUG03081219-M1BS-507797711100_01_P001_merged-DEM.TIF
gdalwarp -q -overwrite -of GTiff \
  "${pulled}/granules/ASTGTMV003_N48E038_dem.tif" \
  "${pulled}/granules/ASTGTMV003_N48E039_dem.tif" \
  "${pulled}/granules/ASTGTMV003_N49E038_dem.tif" \
  "${pulled}/granules/ASTGTMV003_N49E039_dem.tif" \
  "${wv02_mosaic}"
cp "${wv02_mosaic}" "${staged}/23AUG03081219-P1BS-507797711100_01_P001_merged-DEM.TIF"

wv03_mosaic=${staged}/21AUG28055717-M1BS-507458888090_01_P001_merged-DEM.TIF
gdalwarp -q -overwrite -of GTiff \
  "${pulled}/granules/ASTGTMV003_N36E071_dem.tif" \
  "${wv03_mosaic}"
cp "${wv03_mosaic}" "${staged}/21AUG28055717-P1BS-507458888090_01_P001_merged-DEM.TIF"

for raster in "${staged}"/*.TIF "${staged}"/*.tif; do
  gdalinfo "${raster}" >/dev/null
done

for destination in "$@"; do
  case "${destination}" in
    /*) ;;
    *)
      printf 'DEM_DIRECTORY must be absolute: %s\n' "${destination}" >&2
      exit 2
      ;;
  esac
  if [ -n "${stage_host}" ]; then
    ssh "${stage_host}" mkdir -p -- "${destination}"
    rsync --archive --checksum "${staged}/" "${stage_host}:${destination}/"
    printf 'Staged ASTER GDEM data at %s:%s\n' "${stage_host}" "${destination}"
  else
    mkdir -p "${destination}"
    rsync --archive --checksum "${staged}/" "${destination}/"
    printf 'Staged ASTER GDEM data at %s\n' "${destination}"
  fi
done
