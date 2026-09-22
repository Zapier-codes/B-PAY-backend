# Bakes the Daml SDK into the image at build time, rather than relying on it
# being installed live wherever this project later builds/runs. That matters
# concretely: this repo's own dev sandbox (as of 2026-09-22) cannot reach
# get.daml.com at all -- its outbound network is allowlisted to a fixed set of
# package registries that doesn't include Digital Asset's install host -- so
# "install on demand" isn't something every environment running this image can
# actually do. Baking it in here means the image itself carries a working
# `daml` toolchain regardless of the network the container later runs with.
FROM eclipse-temurin:17-jdk-jammy AS daml-sdk

# https://github.com/digital-asset/daml/releases -- kept in sync with
# canton/daml.yaml's sdk-version; bump both together.
ARG DAML_SDK_VERSION=2.10.0

RUN apt-get update \
    && apt-get install -y --no-install-recommends curl ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Official Daml Connect/SDK installer. Needs network access to get.daml.com at
# *image build* time only -- once baked in, containers run from this image
# need no further access to it.
RUN curl -sSL https://get.daml.com/ | sh -s "${DAML_SDK_VERSION}"
ENV PATH="/root/.daml/bin:${PATH}"

WORKDIR /canton
COPY . .

# Compiles this project's .daml sources into a .dar using the SDK baked in
# above. NOT verified against this image in this sandbox (see canton/README.md
# and Instrument.daml) -- this build step, and the daml-finance
# data-dependencies daml.yaml notes as not-yet-wired, both still need a real
# run to confirm.
RUN daml build
