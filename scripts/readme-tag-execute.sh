#!/bin/bash

if [ "$#" -ne 1 ]; then
  echo "Usage: $0 <tag-prefix>"
  exit 1
fi

tag_prefix="$1"

steps=()
while IFS= read -r line; do
  if [[ "$line" == "[$tag_prefix"* ]]; then
    tag="$line"
    IFS= read -r cmd_line
    if [[ "$cmd_line" == \`\`\`bash ]]; then
      cmd=""
      while IFS= read -r cmd_line && [[ "$cmd_line" != \`\`\` ]]; do
        cmd+="$cmd_line"$'\n'
      done
      steps+=("$cmd")
    fi
  fi
done < README.md

for step in "${steps[@]}"; do
  echo "Executing: $step"
  eval "$step"
done
