##
FROM rust:slim-buster AS chef
USER root
RUN cargo install cargo-chef
WORKDIR /vSMTP

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

##
FROM chef AS builder
RUN apt-get update && apt-get install -y \
    build-essential

COPY --from=planner /vSMTP/recipe.json recipe.json

RUN cargo chef cook --release --recipe-path recipe.json

COPY . .

RUN cargo build --release    \
    -p vsmtp                    \
    -p vsmtp-plugin-csv         \
    -p vsmtp-plugin-ldap        \
    -p vsmtp-plugin-memcached   \
    -p vsmtp-plugin-mysql

##
FROM debian:buster-slim AS runtime

# RUN apk upgrade --no-cache && apk add --no-cache libc6-compat

RUN addgroup vsmtp && \
    adduser --shell /sbin/nologin --disabled-password \
    --no-create-home --ingroup vsmtp vsmtp

RUN mkdir /var/log/vsmtp/ && chown vsmtp:vsmtp /var/log/vsmtp/ && chmod 755 /var/log/vsmtp/
RUN mkdir /var/spool/vsmtp/ && chown vsmtp:vsmtp /var/spool/vsmtp/ && chmod 755 /var/spool/vsmtp/
RUN mkdir /etc/vsmtp/ && chown vsmtp:vsmtp /etc/vsmtp/ && chmod 755 /etc/vsmtp/
RUN mkdir /etc/vsmtp/plugins && chown vsmtp:vsmtp /etc/vsmtp/plugins && chmod 755 /etc/vsmtp/plugins

COPY --from=builder /vSMTP/target/release/vsmtp /usr/sbin/vsmtp
COPY --from=builder /vSMTP/target/release/*.so /etc/vsmtp/plugins/

USER vsmtp

RUN /usr/sbin/vsmtp --version
CMD ["/usr/sbin/vsmtp", "-c", "/etc/vsmtp/vsmtp.vsl", "--no-daemon", "--stdout"]
