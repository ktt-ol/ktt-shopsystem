FROM i386/debian:stable
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

RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- --default-host i686-unknown-linux-gnu -y

RUN set -ex && \
    useradd shop && \
    mkdir /var/run/dbus

ENV SBC_DIR /mnt/ktt-shopsystem
WORKDIR $SBC_DIR

COPY tmux-config /root/.tmux.conf
COPY init.sh /root/

ENTRYPOINT ["/root/init.sh"]
