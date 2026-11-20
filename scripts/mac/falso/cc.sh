#!/bin/sh
FALSO_MODO=cc exec node "$(dirname "$0")/falso-cc.mjs" "$@"
