#!/bin/sh
set -e

if [ "$(id -u)" = "0" ]; then
    storage_root="${TULPAR__STORAGE__ROOT:-/home/tulpar/uploads}"
    mkdir -p "$storage_root"
    chown -R tulpar:tulpar "$storage_root"
    exec gosu tulpar "$@"
fi

exec "$@"
