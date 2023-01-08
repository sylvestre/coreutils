#!/bin/sh

# spell-checker:ignore (utils) sccache SCCACHE
ARCH=$1

version=v0.4.0-pre.5
if [ "$ARCH" = "ubuntu-latest" ]; then
  # ex: sccache-v0.4.0-pre.5-x86_64-unknown-linux-musl.tar.gz
  sccache_platform=x86_64-unknown-linux-musl
elif [ "$ARCH" = "macos-latest" ]; then
  # ex: sccache-v0.4.0-pre.5-x86_64-apple-darwin.tar.gz
  sccache_platform=x86_64-apple-darwin
elif [ "$ARCH" = "windows-latest" ]; then
  # ex: sccache-v0.4.0-pre.5-x86_64-pc-windows-msvc.tar.gz
  sccache_platform=x86_64-pc-windows-msvc
fi
sccache_url="https://github.com/mozilla/sccache/releases/download/${version}/sccache-${version}-${sccache_platform}.tar.gz"
echo "URL=${sccache_url}"
wget -q -c "${sccache_url}" -O - | tar -xz
ls -al sccache-*/
mv sccache-*/sccache* ${SCCACHE_PATH}
chmod +x ${SCCACHE_PATH}
${SCCACHE_PATH} --version
