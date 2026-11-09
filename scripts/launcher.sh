#!/bin/bash
# launcher.sh - Robustly launch applications by AppId or command name with optional file arguments

APP_ID="$1"

# The dock may itself be launched with toolkit-specific scale overrides. Those
# are intended for the dock process only; leaking them into applications makes
# GTK/Qt controls and text appear magnified. Let applications use the compositor
# output scale by default. Set DOCK_PRESERVE_APP_SCALE=1 to opt out.
if [ "${DOCK_PRESERVE_APP_SCALE:-0}" != "1" ]; then
    unset GDK_SCALE GDK_DPI_SCALE GTK_SCALE GTK_DPI_SCALE \
        QT_SCALE_FACTOR QT_SCREEN_SCALE_FACTORS QT_AUTO_SCREEN_SCALE_FACTOR \
        QT_ENABLE_HIGHDPI_SCALING QT_FONT_DPI QT_DEVICE_PIXEL_RATIO \
        ELM_SCALE WINIT_X11_SCALE_FACTOR FLTK_SCALING
    # Do not let toolkit defaults reload an inherited desktop scale setting.
    # The compositor continues to scale application surfaces for the output.
    export GDK_SCALE=1 GDK_DPI_SCALE=1
    export QT_SCALE_FACTOR=1 QT_AUTO_SCREEN_SCALE_FACTOR=0
fi

if [ "${DOCK_LAUNCH_DEBUG:-0}" = "1" ]; then
    printf '[DOCK-LAUNCH] script=%s app=%s\n' "$0" "$APP_ID" >&2
    for name in GDK_SCALE GDK_DPI_SCALE QT_SCALE_FACTOR QT_SCREEN_SCALE_FACTORS QT_FONT_DPI QT_DEVICE_PIXEL_RATIO; do
        if printenv "$name" >/dev/null 2>&1; then
            printf '[DOCK-LAUNCH] WARNING: %s remains set to %s\n' "$name" "$(printenv "$name")" >&2
        fi
    done
fi

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

# Locate the desktop file belonging to this application. The launcher uses
# this only for DBusActivatable entries: gtk-launch normally delegates those to
# the session/systemd activation environment, which can re-introduce toolkit
# scale overrides that were removed from this process.
find_desktop_entry() {
    local desktop_id="$1"
    desktop_id="${desktop_id%.desktop}"
    local data_home="${XDG_DATA_HOME:-$HOME/.local/share}"
    local data_dirs="${XDG_DATA_DIRS:-/usr/local/share:/usr/share}"
    local candidate base

    candidate="$data_home/applications/$desktop_id.desktop"
    if [ -f "$candidate" ]; then
        printf '%s\n' "$candidate"
        return 0
    fi

    local old_ifs="$IFS"
    IFS=:
    for base in $data_dirs; do
        candidate="$base/applications/$desktop_id.desktop"
        if [ -f "$candidate" ]; then
            IFS="$old_ifs"
            printf '%s\n' "$candidate"
            return 0
        fi
    done
    IFS="$old_ifs"
    return 1
}

is_dbus_activatable_with_exec() {
    awk '
        /^\[Desktop Entry\][[:space:]]*$/ { in_entry=1; next }
        /^\[/ { if (in_entry) in_entry=0 }
        in_entry && /^DBusActivatable[[:space:]]*=[[:space:]]*true[[:space:]]*$/ { dbus=1 }
        in_entry && /^Exec[[:space:]]*=/ { exec=1 }
        END { exit !(dbus && exec) }
    ' "$1"
}

launch_dbus_entry_without_session_activation() {
    local desktop_file="$1"
    shift
    local runtime_base="${XDG_RUNTIME_DIR:-${TMPDIR:-/tmp}}"
    local temp_root temp_id temp_file data_dirs

    temp_root="$(mktemp -d "$runtime_base/dock-launch.XXXXXX")" || return 1
    mkdir -p "$temp_root/applications" || { rm -rf "$temp_root"; return 1; }
    temp_id="dock-sanitized-${APP_ID//[^A-Za-z0-9._-]/_}-$$"
    temp_file="$temp_root/applications/$temp_id.desktop"

    # Keep the original Exec/Name/Icon fields but change only DBusActivatable.
    # The temporary desktop ID ensures gtk-launch chooses this copy even when
    # the original entry lives in XDG_DATA_HOME.
    awk '
        BEGIN { in_entry=0; wrote=0 }
        /^\[Desktop Entry\][[:space:]]*$/ { in_entry=1; print; next }
        /^\[/ {
            if (in_entry && !wrote) print "DBusActivatable=false"
            in_entry=0
            print
            next
        }
        in_entry && /^DBusActivatable[[:space:]]*=/ {
            print "DBusActivatable=false"
            wrote=1
            next
        }
        { print }
        END { if (in_entry && !wrote) print "DBusActivatable=false" }
    ' "$desktop_file" > "$temp_file" || { rm -rf "$temp_root"; return 1; }

    data_dirs="${XDG_DATA_DIRS:-/usr/local/share:/usr/share}"
    if [ "${DOCK_LAUNCH_DEBUG:-0}" = "1" ]; then
        printf '[DOCK-LAUNCH] bypassing DBus activation for %s to keep the sanitized toolkit environment\n' "$APP_ID" >&2
    fi

    # This launcher script itself is detached by the Rust dock. Run gtk-launch
    # synchronously here so the temporary desktop entry can be removed once it
    # has parsed and spawned the application's Exec command.
    XDG_DATA_DIRS="$temp_root:$data_dirs" gtk-launch "$temp_id" "$@"
    local result=$?
    # Keep the temporary desktop file around briefly in case the application
    # reads GIO_LAUNCHED_DESKTOP_FILE after its process has started.
    ( sleep 30; rm -rf "$temp_root" ) >/dev/null 2>&1 &
    return "$result"
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
    desktop_file="$(find_desktop_entry "$APP_ID" 2>/dev/null || true)"
    if [ -n "$desktop_file" ] && is_dbus_activatable_with_exec "$desktop_file"; then
        launch_dbus_entry_without_session_activation "$desktop_file" "$@"
        exit $?
    fi
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
