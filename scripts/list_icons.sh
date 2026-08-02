#!/bin/bash

SEARCH_DIRS=(
    "/usr/share/icons"
    "$HOME/.local/share/icons"
    "$HOME/.local/share/flatpak/appstream/flathub/x86_64/724b0f962e2504fe47cd53d7fce5085e518ca2fed6e66dd4444293d6b93f277d/icons"
    "/usr/share/pixmaps"
    "$HOME/.local/share/pixmaps"
)

# Allow passing a target output file as argument $1, fallback to local icon_list.txt
OUTPUT_FILE="${1:-icon_list.txt}"

# Create parent directory if needed
mkdir -p "$(dirname "$OUTPUT_FILE")"
> "$OUTPUT_FILE"

TOTAL_COUNT=0

printf "%-30s | %s\n" "Filename" "Full Path" >> "$OUTPUT_FILE"
echo "--------------------------------------------------------------------------" >> "$OUTPUT_FILE"

for dir in "${SEARCH_DIRS[@]}"; do
    if [ -d "$dir" ]; then
        while IFS=$'\t' read -r filename filepath; do
            printf "%-30s | %s\n" "$filename" "$filepath" >> "$OUTPUT_FILE"
            ((TOTAL_COUNT++))
        done < <(find "$dir" -type f \( -name "*.png" -o -name "*.svg" \) -printf "%f\t%p\n")
    fi
done

echo "--------------------------------------------------------------------------" >> "$OUTPUT_FILE"
echo "Total icons (.png and .svg) found: $TOTAL_COUNT" >> "$OUTPUT_FILE"

echo "Results saved to $OUTPUT_FILE"