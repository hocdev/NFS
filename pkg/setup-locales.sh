#!/bin/sh
# Executed by systemd before the first graphical login, in the Linux guest.
set -eu
export PATH=/usr/sbin:/usr/bin:/sbin:/bin
export LC_ALL=C
export DEBIAN_FRONTEND=noninteractive
mkdir -p /var/lib/isolite
# Use dpkg so maintainer scripts, permissions and package records are preserved.
# All dependencies must be provided offline; never change the vendor APT sources.
set -- /usr/share/isolite/language-packs/*.deb
if [ -f "$1" ]; then
    dpkg -i -- "$@"
fi
while IFS= read -r locale; do
    [ -n "$locale" ] || continue
    base=${locale%.UTF-8}
    localedef -i "$base" -f UTF-8 "$locale"
done < /usr/share/isolite/locales
touch /var/lib/isolite/locales-ready
