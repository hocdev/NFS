#!/bin/bash

output_file="/usr/share/nfs-domain-client/gui_users.json"

if [ ! -f "$output_file" ]; then
    touch "$output_file"
fi

> "$output_file"

logged_in_users=$(users | tr ' ' '\n' | sort | uniq)

user_array=()

for user in $logged_in_users; do
    uid=$(id -u "$user")
    if [ "$uid" -gt 2000 ]; then
        user_array+=("\"$user\"")
    fi
done

json_string="["
json_string+=$(IFS=,; echo "${user_array[*]}")
json_string+="]"

echo "$json_string" > "$output_file"

exit 0