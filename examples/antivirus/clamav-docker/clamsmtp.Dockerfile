FROM debian:stable-slim

RUN apt-get update && \
    apt-get install -y clamsmtp && \
    apt-get clean && \
    rm -rf /var/lib/apt/lists/*

RUN sed -i 's/^Listen: .*$/Listen: 0.0.0.0:10026/g' /etc/clamsmtpd.conf
RUN echo "Action: pass" >> /etc/clamsmtpd.conf

RUN sed -i 's/^User: .*$/User: clamav/g' /etc/clamsmtpd.conf
RUN sed -i 's/^OutAddress: .*$/OutAddress: vsmtp.example.tld:10025/g' /etc/clamsmtpd.conf
RUN sed -i 's/^ClamAddress: .*$/ClamAddress: av.example.tld:3310/g' /etc/clamsmtpd.conf

RUN mkdir -p /var/spool/clamsmtp/
RUN chown clamav:clamav /var/spool/clamsmtp/

CMD ["/bin/bash", "-c", "cat /etc/clamsmtpd.conf && clamsmtpd -d 4 -f /etc/clamsmtpd.conf"]
