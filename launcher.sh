#!/bin/bash
# launcher.sh - Robustly launch applications by AppId or command name

APP_ID="$1"

if [ -z "$APP_ID" ]; then
    echo "Usage: $0 <app_id>"
    exit 1
fi

# 0. Special handler for custom local binaries like taskman
if [ "$APP_ID" = "taskman" ]; then
    if [ -x "./taskman" ]; then
        ./taskman >/dev/null 2>&1 &
        exit 0
    fi
fi

# 0b. Check if this is a Steam AppID (e.g. steam_icon_730 or steam_app_730)
if [[ "$APP_ID" =~ ^steam_(icon|app)_([0-9]+)$ ]]; then
    APPID="${BASH_REMATCH[2]}"
    steam "steam://rungameid/${APPID}" >/dev/null 2>&1 &
    exit 0
fi

# 1. Try gtk-launch (best for AppIds/Desktop IDs)
if command -v gtk-launch >/dev/null 2>&1; then
    if gtk-launch "$APP_ID" >/dev/null 2>&1; then
        exit 0
    fi
fi

# 2. Try as a direct command
if command -v "$APP_ID" >/dev/null 2>&1; then
    "$APP_ID" &
    exit 0
fi

# 3. Try stripping prefixes (e.g., org.xfce.mousepad -> mousepad)
if [[ "$APP_ID" == *.* ]]; then
    SHORT_NAME="${APP_ID##*.}"
    if command -v "$SHORT_NAME" >/dev/null 2>&1; then
        "$SHORT_NAME" &
        exit 0
    fi
fi

echo "Failed to launch $APP_ID"
exit 1