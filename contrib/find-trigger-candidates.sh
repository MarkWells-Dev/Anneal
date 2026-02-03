#!/bin/sh
#
# find-trigger-candidates.sh - Analyze reverse dependencies to find trigger candidates
#
# This script counts reverse dependencies for all installed packages on your
# system. Packages with high reverse dependency counts are potential candidates
# for the Anneal curated trigger list.
#
# Usage:
#   ./find-trigger-candidates.sh [OPTIONS] [OUTPUT_FILE]
#
# Options:
#   -f    Force overwrite if output file already exists
#   -h    Show this help message
#
# Arguments:
#   OUTPUT_FILE   Path to output file (default: dep-count.txt)
#
# Output format:
#   Each line contains: <reverse_dep_count> <package_name>
#   Sorted by reverse dependency count (highest first)
#
# Example:
#   ./find-trigger-candidates.sh                    # Output to dep-count.txt
#   ./find-trigger-candidates.sh results.txt       # Custom output file
#   ./find-trigger-candidates.sh -f                # Overwrite existing file
#   ./find-trigger-candidates.sh -f my-results.txt # Both options
#
# Requirements:
#   - pacman (obviously)
#   - pactree (from pacman-contrib)
#   - pv (optional, for progress bar)
#
# Note: This can take several minutes depending on how many packages you have
# installed. The script runs dependency checks in parallel to speed things up.
#

set -e

# Defaults
OUTPUT_FILE="dep-count.txt"
FORCE=0

# Parse options
while getopts "fh" opt; do
    case "$opt" in
        f) FORCE=1 ;;
        h)
            sed -n '2,/^$/p' "$0" | sed 's/^# \?//'
            exit 0
            ;;
        *)
            echo "Usage: $0 [-f] [-h] [OUTPUT_FILE]" >&2
            exit 1
            ;;
    esac
done
shift $((OPTIND - 1))

# Positional argument for output file
if [ -n "$1" ]; then
    OUTPUT_FILE="$1"
fi

# Check if output file exists
if [ -e "$OUTPUT_FILE" ] && [ "$FORCE" -eq 0 ]; then
    echo "Error: $OUTPUT_FILE already exists. Use -f to overwrite." >&2
    exit 1
fi

# Check for pactree
if ! command -v pactree >/dev/null 2>&1; then
    echo "Error: pactree not found. Install pacman-contrib." >&2
    exit 1
fi

# Count packages
total=$(pacman -Qq | wc -l)
echo "Scanning $total packages for reverse dependencies..."
echo "Output will be written to: $OUTPUT_FILE"

# Run the analysis
# shellcheck disable=SC2016 # Single quotes intentional - xargs replaces {} before sh -c runs
if command -v pv >/dev/null 2>&1; then
    # Use pv for progress bar if available
    pacman -Qq | pv -l -s "$total" | xargs -I {} -P "$(nproc)" sh -c \
        'echo "$(pactree -r -u "{}" 2>/dev/null | wc -l) {}"' | sort -rn > "$OUTPUT_FILE"
else
    # Fall back to simple counter
    echo "(Install 'pv' for a progress bar)"
    pacman -Qq | xargs -I {} -P "$(nproc)" sh -c \
        'echo "$(pactree -r -u "{}" 2>/dev/null | wc -l) {}"' | sort -rn > "$OUTPUT_FILE"
fi

echo "Done. Results written to $OUTPUT_FILE"
echo "Top 10 packages by reverse dependency count:"
head -10 "$OUTPUT_FILE"
