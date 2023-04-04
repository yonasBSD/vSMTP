FROM clamav/clamav:1.0.1

## Remove the User line of the clamd.conf file
## This is needed to run the clamav as root (do not drop privileges)
RUN sed -i '/User clamav/d' /etc/clamav/clamd.conf
