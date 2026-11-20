#!/bin/sh
FALSO_MODO=ar exec node "$(dirname "$0")/falso-cc.mjs" "$@"
