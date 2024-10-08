#!/bin/sh

set -e

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

UNAME=`uname`
LINUX_OPENSSL_DIR="$SCRIPT_DIR/target/openssl"

dl_to_dir() {
  cd $1
  curl -LO $2
  cd - > /dev/null
}

export_build_flags() {
  export OPENSSL_DIR="$1"
  export LD_LIBRARY_PATH="$1/lib"
}

if [ "$UNAME" != 'Linux' ]; then
echo
elif [ -e "$LINUX_OPENSSL_DIR" ]; then
export_build_flags $LINUX_OPENSSL_DIR
else
OPENSSL_VERSION="1.1.1w"
OPENSSL_SUFFIX=`echo "$OPENSSL_VERSION" | tr . _`
OPENSSL_PARENT_DIR="$PWD/target"
OPENSSL_EXTRACT_DIR="$OPENSSL_PARENT_DIR/openssl-$OPENSSL_VERSION"
OPENSSL_URL="https://github.com/openssl/openssl/releases/download/OpenSSL_$OPENSSL_SUFFIX/openssl-$OPENSSL_VERSION.tar.gz"

mkdir -p target && cd target

[ -e "$OPENSSL_EXTRACT_DIR.tar.gz" ] || dl_to_dir $OPENSSL_PARENT_DIR $OPENSSL_URL || exit 1
[ -e "$OPENSSL_EXTRACT_DIR" ] || tar -xzvf $OPENSSL_EXTRACT_DIR.tar.gz -C $OPENSSL_PARENT_DIR || exit 1

cd $OPENSSL_EXTRACT_DIR && \
./config --prefix=$LINUX_OPENSSL_DIR && \
make && \
make install && \
cd $SCRIPT_DIR && \
export_build_flags $LINUX_OPENSSL_DIR
fi

cargo build --release
