# Docker How-to

This directory regroups `Dockerfiles` used to run instances of vSMTP and build / test packages automatically.

## Run vSMTP

The `Dockerfile` available in this repository download, builds and run a single instance of the latest version of vSMTP in a `rust:alpine` image with a minimal configuration setup. to run the instance, simply execute the following command.

```sh
docker run -it viridit/vsmtp:v2.0.0
```

## Build for a specific linux distribution

See the `debian`, `ubuntu` and `redhat` directories.
