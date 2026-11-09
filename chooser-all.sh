#!/bin/bash
echo "$(date): chooser called, stdin:" >> /tmp/chooser-debug.log
cat >> /tmp/chooser-debug.log
echo "" >> /tmp/chooser-debug.log

INDEX_FILE="/tmp/chooser-index.txt"
WINDOWS_FILE="/tmp/chooser-windows.txt"
SELECTED_TITLE_FILE="/tmp/chooser-selected-title.txt"

if [ -f "$INDEX_FILE" ]; then
    INDEX=$(cat "$INDEX_FILE" 2>/dev/null)
fi
INDEX=${INDEX:-0}

ALL_LINES=()
while IFS= read -r line; do
    ALL_LINES+=("$line")
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
