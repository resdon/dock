#!/bin/bash
# XDG Desktop Portal chooser script for PipeWire window capture.
#
# Called by the wlr portal (via chooser_cmd in ~/.config/xdg-desktop-portal-wlr/config)
# when the dock requests screen capture permission. Instead of showing a dialog,
# this script auto-selects the window at the index specified in INDEX_FILE.
#
# The portal passes all available windows on stdin in the format:
#   "Window: <title> (output: <output>)"
#
# The script:
# 1. Reads the desired window index from /tmp/chooser-index.txt
# 2. Extracts all window titles from stdin
# 3. Writes all titles to /tmp/chooser-windows.txt (used by the dock for matching)
# 4. Outputs the selected window line to stdout (portal reads this as the choice)
#
# Files:
#   /tmp/chooser-index.txt       — written by dock before each portal session
#   /tmp/chooser-windows.txt     — written by this script, read by dock for titles
#   /tmp/chooser-selected-title.txt — written by this script with the selected title
#   /tmp/chooser-debug.log       — debug log of chooser invocations

INDEX_FILE="/tmp/chooser-index.txt"
WINDOWS_FILE="/tmp/chooser-windows.txt"
SELECTED_TITLE_FILE="/tmp/chooser-selected-title.txt"

if [ -f "$INDEX_FILE" ]; then
    INDEX=$(cat "$INDEX_FILE" 2>/dev/null)
fi
INDEX=${INDEX:-0}

# Read all stdin lines into array
ALL_LINES=()
while IFS= read -r line; do
    ALL_LINES+=("$line")
done

# Log what we received
echo "$(date): chooser called, ${#ALL_LINES[@]} lines" >> /tmp/chooser-debug.log
for line in "${ALL_LINES[@]}"; do
    echo "$line" >> /tmp/chooser-debug.log
done

WINDOWS=()
for line in "${ALL_LINES[@]}"; do
    if [[ "$line" == Window:* ]]; then
        WINDOWS+=("$line")
    fi
done

> "$WINDOWS_FILE"
for win in "${WINDOWS[@]}"; do
    TITLE="${win#Window: }"
    echo "$TITLE" >> "$WINDOWS_FILE"
done

WINDOW_COUNT=${#WINDOWS[@]}
echo "$(date): Found $WINDOW_COUNT windows, selecting index $INDEX" >> /tmp/chooser-debug.log

if [ "$INDEX" -lt "$WINDOW_COUNT" ]; then
    SELECTED="${WINDOWS[$INDEX]}"
    echo "$SELECTED"
    SELECTED_TITLE="${SELECTED#Window: }"
    echo "$SELECTED_TITLE" > "$SELECTED_TITLE_FILE"
    echo "$(date): Selected: $SELECTED_TITLE" >> /tmp/chooser-debug.log
    exit 0
fi

echo "$(date): Index $INDEX out of range" >> /tmp/chooser-debug.log
exit 1
