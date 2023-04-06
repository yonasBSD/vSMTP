##
FROM clux/muslrust:1.68.0-stable AS chef
USER root
RUN cargo install cargo-chef@0.1.51
WORKDIR /vSMTP

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

##
FROM chef AS builder
RUN apt-get update && apt-get install -y \
    build-essential \
    musl-dev
ENV RUSTFLAGS="-C target-feature=-crt-static"

COPY --from=planner /vSMTP/recipe.json recipe.json

RUN cargo chef cook --release --target x86_64-unknown-linux-musl --recipe-path recipe.json

COPY . .

RUN cargo build --target x86_64-unknown-linux-musl --release    \
    -p vsmtp                    \
    -p vsmtp-plugin-csv         \
    -p vsmtp-plugin-ldap        \
    -p vsmtp-plugin-memcached   \
    -p vsmtp-plugin-mongodb     \
    -p vsmtp-plugin-redis       \
    -p vsmtp-plugin-mysql

##
FROM alpine AS runtime

RUN apk upgrade --no-cache && apk add --no-cache libc6-compat

RUN addgroup vsmtp && \
    adduser --shell /sbin/nologin --disabled-password \
    --no-create-home --ingroup vsmtp vsmtp

RUN mkdir /var/log/vsmtp/ && chown vsmtp:vsmtp /var/log/vsmtp/ && chmod 755 /var/log/vsmtp/
RUN mkdir /var/spool/vsmtp/ && chown vsmtp:vsmtp /var/spool/vsmtp/ && chmod 755 /var/spool/vsmtp/
RUN mkdir /etc/vsmtp/ && chown vsmtp:vsmtp /etc/vsmtp/ && chmod 755 /etc/vsmtp/
RUN mkdir /etc/vsmtp/plugins && chown vsmtp:vsmtp /etc/vsmtp/plugins && chmod 755 /etc/vsmtp/plugins

COPY --from=builder /vSMTP/target/x86_64-unknown-linux-musl/release/vsmtp /usr/sbin/vsmtp
COPY --from=builder /vSMTP/target/x86_64-unknown-linux-musl/release/*.so /etc/vsmtp/plugins/

USER vsmtp

RUN /usr/sbin/vsmtp --version
CMD ["/usr/sbin/vsmtp", "-c", "/etc/vsmtp/vsmtp.vsl", "--no-daemon", "--stdout"]
