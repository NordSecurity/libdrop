FROM ubuntu:22.04

LABEL org.opencontainers.image.source=https://github.com/NordSecurity/libdrop

RUN \
    export DEBIAN_FRONTEND=noninteractive && \
    apt-get update && \
    apt-get install -y python3 iproute2 python3-requests

COPY prepare_files.sh /usr/local/bin/prepare_files.sh
RUN /usr/local/bin/prepare_files.sh
