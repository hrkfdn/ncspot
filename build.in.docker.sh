docker run \
  --rm -v \
  "$PWD":/usr/src/myapp \
  -e DBUS_SESSION_BUS_ADDRESS="$DBUS_SESSION_BUS_ADDRESS" \
  -v /run/user/$(id -u)/bus:/run/user/$(id -u)/bus \
  -w /usr/src/myapp rust:1-slim \
  bash -c 'apt update \
    && apt install -y \
    libdbus-1-dev \
    libxcb1-dev \
    libxcb1 \
    python3 \
    libssl-dev \
    libpulse-dev \
    libxcb-render0-dev \
    libxcb-shape0-dev \
    libxcb-xfixes0-dev \
    libncursesw5-dev \
    libtinfo-dev  \
    &&  cargo build --release'
