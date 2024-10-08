#!/bin/sh

CURRENT_DIR=$PWD
# locate
if [ -z "$BASH_SOURCE" ]; then
    SCRIPT_DIR=`dirname "$(readlink -f $0)"`
elif [ -e '/bin/zsh' ]; then
    F=`/bin/zsh -c "print -lr -- $BASH_SOURCE(:A)"`
    SCRIPT_DIR=`dirname $F`
elif [ -e '/usr/bin/realpath' ]; then
    F=`/usr/bin/realpath $BASH_SOURCE`
    SCRIPT_DIR=`dirname $F`
else
    F=$BASH_SOURCE
    while [ -h "$F" ]; do F="$(readlink $F)"; done
    SCRIPT_DIR=`dirname $F`
fi

cd $SCRIPT_DIR

EPG_BIN='target/release/epg'
[ -e $EPG_BIN ] || ./build.sh

UNAME=`uname`
[ "$UNAME" = 'Linux' ] && export LD_LIBRARY_PATH="$SCRIPT_DIR/target/openssl/lib"

export PGDIR='target/postgresql'
mkdir -p $PGDIR

PGVERSION=17.0.0 PGPORT=5017 ./$EPG_BIN
