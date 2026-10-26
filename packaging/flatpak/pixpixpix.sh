#!/bin/sh
# The Flatpak's command: starts pixpixpix (in ../libexec) with the host's data
# dir. Flatpak points XDG_DATA_HOME into the app's private ~/.var/app directory,
# and the trash lives in the data dir: without this, x would move files into a
# trash no file manager shows instead of ~/.local/share/Trash.
XDG_DATA_HOME="${HOST_XDG_DATA_HOME:-$HOME/.local/share}"
export XDG_DATA_HOME
exec "${0%/*}/../libexec/pixpixpix" "$@"
