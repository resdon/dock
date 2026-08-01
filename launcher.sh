#!/bin/bash
# launcher.sh - Robustly launch applications by AppId or command name with optional file arguments

APP_ID="$1"

if [ -z "$APP_ID" ]; then
    echo "Usage: $0 <app_id> [file1 file2 ...]"
    exit 1
fi

# Remove APP_ID from parameters so "$@" only contains the file paths
shift

# Helper function to execute completely detached from the current process session
run_detached() {
    if command -v setsid >/dev/null 2>&1; then
        setsid -f "$@" >/dev/null 2>&1
    else
        nohup "$@" >/dev/null 2>&1 &
        disown
    fi
}

# 0. Special handler for custom local binaries like taskman
if [ "$APP_ID" = "taskman" ]; then
    if [ -x "./taskman" ]; then
        run_detached ./taskman "$@"
        exit 0
    fi
fi

# 0b. Check if this is a Steam AppID (e.g. steam_icon_730 or steam_app_730)
if [[ "$APP_ID" =~ ^steam_(icon|app)_([0-9]+)$ ]]; then
    APPID="${BASH_REMATCH[2]}"
    run_detached steam "steam://rungameid/${APPID}"
    exit 0
fi

# 1. Try gtk-launch (gtk-launch supports passing files: gtk-launch <desktop-id> [files...])
# Note: gtk-launch usually delegates spawning to D-Bus/systemd user services, but detaching
# prevents gtk-launch itself from holding onto the parent shell's FDs.
if command -v gtk-launch >/dev/null 2>&1; then
    run_detached gtk-launch "$APP_ID" "$@"
    exit 0
fi

# 2. Try as a direct command
if command -v "$APP_ID" >/dev/null 2>&1; then
    run_detached "$APP_ID" "$@"
    exit 0
fi

# 3. Try stripping prefixes (e.g., org.xfce.mousepad -> mousepad)
if [[ "$APP_ID" == *.* ]]; then
    SHORT_NAME="${APP_ID##*.}"
    if command -v "$SHORT_NAME" >/dev/null 2>&1; then
        run_detached "$SHORT_NAME" "$@"
        exit 0
    fi
fi

echo "Failed to launch $APP_ID"
exit 1