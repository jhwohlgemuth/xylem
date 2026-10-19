#!/bin/sh
set -eu

PATH="/usr/sbin:$PATH"
export PATH
if ! command -v sshd >/dev/null 2>&1 || ! command -v ssh-keygen >/dev/null 2>&1; then
  if command -v apt-get >/dev/null 2>&1; then
    apt-get update
    DEBIAN_FRONTEND=noninteractive apt-get install -y openssh-server
  elif command -v apk >/dev/null 2>&1; then
    apk add --no-cache openssh
  else
    echo "OpenSSH server and ssh-keygen are required for acquisition tests" >&2
    exit 1
  fi
fi
mkdir -p /run/sshd

version=1.3.4
case "$(uname -m)" in
  x86_64) architecture=amd64; checksum=f27adb935022d94df8dc77719c322dda592c78a0d57a6f7dcdd8d900b248c454 ;;
  aarch64) architecture=arm64; checksum=15702c6e3a4a56a8bd8ac5c17efdbcab56d9bada661ccbcf017f5b10c1d89399 ;;
  *) echo "unsupported ORAS CI architecture" >&2; exit 1 ;;
esac
destination=/tmp/xylem-ci-bin
mkdir -p "$destination"
archive="$destination/oras_${version}_linux_${architecture}.tar.gz"
curl -LsSf --output "$archive" "https://github.com/oras-project/oras/releases/download/v${version}/oras_${version}_linux_${architecture}.tar.gz"
printf '%s  %s\n' "$checksum" "$archive" | sha256sum --check -
tar -xzf "$archive" -C "$destination" oras
"$destination/oras" version
