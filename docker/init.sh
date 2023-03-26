#!/bin/bash

. $HOME/.cargo/env

dbus-daemon --system
cargo build --release
cargo deb

if [[ $? != 0 ]]; then
    echo "Build failed."
    exit 1
fi

if [[ "$1" = 'tmux' ]]; then
    tmux
fi
