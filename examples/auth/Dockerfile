FROM viridit/vsmtp:unstable

USER root

## saslauthd is used internally by vsmtp
## openssl is **only used** to create a new user (not needed by vsmtp)
RUN apt-get update && apt-get install -y \
    sasl2-bin   \
    openssl

RUN saslauthd -a shadow

RUN useradd -p $(openssl passwd -1 example-user-password) example-username

RUN usermod -a -G sasl vsmtp
USER vsmtp
