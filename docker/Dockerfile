ARG DEBIAN_ARCH=i386
ARG DEBIAN_RELEASE=testing
FROM ${DEBIAN_ARCH}/debian:${DEBIAN_RELEASE}
ARG RUST_ARCH=i686-unknown-linux-gnu
ENV DEBIAN_FRONTEND noninteractive

RUN apt-get update && apt-get install -y --no-install-recommends \
	ca-certificates curl \
	build-essential \
	libgpgme11-dev \
	libarchive-dev \
	libgstreamer1.0-dev \
	libpango1.0-dev \
	libcairo2-dev \
	libgdk-pixbuf2.0-dev \
	libxml2-dev \
	fonts-lmodern \
	sqlite3 \
	dbus systemd udev tmux

RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- --default-host ${RUST_ARCH} -y

RUN . $HOME/.cargo/env ; cargo install cargo-deb

RUN set -ex && \
    useradd shop && \
    mkdir -p /var/run/dbus

ENV SBC_DIR /mnt/ktt-shopsystem
WORKDIR $SBC_DIR

COPY tmux-config /root/.tmux.conf
COPY init.sh /root/

ENTRYPOINT ["/root/init.sh"]
