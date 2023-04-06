FROM redis

USER root
RUN apt-get update && apt-get install -y netcat && apt-get clean

HEALTHCHECK --start-period=3s --timeout=2s --interval=10s CMD ["nc", "-z", "127.0.0.1", "6379"]
